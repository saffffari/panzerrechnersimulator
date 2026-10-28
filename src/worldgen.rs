//! worldgen.rs — the ACT-SCALE in-engine world generator (Rust port of tools/worldgen/gen_world.py,
//! which stays only as the reference spec). Generates the full act map as INSTANCED placements:
//! a ruined city district of kitbashed PSX modules WITH walkable interiors (partition walls,
//! doorways, furniture, stairs, shell-holed upper floors), a countryside fringe (villages, pines),
//! an underground bunker (Retro FPS kit), and street rubble — thousands of placements of a few
//! dozen unique modules, spatially bucketed so `Renderer::add_instanced` batches cull well.
//!
//! Scope notes (v1): purely VISUAL/static. The hand-tuned combat block from `city::generate` keeps
//! its LOS occluders / destructible walls / emissive windows; this district surrounds it as the
//! act-scale world. Occluders + destructibility for the wide district come later (they need the
//! stealth system's cost model revisited at scale). Deterministic: counter-hash, no `rand`.

#![allow(dead_code)] // retained as an explicit dev/test generator, never a campaign fallback

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use glam::{Mat4, Quat, Vec3};

use crate::gltf_load::CpuMesh;
use crate::terrain::Heightfield;
#[path = "manifest_wreck_collision.rs"]
mod manifest_wreck_collision;
pub(crate) use manifest_wreck_collision::StaticTankCollider;

// ── Module kit (paths verified on disk; MISSING assets are logged and skipped, never fatal) ──────
const MP2: &str =
    "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/";
const MP: &str =
    "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/";
const NAT: &str = "assets/pizza/_extracted/PSX Nature v1.7.3/PSX Nature (Tree branches separated)/Models/GLB (recommended)/";
const FPS: &str = "assets/pizza/_extracted/Modular Retro FPS Kit v1.5/Modular Retro FPS Kit/Models/other-formats/GLB/";

/// The kit keys. Paths assembled in `path_of` — one place to retarget an asset.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kit {
    Wall,
    WallWin,
    WallHole,
    Doorway,
    Window,
    Floor,
    FloorHole,
    RoofPitch,
    RoofFlat,
    Pillar,
    PillarBroken,
    Stairs,
    RubbleField,
    RubbleStack,
    Brick,
    Warehouse,
    Garage,
    Shed,
    FurnBed,
    FurnTable,
    FurnChair,
    FurnCabinet,
    Pine,
    Pine2,
    Stone,
    FpsWall,
    FpsFloor,
    FpsStairs,
    FpsTunnel,
    FpsTunnelEnd,
    FpsCrate,
    FpsBarrel,
    // Environmental-storytelling vocabulary. These are deliberately small in number: each
    // module is instanced many times, so a tableau can read as specific without becoming a
    // draw-call-per-prop scene. Every path below was verified against the shipped asset library.
    Gravel,
    Bench,
    TrashCan,
    Bucket,
    TinCan,
    SupplyCrate,
    EmptySupplyCrate,
    WoodCrate,
    Board,
    Plank,
    Luggage,
    Bottle,
    Radio,
    FirstAid,
    Jerrycan,
    Toolbox,
    Tire,
    Axe,
    Shovel,
    HandSaw,
    Meat,
    CannedFood,
    MailBox,
    Grass,
    Grass2,
    Stump,
    Fence,
    TelegraphPole,
    HandCart,
    Barricade,
    Chimney,
    Deadman,
    GraveCross,
    WreckT34,
    WreckPz3,
    WreckPanther,
    WreckTiger2,
    WreckIs2,
    WreckKv1,
    // Seasonal variants retain the Pizza woody root while omitting its separately authored leaves.
    BareTree5,
    BareTree7,
    DeadTree8,
    DeadTree9,
    FallenLog,
    Stone2,
    Stone3,
    Fern,
    Fern2,
}

fn path_of(k: Kit) -> String {
    use Kit::*;
    match k {
        Wall => format!("{MP2}Modular Structures/wall_hr_1.glb"),
        WallWin => format!("{MP2}Modular Structures/wall_hr_1_m1a_1.glb"),
        WallHole => format!("{MP2}Modular Structures/wall_hr_1_hole_1.glb"),
        Doorway => format!("{MP2}Modular Structures/doorway_hr_1.glb"),
        Window => format!("{MP2}Modular Structures/window_hr_1.glb"),
        Floor => format!("{MP2}Modular Structures/floor_ceiling_hr_1.glb"),
        FloorHole => format!("{MP2}Modular Structures/floor_ceiling_hr_2_hole.glb"),
        RoofPitch => format!("{MP2}Modular Structures/roof_hr_3_middle_angle.glb"),
        RoofFlat => format!("{MP2}Modular Structures/roof_mh_4_a_1.glb"),
        Pillar => format!("{MP2}Modular Structures/pillar_hr_2.glb"),
        PillarBroken => format!("{MP2}Modular Structures/pillar_hr_1_broken.glb"),
        Stairs => format!("{MP}Modular Structures/stairs_5_concrete.glb"),
        RubbleField => format!("{MP2}Debris & Misc/debris_bricks_mx_1.glb"),
        RubbleStack => format!("{MP2}Debris & Misc/bricks_stacked_mx_1.glb"),
        Brick => format!("{MP2}Debris & Misc/brick_mx_1.glb"),
        Warehouse => format!("{MP2}Buildings/warehouse_mx_1.glb"),
        Garage => format!("{MP2}Buildings/garage_hl_1.glb"),
        Shed => format!("{MP2}Buildings/shed_ax_1.glb"),
        FurnBed => format!("{MP}Furniture/bed_1.glb"),
        FurnTable => format!("{MP}Furniture/coffee_table_1.glb"),
        FurnChair => format!("{MP}Furniture/chair_mp_1.glb"),
        FurnCabinet => format!("{MP}Furniture/bedside_table_1.glb"),
        Pine => format!("{NAT}pine_tree_n_1.glb"),
        Pine2 => format!("{NAT}pine_tree_n_2.glb"),
        BareTree5 => format!("{NAT}tree_5.glb"),
        BareTree7 => format!("{NAT}tree_7.glb"),
        DeadTree8 => format!("{NAT}tree_8.glb"),
        DeadTree9 => format!("{NAT}tree_9.glb"),
        FallenLog => format!("{NAT}tree_log_1.glb"),
        Stone2 => format!("{NAT}stone_2.glb"),
        Stone3 => format!("{NAT}stone_3.glb"),
        Fern => format!("{NAT}fern_1.glb"),
        Fern2 => format!("{NAT}fern_2.glb"),
        Stone => format!("{NAT}stone_1.glb"),
        FpsWall => format!("{FPS}wall_2.glb"),
        FpsFloor => format!("{FPS}floor_1.glb"),
        FpsStairs => format!("{FPS}stairs_1.glb"),
        FpsTunnel => format!("{FPS}tunnel_1.glb"),
        FpsTunnelEnd => format!("{FPS}tunnel_1_end.glb"),
        FpsCrate => format!("{FPS}crate_1.glb"),
        FpsBarrel => format!("{FPS}metal_barrel_fps_1.glb"),
        Gravel => format!("{MP2}Debris & Misc/gravel_pile_hr_1.glb"),
        Bench => format!("{MP2}Props/bench_mx_1.glb"),
        // The pack has no municipal bin. Its narrow galvanized barrel is the period-correct
        // silhouette; Bucket/TinCan are progressively cheaper fallbacks at placement sites.
        TrashCan => format!("{MP2}Props/metal_barrel_hr_4.glb"),
        Bucket => format!("{MP2}Props/bucket_mx_1.glb"),
        TinCan => format!("{MP}Small Props/tin_can_mp_1_rusty.glb"),
        SupplyCrate => format!("{MP}Large Props/supply_crate_1.glb"),
        EmptySupplyCrate => format!("{MP}Large Props/supply_crate_1_empty.glb"),
        WoodCrate => format!("{MP2}Props/wooden_crate_1.glb"),
        Board => format!("{MP2}Props/wooden_board_2.glb"),
        Plank => format!("{MP2}Props/wooden_plank_1.glb"),
        Luggage => format!("{MP2}Props/luggage_mp_1.glb"),
        Bottle => format!("{MP2}Props/glass_bottle_mx_3.glb"),
        Radio => format!("{MP2}Props/handheld_fm_radio_etx_1.glb"),
        FirstAid => format!("{MP2}Props/first_aid_kit_hr_1.glb"),
        Jerrycan => format!("{MP2}Props/jerrycan_mx_1.glb"),
        Toolbox => format!("{MP2}Props/toolbox_mx_1.glb"),
        Tire => format!("{MP2}Props/tire_1.glb"),
        Axe => format!("{MP2}Weapons & Tools/axe_mx_1.glb"),
        Shovel => format!("{MP2}Weapons & Tools/shovel_mx_1.glb"),
        HandSaw => format!("{MP}Items & Weapons/hand_saw_1.glb"),
        Meat => format!("{MP}Items & Weapons/meat_1.glb"),
        CannedFood => format!("{MP}Items & Weapons/canned_food_mp_5_rusty.glb"),
        MailBox => format!("{MP2}Props/mail_box_mx_1.glb"),
        Grass => format!("{NAT}grass_1.glb"),
        Grass2 => format!("{NAT}grass_3.glb"),
        Stump => format!("{NAT}tree_stump_1.glb"),
        Fence => format!("{MP2}Modular Props/fence_b_1.glb"),
        TelegraphPole => "procedural:worldgen/telegraph-pole".into(),
        HandCart => "procedural:worldgen/hand-cart".into(),
        Barricade => format!("{MP2}Modular Props/barricade_a_2.glb"),
        Chimney => format!("{MP2}Large Props & Machinery/chimney_a_2.glb"),
        // These seven keys are semantic recipes, not imported models. Existing manifest source
        // strings are translated by `replacement_kit_for_retired_path`; story placements use the
        // same recipes directly. Keeping the logical keys preserves every authored tableau while
        // the runtime visual language stays procedural/Pizza + the retained current tank rigs.
        Deadman => "procedural:worldgen/frozen-corpse".into(),
        GraveCross => format!("{MP}Small Props/cross_1.glb"),
        WreckT34 => "procedural:worldgen/charred-t34-a".into(),
        WreckPz3 => "procedural:worldgen/charred-tiger-a".into(),
        WreckPanther => "procedural:worldgen/charred-t34-b".into(),
        WreckTiger2 => "procedural:worldgen/charred-tiger-b".into(),
        WreckIs2 => "procedural:worldgen/charred-t34-c".into(),
        WreckKv1 => "procedural:worldgen/charred-tiger-c".into(),
    }
}

const CELL: f32 = 4.0; // wall_hr_1 is 4.00 m wide
const WALL_H: f32 = 3.0; // storey height
const STREET: f32 = 10.0;
const MIN_FC: i32 = 3;
const MAX_FC: i32 = 5;
const PITCH: f32 = MAX_FC as f32 * CELL + STREET;
/// Spatial bucket size for instanced batches — the culling granularity.
const BUCKET: f32 = 96.0;

/// All spatial pages for one immutable mesh/material prototype. Pages remain independent culling
/// units; only the heavyweight vertex/index/material upload is shared between them.
pub struct InstancedAssetPages {
    pub kit: Kit,
    pub mesh: CpuMesh,
    pub pages: Vec<Vec<Mat4>>,
}

/// All mutable lightweight-object pages for one immutable mesh/material prototype.
pub struct ScatterAssetPages {
    pub kit: Kit,
    pub mesh: CpuMesh,
    pub pages: Vec<Vec<Mat4>>,
    pub profile: ScatterProfile,
}

/// The generated world: per-module instanced placements, spatially paged and grouped by immutable
/// asset. `Game::build_act` registers one renderer prototype per entry while retaining every page
/// as an independent culling unit. `tree_batches` route through `trees::Trees` so every trunk is
/// destructible (hinge-fall — docs/DESTRUCTION.md §5) on the same shared-prototype draw path.
pub struct GenWorld {
    pub batches: Vec<InstancedAssetPages>,
    pub tree_batches: Vec<InstancedAssetPages>,
    /// Lightweight props stay instanced but hand their mutable transforms to the cheap scatter
    /// subsystem: no crate/barrel/bench/fence becomes invisible static tank mass.
    pub scatter_batches: Vec<ScatterAssetPages>,
    /// Static hard-cover bounds authored by the overlay (wrecks, barricades, sheds). Tiny debris
    /// remains traversable; these boxes are handed to LOS, shell and tank-contact systems.
    pub cover_aabbs: Vec<(Vec3, Vec3)>,
    pub placements: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScatterMotion {
    /// Tall light objects rotate once about their base and stay down.
    Hinge,
    /// Crates, carts, benches and household objects receive a short ballistic shove.
    Loose,
    /// Barrels, cans and tires receive lower launch and longer horizontal roll.
    Roll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScatterMaterial {
    Wood,
    Metal,
    Mixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScatterProfile {
    pub motion: ScatterMotion,
    pub material: ScatterMaterial,
}

fn scatter_profile(k: Kit) -> Option<ScatterProfile> {
    use ScatterMaterial::{Metal, Mixed, Wood};
    use ScatterMotion::{Hinge, Loose, Roll};
    let (motion, material) = match k {
        Kit::Fence | Kit::TelegraphPole => (Hinge, Wood),
        Kit::FpsBarrel | Kit::TrashCan | Kit::Bucket | Kit::TinCan => (Roll, Metal),
        Kit::Tire | Kit::Bottle => (Roll, Mixed),
        Kit::Jerrycan | Kit::Toolbox | Kit::MailBox | Kit::Axe | Kit::Shovel | Kit::HandSaw => {
            (Loose, Metal)
        }
        Kit::FpsCrate
        | Kit::Bench
        | Kit::SupplyCrate
        | Kit::EmptySupplyCrate
        | Kit::WoodCrate
        | Kit::Board
        | Kit::Plank
        | Kit::FurnBed
        | Kit::FurnTable
        | Kit::FurnChair
        | Kit::FurnCabinet
        | Kit::HandCart => (Loose, Wood),
        Kit::Luggage | Kit::Radio | Kit::FirstAid => (Loose, Mixed),
        _ => return None,
    };
    Some(ScatterProfile { motion, material })
}

/// Path classifier for authored/editor manifest pages. It is deliberately explicit and excludes
/// concrete fences: small timber/metal fence panels topple, while concrete remains outside the
/// lightweight system for the structural compiler to own.
pub(crate) fn manifest_scatter_profile(path: &str) -> Option<ScatterProfile> {
    use ScatterMaterial::{Metal, Mixed, Wood};
    use ScatterMotion::{Hinge, Loose, Roll};
    let stem = Path::new(path).file_stem()?.to_str()?.to_ascii_lowercase();
    let contains = |needle: &str| stem.contains(needle);
    let (motion, material) = if (contains("fence") && !contains("concrete"))
        || contains("telegraph_pole")
        || contains("telephone_pole")
        || contains("utility_pole")
        || contains("power_pole")
    {
        (Hinge, if contains("metal") { Metal } else { Wood })
    } else if contains("barrel") || contains("drum") || contains("tin_can") {
        (Roll, Metal)
    } else if contains("tire") || contains("bottle") {
        (Roll, Mixed)
    } else if contains("crate")
        || contains("bench")
        || contains("chair")
        || contains("table")
        || contains("cart")
        || contains("wagon")
        || contains("board")
        || contains("plank")
    {
        (Loose, Wood)
    } else if contains("bucket")
        || contains("jerrycan")
        || contains("toolbox")
        || contains("mail_box")
        || contains("shovel")
        || contains("axe")
        || contains("hand_saw")
    {
        (Loose, Metal)
    } else if contains("luggage") || contains("radio") || contains("first_aid") {
        (Loose, Mixed)
    } else {
        return None;
    };
    Some(ScatterProfile { motion, material })
}

fn story_hard_cover(k: Kit) -> bool {
    matches!(
        k,
        Kit::Wall
            | Kit::WallWin
            | Kit::WallHole
            | Kit::Doorway
            | Kit::Pillar
            | Kit::PillarBroken
            | Kit::Warehouse
            | Kit::Garage
            | Kit::Shed
            | Kit::FpsWall
            | Kit::FpsTunnel
            | Kit::FpsTunnelEnd
            | Kit::Barricade
            | Kit::Chimney
            | Kit::WreckT34
            | Kit::WreckPz3
            | Kit::WreckPanther
            | Kit::WreckTiger2
            | Kit::WreckIs2
            | Kit::WreckKv1
    )
}

fn transformed_aabb(mesh: &CpuMesh, model: Mat4) -> (Vec3, Vec3) {
    transformed_local_aabb(model, mesh.min, mesh.max)
}

fn transformed_local_aabb(model: Mat4, local_min: Vec3, local_max: Vec3) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for corner in 0..8u32 {
        let local = Vec3::new(
            if corner & 1 == 0 {
                local_min.x
            } else {
                local_max.x
            },
            if corner & 2 == 0 {
                local_min.y
            } else {
                local_max.y
            },
            if corner & 4 == 0 {
                local_min.z
            } else {
                local_max.z
            },
        );
        let world = model.transform_point3(local);
        min = min.min(world);
        max = max.max(world);
    }
    (min, max)
}

/// The small vocabulary of manifest assets that represent real static building mass. Campaign
/// manifests also contain floors, roofs, asphalt, furniture, rubble, vegetation, corpses and tank
/// display rigs; a dimensions-only heuristic turns too many of those into invisible barricades.
/// Keep this deliberately path-explicit so adding a new art asset cannot silently change physics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManifestStructure {
    Wall,
    WindowWall,
    BreachedWall,
    Doorway,
    Pillar,
    Warehouse,
    Garage,
    Shed,
    FpsWall,
    FpsTunnel,
    FpsTunnelEnd,
}

/// A manifest structure the Tiger's push and shells can bring down (a destruction record since
/// 2026-09-29 instead of static story cover): walls, window walls, breached walls, doorways,
/// piers and poles, the FPS kit's walls and the shed, garage and warehouse shells. `Some(true)`
/// for timber (the shed), `Some(false)` for brick; tunnels and wrecks stay static mass.
pub(crate) fn manifest_breakable(path: &str) -> Option<bool> {
    use ManifestStructure::*;
    match manifest_structure(path)? {
        Wall | WindowWall | BreachedWall | Doorway | Pillar | FpsWall | Garage | Warehouse => Some(false),
        Shed => Some(true),
        FpsTunnel | FpsTunnelEnd => None,
    }
}

fn manifest_structure(path: &str) -> Option<ManifestStructure> {
    use ManifestStructure::*;

    // Compare components/stems instead of keeping partial model paths as string literals. Besides
    // being separator-safe, this prevents the runtime asset cooker from mistaking a classifier
    // suffix for a new authored model dependency.
    let path = Path::new(path);
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("glb"))
    {
        return None;
    }
    let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
    let parent = path.parent()?.file_name()?.to_str()?.to_ascii_lowercase();
    match (parent.as_str(), stem.as_str()) {
        ("modular structures", "wall_hr_1_m1a_1" | "window_hr_1") => Some(WindowWall),
        ("modular structures", "wall_hr_1_hole_1") => Some(BreachedWall),
        ("modular structures", "doorway_hr_1") => Some(Doorway),
        ("modular structures", "wall_hr_1") => Some(Wall),
        ("modular structures", "pillar_hr_2") => Some(Pillar),
        ("buildings", "warehouse_mx_1") => Some(Warehouse),
        ("buildings", "garage_hl_1") => Some(Garage),
        ("buildings", "shed_ax_1") => Some(Shed),
        ("glb", "wall_2") => Some(FpsWall),
        ("glb", "tunnel_1_end") => Some(FpsTunnelEnd),
        ("glb", "tunnel_1") => Some(FpsTunnel),
        _ => None,
    }
}

/// One-piece farm assets are retained for authored manifest stability, but their gameplay shell is
/// a compatibility approximation. This explicit signal keeps them visible in build telemetry until
/// worldgen replaces each with a true modular room blueprint.
pub(crate) fn manifest_uses_compatibility_shell(path: &str) -> bool {
    matches!(
        manifest_structure(path),
        Some(ManifestStructure::Warehouse | ManifestStructure::Garage | ManifestStructure::Shed)
    )
}

fn proxy_boxes(model: Mat4, local: impl IntoIterator<Item = (Vec3, Vec3)>) -> Vec<(Vec3, Vec3)> {
    local
        .into_iter()
        .map(|(min, max)| transformed_local_aabb(model, min, max))
        .collect()
}

fn doorway_boxes(
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
    min_z: f32,
    max_z: f32,
    opening_min_x: f32,
    opening_max_x: f32,
    opening_top: f32,
) -> [(Vec3, Vec3); 3] {
    [
        (
            Vec3::new(min_x, min_y, min_z),
            Vec3::new(opening_min_x, opening_top, max_z),
        ),
        (
            Vec3::new(opening_max_x, min_y, min_z),
            Vec3::new(max_x, opening_top, max_z),
        ),
        (
            Vec3::new(min_x, opening_top, min_z),
            Vec3::new(max_x, max_y, max_z),
        ),
    ]
}

fn open_building_shell(
    model: Mat4,
    min: Vec3,
    max: Vec3,
    thickness: f32,
    door_min_x: f32,
    door_max_x: f32,
    door_top: f32,
    entrance_at_max_z: bool,
) -> Vec<(Vec3, Vec3)> {
    let entrance_z = if entrance_at_max_z {
        (max.z - thickness, max.z)
    } else {
        (min.z, min.z + thickness)
    };
    let rear_z = if entrance_at_max_z {
        (min.z, min.z + thickness)
    } else {
        (max.z - thickness, max.z)
    };
    let mut boxes = vec![
        (
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(min.x + thickness, max.y, max.z),
        ),
        (
            Vec3::new(max.x - thickness, min.y, min.z),
            Vec3::new(max.x, max.y, max.z),
        ),
        (
            Vec3::new(min.x, min.y, rear_z.0),
            Vec3::new(max.x, max.y, rear_z.1),
        ),
    ];
    boxes.extend(doorway_boxes(
        min.x,
        max.x,
        min.y,
        max.y,
        entrance_z.0,
        entrance_z.1,
        door_min_x,
        door_max_x,
        door_top,
    ));
    proxy_boxes(model, boxes)
}

/// Exact, aperture-aware static collision for one manifest placement.
///
/// These proxies feed the shared foot, tank, shell and LOS registries in `game.rs`. Modular floors,
/// roofs and stairs are intentionally absent: the current terrain controller cannot yet walk an
/// authored upper floor, and treating their union AABBs as horizontal walls would make the ground
/// storey inaccessible. The three legacy one-piece farm buildings receive hollow perimeter shells
/// with their visible entrance left open; they remain flagged here as the bounded compatibility
/// exception until worldgen replaces them with the same modular room grammar as the towns.
pub(crate) fn manifest_structure_proxies(
    path: &str,
    mesh: &CpuMesh,
    model: Mat4,
) -> Vec<(Vec3, Vec3)> {
    use ManifestStructure::*;

    if manifest_wreck_collision::accepts(path) {
        return manifest_wreck_collision::proxies(path, mesh, model);
    }

    match manifest_structure(path) {
        // The Pizza city module contract is measured, stable and much tighter than some variants'
        // decorative debris bounds: 4.0 m wide × 3.0 m tall × 0.4 m thick.
        Some(Wall) => proxy_boxes(
            model,
            [(Vec3::new(-2.0, 0.0, -0.20), Vec3::new(2.0, 3.0, 0.20))],
        ),
        Some(WindowWall) => proxy_boxes(
            model,
            [
                (Vec3::new(-2.0, 0.0, -0.21), Vec3::new(-1.0, 3.0, 0.21)),
                (Vec3::new(1.0, 0.0, -0.21), Vec3::new(2.0, 3.0, 0.21)),
                (Vec3::new(-1.0, 0.0, -0.21), Vec3::new(1.0, 1.02, 0.21)),
                (Vec3::new(-1.0, 2.50, -0.21), Vec3::new(1.0, 3.0, 0.21)),
            ],
        ),
        Some(BreachedWall) => proxy_boxes(
            model,
            [
                (Vec3::new(-2.0, 0.0, -0.21), Vec3::new(-1.05, 3.0, 0.21)),
                (Vec3::new(1.05, 0.0, -0.21), Vec3::new(2.0, 3.0, 0.21)),
                (Vec3::new(-1.05, 0.0, -0.21), Vec3::new(1.05, 0.30, 0.21)),
                (Vec3::new(-1.05, 2.32, -0.21), Vec3::new(1.05, 3.0, 0.21)),
            ],
        ),
        Some(Doorway) => proxy_boxes(
            model,
            doorway_boxes(-2.0, 2.0, 0.0, 3.0, -0.21, 0.21, -0.80, 0.80, 2.10),
        ),
        Some(Pillar | FpsWall) => vec![transformed_aabb(mesh, model)],
        // Source geometry inspection: the wide double door is on warehouse +Z; the garage and
        // shed likewise present their authored door leaves on +Z. Collision models those leaves as
        // open so generated buildings are spaces, not opaque AABB blocks.
        Some(Warehouse) => open_building_shell(
            model,
            Vec3::new(-4.40, 0.0, -6.30),
            Vec3::new(4.40, 7.20, 6.30),
            0.24,
            -2.05,
            2.05,
            3.75,
            true,
        ),
        Some(Garage) => open_building_shell(
            model,
            Vec3::new(-2.50, 0.0, -3.0),
            Vec3::new(2.50, 4.0, 3.0),
            0.22,
            -1.50,
            1.50,
            2.50,
            true,
        ),
        Some(Shed) => open_building_shell(
            model,
            Vec3::new(-1.10, 0.0, -1.0),
            Vec3::new(1.10, 2.59, 1.0),
            0.18,
            -0.52,
            0.52,
            2.10,
            true,
        ),
        Some(FpsTunnel | FpsTunnelEnd) => {
            // The imported tunnel is an open U-section after its glTF node rotation. Preserve its
            // passage rather than turning the entire 2×1.8×2 m module into a solid cube.
            let min = mesh.min;
            let max = mesh.max;
            let wall = ((max.x - min.x).min(max.z - min.z) * 0.10).clamp(0.12, 0.24);
            let roof = ((max.y - min.y) * 0.12).clamp(0.12, 0.24);
            let mut boxes = vec![
                (
                    Vec3::new(min.x, min.y, min.z),
                    Vec3::new(min.x + wall, max.y, max.z),
                ),
                (
                    Vec3::new(max.x - wall, min.y, min.z),
                    Vec3::new(max.x, max.y, max.z),
                ),
                (
                    Vec3::new(min.x, max.y - roof, min.z),
                    Vec3::new(max.x, max.y, max.z),
                ),
            ];
            if manifest_structure(path) == Some(FpsTunnelEnd) {
                boxes.push((
                    Vec3::new(min.x, min.y, min.z),
                    Vec3::new(max.x, max.y, min.z + wall),
                ));
            }
            proxy_boxes(model, boxes)
        }
        None => Vec::new(),
    }
}

/// Deterministic [0,1) counter hash (project pattern, avalanched — mirrors city.rs / gen_world.py).
fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(2_654_435_761);
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    ((x >> 8) & 0xffff) as f32 / 65536.0
}

// These identifiers remain in the tracked manifests so every placement index and transform stays
// byte-for-byte authored. They are compatibility keys only: `load_module` intercepts them before
// resource resolution, and the runtime cooker deliberately ships none of the referenced GLBs.
const RETIRED_DEADMAN_GLB: &str = "assets/flavor/Characters/deadman-psx/baked/deadman_prone.glb";
const RETIRED_WRECK_GLBS: [(&str, Kit); 6] = [
    ("assets/tanks/wrecks/t34_85.glb", Kit::WreckT34),
    ("assets/tanks/wrecks/pz3.glb", Kit::WreckPz3),
    ("assets/tanks/wrecks/panther.glb", Kit::WreckPanther),
    ("assets/tanks/wrecks/tiger2.glb", Kit::WreckTiger2),
    ("assets/tanks/wrecks/is2.glb", Kit::WreckIs2),
    // KV-1 is manifest-only today; its explicit semantic key preserves a distinct scale/rest pose.
    ("assets/tanks/wrecks/kv1.glb", Kit::WreckKv1),
];
const RETIRED_HALF_TIGER_GLBS: [&str; 3] = [
    "assets/tanks/tiger/baked/half/tiger_hull.glb",
    "assets/tanks/tiger/baked/half/tiger_turret.glb",
    "assets/tanks/tiger/baked/half/tiger_gun.glb",
];
const CURRENT_TIGER_PART_GLBS: [&str; 3] = [
    "assets/tanks/tiger/baked/tiger_hull.glb",
    "assets/tanks/tiger/baked/tiger_turret.glb",
    "assets/tanks/tiger/baked/tiger_gun.glb",
];

fn replacement_kit_for_retired_path(path: &str) -> Option<Kit> {
    if path == RETIRED_DEADMAN_GLB {
        return Some(Kit::Deadman);
    }
    RETIRED_WRECK_GLBS
        .iter()
        .find_map(|(retired, kit)| (*retired == path).then_some(*kit))
}

fn retired_tiger_part_index(path: &str) -> Option<usize> {
    RETIRED_HALF_TIGER_GLBS
        .iter()
        .position(|retired| *retired == path)
}

pub(crate) fn is_retired_semantic_model_path(path: &str) -> bool {
    replacement_kit_for_retired_path(path).is_some() || retired_tiger_part_index(path).is_some()
}

/// Resolve an old half-resolution placement key to the matching part from the current Tiger rig.
/// The ordinary path is exactly `tank::build_tiger`'s loader + Dunkelgrau repaint, but loads only
/// the requested part. Its procedural build fallback is retained for a deliberately assetless dev
/// checkout; packaged execution always carries the three current GLBs.
fn current_tiger_part_mesh(index: usize) -> Option<CpuMesh> {
    let path = *CURRENT_TIGER_PART_GLBS.get(index)?;
    crate::gltf_load::load_glb(path)
        .ok()
        .map(crate::tank::repaint_german_grey)
        .or_else(|| crate::tank::build_tiger().into_iter().nth(index))
}

fn is_retained_wreck(k: Kit) -> bool {
    matches!(
        k,
        Kit::WreckT34
            | Kit::WreckPz3
            | Kit::WreckPanther
            | Kit::WreckTiger2
            | Kit::WreckIs2
            | Kit::WreckKv1
    )
}

/// Collapse several already-positioned meshes into one flat-shaded PSX-style silhouette. The
/// retained tank bakes use separate texture atlases per articulated part, so a deliberately matte
/// one-pixel charcoal material is both cheaper and more truthful for a completely burned-out hull.
fn merge_solid_meshes(meshes: impl IntoIterator<Item = CpuMesh>, color: [u8; 3]) -> CpuMesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for mesh in meshes {
        let base = vertices.len() as u32;
        vertices.extend(mesh.vertices);
        indices.extend(mesh.indices.into_iter().map(|index| base + index));
        min = min.min(mesh.min);
        max = max.max(mesh.max);
    }
    debug_assert!(!vertices.is_empty());
    CpuMesh::basic(
        vertices,
        indices,
        Some(crate::prims::solid_color(color)),
        min,
        max,
    )
}

/// A prone, snow-dusted greatcoat silhouette assembled from six flat-shaded boxes. It is the same
/// deliberately chunky visual grammar as the engine's other procedural blockout people: readable
/// after the 640×360 crush, deterministic, and independent of an imported character license.
fn procedural_corpse_mesh() -> CpuMesh {
    let color = [62, 66, 60];
    let boxes = [
        (Vec3::new(-0.24, 0.0, -0.30), Vec3::new(0.24, 0.26, 0.40)),
        (Vec3::new(-0.13, 0.0, 0.40), Vec3::new(0.13, 0.20, 0.66)),
        (Vec3::new(-0.22, 0.0, -0.95), Vec3::new(-0.02, 0.18, -0.28)),
        (Vec3::new(0.02, 0.0, -0.95), Vec3::new(0.22, 0.18, -0.28)),
        (Vec3::new(-0.46, 0.0, -0.02), Vec3::new(-0.24, 0.13, 0.34)),
        (Vec3::new(0.24, 0.0, -0.02), Vec3::new(0.46, 0.13, 0.34)),
    ];
    merge_solid_meshes(
        boxes
            .into_iter()
            .map(|(min, max)| crate::prims::box_mesh(min, max, color)),
        color,
    )
}

/// Asset-pack independent telegraph pole. Square timber and chunky cross-arm insulators read cleanly
/// after the retro crush, and the base-at-y=0 contract gives the scatter subsystem an exact hinge.
fn procedural_telegraph_pole_mesh() -> CpuMesh {
    let color = [82, 64, 43];
    let boxes = [
        (Vec3::new(-0.11, 0.0, -0.11), Vec3::new(0.11, 6.4, 0.11)),
        (Vec3::new(-1.05, 5.50, -0.10), Vec3::new(1.05, 5.68, 0.10)),
        (Vec3::new(-0.76, 6.00, -0.09), Vec3::new(0.76, 6.16, 0.09)),
        (Vec3::new(-0.94, 5.68, -0.12), Vec3::new(-0.78, 5.88, 0.12)),
        (Vec3::new(0.78, 5.68, -0.12), Vec3::new(0.94, 5.88, 0.12)),
        (Vec3::new(-0.65, 6.16, -0.11), Vec3::new(-0.51, 6.34, 0.11)),
        (Vec3::new(0.51, 6.16, -0.11), Vec3::new(0.65, 6.34, 0.11)),
    ];
    merge_solid_meshes(
        boxes
            .into_iter()
            .map(|(min, max)| crate::prims::box_mesh(min, max, color)),
        color,
    )
}

/// A deliberately low-poly handcart assembled from one cargo bed, two shafts and faceted wheels.
/// It fills the missing licensed-asset role without adding a new external dependency.
fn procedural_hand_cart_mesh() -> CpuMesh {
    let color = [91, 70, 45];
    let body = [
        crate::prims::box_mesh(
            Vec3::new(-0.72, 0.48, -0.70),
            Vec3::new(0.72, 1.08, 0.62),
            color,
        ),
        crate::prims::box_mesh(
            Vec3::new(-0.64, 0.38, 0.48),
            Vec3::new(-0.49, 0.50, 2.10),
            color,
        ),
        crate::prims::box_mesh(
            Vec3::new(0.49, 0.38, 0.48),
            Vec3::new(0.64, 0.50, 2.10),
            color,
        ),
        crate::prims::box_mesh(
            Vec3::new(-0.78, 0.43, -0.06),
            Vec3::new(0.78, 0.57, 0.08),
            color,
        ),
    ];
    let wheel = crate::prims::faceted_sphere_mesh(color);
    let left = wheel
        .clone()
        .transformed(Mat4::from_scale_rotation_translation(
            Vec3::new(0.10, 0.49, 0.49),
            Quat::IDENTITY,
            Vec3::new(-0.79, 0.49, -0.03),
        ));
    let right = wheel.transformed(Mat4::from_scale_rotation_translation(
        Vec3::new(0.10, 0.49, 0.49),
        Quat::IDENTITY,
        Vec3::new(0.79, 0.49, -0.03),
    ));
    merge_solid_meshes(body.into_iter().chain([left, right]), color)
}

/// Build one static burned-out tank from the retained articulated gameplay rig. Turret and gun are
/// baked into a damaged resting pose, then the whole assembly receives one matte-charcoal material.
fn build_charred_wreck(t34: bool) -> CpuMesh {
    let (parts, rig, turret_yaw, gun_droop, color) = if t34 {
        (
            crate::tank::T34_PARTS,
            crate::tank::build_t34(),
            0.72,
            0.16,
            [31, 35, 30],
        )
    } else {
        (
            crate::tank::TIGER_PARTS,
            crate::tank::build_tiger(),
            -0.93,
            0.19,
            [36, 34, 30],
        )
    };
    let [hull, turret, gun] = rig;
    let turret_pose =
        Mat4::from_translation(parts.turret_pivot) * Mat4::from_rotation_y(turret_yaw);
    let gun_pose = turret_pose
        * Mat4::from_translation(parts.gun_trunnion)
        // Source barrels point along +Z: positive X rotation lowers their muzzle.
        * Mat4::from_rotation_x(gun_droop);
    merge_solid_meshes(
        [
            hull,
            turret.transformed(turret_pose),
            gun.transformed(gun_pose),
        ],
        color,
    )
}

static CHARRED_T34_WRECK: OnceLock<CpuMesh> = OnceLock::new();
static CHARRED_TIGER_WRECK: OnceLock<CpuMesh> = OnceLock::new();

fn retained_wreck_mesh(k: Kit) -> CpuMesh {
    let t34 = matches!(k, Kit::WreckT34 | Kit::WreckPanther | Kit::WreckIs2);
    let base = if t34 {
        CHARRED_T34_WRECK
            .get_or_init(|| build_charred_wreck(true))
            .clone()
    } else {
        CHARRED_TIGER_WRECK
            .get_or_init(|| build_charred_wreck(false))
            .clone()
    };
    // One retained donor still reads as a varied knocked-out column: silhouette scale and the
    // blast-slumped resting angle are semantic, fixed per compatibility key, and therefore stable
    // for both manifest and story-overlay placements.
    let (scale, pitch, roll) = match k {
        Kit::WreckT34 => (1.00, 0.015, 0.035),
        Kit::WreckPz3 => (0.82, -0.020, -0.055),
        Kit::WreckPanther => (1.08, 0.025, 0.065),
        Kit::WreckTiger2 => (1.12, -0.018, -0.040),
        Kit::WreckIs2 => (1.16, 0.020, 0.050),
        Kit::WreckKv1 => (1.02, -0.015, -0.045),
        _ => unreachable!("retained_wreck_mesh requires a wreck kit"),
    };
    base.transformed(
        Mat4::from_rotation_x(pitch)
            * Mat4::from_rotation_z(roll)
            * Mat4::from_scale(Vec3::splat(scale)),
    )
}

/// Load a kit module. Pizza entries resolve normally; bodies and wrecks are semantic recipes so
/// no foreign corpse/wreck GLB can leak back into either developer or packaged execution.
pub fn load_kit(k: Kit) -> Option<CpuMesh> {
    match k {
        Kit::BareTree5 | Kit::BareTree7 => {
            let leaves = if k == Kit::BareTree5 { "tree_5_leaves" } else { "tree_7_leaves" };
            // The separated leaf mesh is a CHILD of the trunk, not a scene root. Retain every
            // woody triangle and accumulated transform in page zero; omit the foliage subtree.
            crate::gltf_load::load_glb_partitioned_subtrees(&path_of(k), &[leaves])
                .ok().map(|mut pages| pages.remove(0))
        }
        Kit::Deadman => Some(procedural_corpse_mesh()),
        Kit::TelegraphPole => Some(procedural_telegraph_pole_mesh()),
        Kit::HandCart => Some(procedural_hand_cart_mesh()),
        _ if is_retained_wreck(k) => Some(retained_wreck_mesh(k)),
        _ => load_module(&path_of(k)),
    }
}

/// Path-keyed variant of `load_kit`. The manifest loader passes its original authored path here;
/// retired compatibility identifiers are translated before the generic GLB/resource resolver.
pub fn load_module(path: &str) -> Option<CpuMesh> {
    if let Some(part_index) = retired_tiger_part_index(path) {
        return current_tiger_part_mesh(part_index);
    }
    if let Some(replacement) = replacement_kit_for_retired_path(path) {
        return load_kit(replacement);
    }
    // The NAT (Nature separated) pack used to need an upright+re-origin fix-up here — the GLBs
    // shipped lying on their sides with showcase-scene offsets. The files themselves are now
    // corrected on disk (tools/worldgen/fix_sideways_glbs.py, originals kept as *_orig.glb), so
    // EVERY consumer — engine, worldbuilder, thumbnails, the Blender build — sees them upright
    // without special-casing. Compensating here again would lay them back down.
    crate::gltf_load::load_glb(path).ok()
}

struct Gen<'a> {
    hf: &'a Heightfield,
    seed: u32,
    cache: HashMap<Kit, Option<CpuMesh>>,
    /// (kit, bucket) → per-instance transforms.
    buckets: HashMap<(Kit, i32, i32), Vec<Mat4>>,
    placements: usize,
    missing: usize,
}

impl Gen<'_> {
    fn rnd(&self, id: u32, salt: u32) -> f32 {
        hash01(self.seed ^ id.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA77))
    }

    fn loaded(&mut self, k: Kit) -> bool {
        if !self.cache.contains_key(&k) {
            let m = load_kit(k);
            if m.is_none() {
                self.missing += 1;
                println!("[worldgen] MISSING {}", path_of(k));
            }
            self.cache.insert(k, m);
        }
        self.cache.get(&k).map(Option::is_some).unwrap_or(false)
    }

    fn place(&mut self, k: Kit, x: f32, y: f32, z: f32, rot: f32, scale: f32) {
        self.place_pose(k, x, y, z, rot, 0.0, 0.0, scale);
    }

    /// General pose used by the story layer for toppled chairs, abandoned tools and wreck-adjacent
    /// clutter. The district generator keeps using `place`, so its established transforms do not
    /// change. Rotation order is yaw (world Y), then local pitch/roll.
    #[allow(clippy::too_many_arguments)]
    fn place_pose(
        &mut self,
        k: Kit,
        x: f32,
        y: f32,
        z: f32,
        yaw: f32,
        pitch: f32,
        roll: f32,
        scale: f32,
    ) {
        if !self.loaded(k) {
            return;
        }
        let m = Mat4::from_scale_rotation_translation(
            Vec3::splat(scale),
            Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Quat::from_rotation_z(roll),
            Vec3::new(x, y, z),
        );
        let bx = (x / BUCKET).floor() as i32;
        let bz = (z / BUCKET).floor() as i32;
        self.buckets.entry((k, bx, bz)).or_default().push(m);
        self.placements += 1;
        if is_retained_wreck(k) {
            self.place_wreck_dressing(k, x, y, z, yaw, scale);
        }
    }

    /// Give every story-layer wreck a small Pizza-authored debris signature. The retained tank hull
    /// remains the sole hard-cover object; these rubble/tire/board pieces are traversable dressing.
    /// Offsets are fixed per semantic wreck key and rotate with its authored yaw, so the result is
    /// reproducible while columns do not read as five copies of the same donor mesh.
    fn place_wreck_dressing(&mut self, k: Kit, x: f32, y: f32, z: f32, yaw: f32, scale: f32) {
        let variant = match k {
            Kit::WreckT34 => 0,
            Kit::WreckPz3 => 1,
            Kit::WreckPanther => 2,
            Kit::WreckTiger2 => 3,
            Kit::WreckIs2 => 4,
            Kit::WreckKv1 => 5,
            _ => return,
        };
        let sign = if variant & 1 == 0 { 1.0 } else { -1.0 };
        let loose_prop = if variant % 3 == 0 {
            Kit::Jerrycan
        } else {
            Kit::Tire
        };
        let timber = if variant & 1 == 0 {
            Kit::Board
        } else {
            Kit::Plank
        };
        let dressing = [
            (
                Kit::RubbleField,
                Vec3::new(-0.25 * sign, -0.13, 0.15),
                0.31 + variant as f32 * 0.17,
                0.0,
                0.0,
                1.18,
            ),
            (
                Kit::RubbleStack,
                Vec3::new(1.55 * sign, -0.07, -1.05),
                -0.46 + variant as f32 * 0.11,
                0.0,
                0.0,
                0.82,
            ),
            (
                loose_prop,
                Vec3::new(-1.75 * sign, 0.03, 1.20),
                0.84 - variant as f32 * 0.09,
                1.20,
                0.16 * sign,
                0.92,
            ),
            (
                timber,
                Vec3::new(0.85 * sign, 0.01, 2.05),
                -0.72 + variant as f32 * 0.13,
                0.08,
                -0.10 * sign,
                1.05,
            ),
        ];
        let rotate = Quat::from_rotation_y(yaw);
        for (item, local, yaw_delta, pitch, roll, item_scale) in dressing {
            let offset = rotate * (local * scale);
            self.place_pose(
                item,
                x + offset.x,
                y + offset.y,
                z + offset.z,
                yaw + yaw_delta,
                pitch,
                roll,
                scale * item_scale,
            );
        }
    }

    /// Place the first available asset in a semantic fallback chain. This keeps a tableau legible
    /// on reduced installations: e.g. a missing Tiger-II wreck becomes a Pz.III, then a rubble heap;
    /// a missing trash barrel becomes the FPS barrel, then a bucket. Missing optional libraries are
    /// still logged once by `loaded`, never fatal.
    #[allow(clippy::too_many_arguments)]
    fn place_first_pose(
        &mut self,
        choices: &[Kit],
        x: f32,
        z: f32,
        y_offset: f32,
        yaw: f32,
        pitch: f32,
        roll: f32,
        scale: f32,
    ) {
        let mut chosen = None;
        for &k in choices {
            if self.loaded(k) {
                chosen = Some(k);
                break;
            }
        }
        if let Some(k) = chosen {
            let y = self.ground(x, z) + y_offset;
            self.place_pose(k, x, y, z, yaw, pitch, roll, scale);
        }
    }

    fn place_first(
        &mut self,
        choices: &[Kit],
        x: f32,
        z: f32,
        y_offset: f32,
        yaw: f32,
        scale: f32,
    ) {
        self.place_first_pose(choices, x, z, y_offset, yaw, 0.0, 0.0, scale);
    }

    /// Counter-hash scatter in a disk. The item order, angle, radius, scale and yaw all derive from
    /// `(act seed, patch id, item index)`, so adding a manifest edit cannot perturb these placements.
    #[allow(clippy::too_many_arguments)]
    fn scatter_patch(
        &mut self,
        cx: f32,
        cz: f32,
        patch_id: u32,
        count: u32,
        radius: f32,
        choices: &[Kit],
        min_scale: f32,
        max_scale: f32,
        y_offset: f32,
        exclude_r: f32,
    ) {
        if choices.is_empty() {
            return;
        }
        for i in 0..count {
            let a = self.rnd(patch_id, i * 7 + 1) * std::f32::consts::TAU;
            let d = self.rnd(patch_id, i * 7 + 2).sqrt() * radius;
            let x = cx + a.cos() * d;
            let z = cz + a.sin() * d;
            if x * x + z * z < exclude_r * exclude_r {
                continue;
            }
            let ki = (self.rnd(patch_id, i * 7 + 3) * choices.len() as f32) as usize;
            let k = choices[ki.min(choices.len() - 1)];
            let scale = min_scale + self.rnd(patch_id, i * 7 + 4) * (max_scale - min_scale);
            let yaw = self.rnd(patch_id, i * 7 + 5) * std::f32::consts::TAU;
            self.place_first(
                &[k, Kit::RubbleField, Kit::Brick],
                x,
                z,
                y_offset,
                yaw,
                scale,
            );
        }
    }

    fn ground(&self, x: f32, z: f32) -> f32 {
        self.hf.height_at(x, z)
    }

    // ── one kitbashed building WITH interior (partitions, furniture, stairs) ──
    #[allow(clippy::too_many_arguments)]
    fn building(&mut self, cx: f32, cz: f32, bid: u32, r: f32) {
        let base = self.ground(cx, cz) - 0.3;
        let fcx = (MIN_FC + (self.rnd(bid, 2) * (MAX_FC - MIN_FC + 1) as f32) as i32).min(MAX_FC);
        let fcz = (MIN_FC + (self.rnd(bid, 3) * (MAX_FC - MIN_FC + 1) as f32) as i32).min(MAX_FC);
        let (hx, hz) = (fcx as f32 * CELL * 0.5, fcz as f32 * CELL * 0.5);
        let storeys = if self.rnd(bid, 4) < (1.0 - r) * 0.45 {
            2
        } else {
            1
        };

        for s in 0..storeys {
            let y = base + s as f32 * WALL_H;
            let ground = s == 0;
            for i in 0..fcx {
                let wx = cx - hx + (i as f32 + 0.5) * CELL;
                self.wall_seg(wx, y, cz - hz, 0.0, bid, (s * 40 + i) as u32, r, ground);
                self.wall_seg(
                    wx,
                    y,
                    cz + hz,
                    0.0,
                    bid,
                    (s * 40 + 10 + i) as u32,
                    r,
                    ground,
                );
            }
            for j in 0..fcz {
                let wz = cz - hz + (j as f32 + 0.5) * CELL;
                let hpi = std::f32::consts::FRAC_PI_2;
                self.wall_seg(
                    cx - hx,
                    y,
                    wz,
                    hpi,
                    bid,
                    (s * 40 + 20 + j) as u32,
                    r,
                    ground,
                );
                self.wall_seg(
                    cx + hx,
                    y,
                    wz,
                    hpi,
                    bid,
                    (s * 40 + 30 + j) as u32,
                    r,
                    ground,
                );
            }
            for (k, (px, pz)) in [
                (cx - hx, cz - hz),
                (cx + hx, cz - hz),
                (cx - hx, cz + hz),
                (cx + hx, cz + hz),
            ]
            .iter()
            .enumerate()
            {
                let v = self.rnd(bid, 300 + (s * 4 + k as i32) as u32);
                if v < 0.15 * r {
                    continue; // blown away
                }
                if v < 0.4 * r {
                    self.place(
                        Kit::PillarBroken,
                        *px,
                        y,
                        *pz,
                        self.rnd(bid, 900 + k as u32) * std::f32::consts::TAU,
                        1.0,
                    );
                } else {
                    self.place(Kit::Pillar, *px, y, *pz, 0.0, 1.0);
                }
            }
            // floor slabs; upper storeys can be shell-holed
            for gi in 0..fcx {
                for gj in 0..fcz {
                    let fx = cx - hx + (gi as f32 + 0.5) * CELL;
                    let fz = cz - hz + (gj as f32 + 0.5) * CELL;
                    let holed = s > 0 && self.rnd(bid, 1300 + (gi * 8 + gj) as u32) < 0.25 * r;
                    self.place(
                        if holed { Kit::FloorHole } else { Kit::Floor },
                        fx,
                        y,
                        fz,
                        0.0,
                        1.0,
                    );
                }
            }
            // INTERIOR partition row with a doorway → two walkable rooms
            if fcx >= 4 {
                let pj = fcz / 2;
                let wz = cz - hz + pj as f32 * CELL;
                let door_i = 1 + (self.rnd(bid, 1400 + s as u32) * (fcx - 2) as f32) as i32;
                for i in 0..fcx {
                    let wx = cx - hx + (i as f32 + 0.5) * CELL;
                    if self.rnd(bid, 1500 + (s * 16 + i) as u32) < 0.30 * r {
                        continue; // ruination ate this partition segment
                    }
                    self.place(
                        if i == door_i { Kit::Doorway } else { Kit::Wall },
                        wx,
                        y,
                        wz,
                        0.0,
                        1.0,
                    );
                }
            }
            // scavengeable ground-floor furniture
            if ground {
                let nf = 2 + (self.rnd(bid, 1600) * 4.0) as u32;
                for f in 0..nf {
                    let fx = cx - hx + CELL + self.rnd(bid, 1700 + f) * (2.0 * hx - 2.0 * CELL);
                    let fz = cz - hz + CELL + self.rnd(bid, 1750 + f) * (2.0 * hz - 2.0 * CELL);
                    let kind = [
                        Kit::FurnBed,
                        Kit::FurnTable,
                        Kit::FurnChair,
                        Kit::FurnCabinet,
                    ][(self.rnd(bid, 1800 + f) * 4.0) as usize % 4];
                    self.place(
                        kind,
                        fx,
                        base + 0.05,
                        fz,
                        self.rnd(bid, 1850 + f) * std::f32::consts::TAU,
                        1.0,
                    );
                }
            }
        }
        if storeys == 2 {
            self.place(
                Kit::Stairs,
                cx - hx + CELL * 0.9,
                base,
                cz + hz - CELL * 0.9,
                std::f32::consts::PI,
                1.0,
            );
        }
        // roof — mostly gone under heavy ruination
        let top = base + storeys as f32 * WALL_H;
        if self.rnd(bid, 9) > 0.25 + 0.6 * r {
            let flat = self.rnd(bid, 8) < 0.4;
            for gi in 0..fcx {
                for gj in 0..fcz {
                    let fx = cx - hx + (gi as f32 + 0.5) * CELL;
                    let fz = cz - hz + (gj as f32 + 0.5) * CELL;
                    if flat {
                        if gi % 2 == 0 {
                            self.place(Kit::RoofFlat, fx + CELL * 0.5, top - 0.05, fz, 0.0, 1.0);
                        }
                    } else {
                        self.place(Kit::RoofPitch, fx, top - 0.05, fz, 0.0, 1.0);
                    }
                }
            }
        }
        // street rubble ring
        let n = 1 + (r * 5.0) as u32;
        for k in 0..n {
            let a = self.rnd(bid, 400 + k) * std::f32::consts::TAU;
            let d = hx.max(hz) + 1.0 + self.rnd(bid, 450 + k) * (STREET * 0.5);
            let (x, z) = (cx + a.cos() * d, cz + a.sin() * d);
            let kind = [Kit::RubbleField, Kit::RubbleStack, Kit::Brick]
                [(self.rnd(bid, 500 + k) * 3.0) as usize % 3];
            self.place(
                kind,
                x,
                self.ground(x, z) - 0.1,
                z,
                self.rnd(bid, 550 + k) * std::f32::consts::TAU,
                0.8 + self.rnd(bid, 600 + k) * 0.6,
            );
        }
    }

    fn wall_seg(
        &mut self,
        x: f32,
        y: f32,
        z: f32,
        rot: f32,
        bid: u32,
        key: u32,
        r: f32,
        ground: bool,
    ) {
        let v = self.rnd(bid, 100 + key);
        if v < 0.08 + 0.30 * r {
            self.place(
                Kit::RubbleStack,
                x,
                self.ground(x, z) - 0.1,
                z,
                self.rnd(bid, 700 + key) * std::f32::consts::TAU,
                1.0,
            );
            return;
        }
        let w = self.rnd(bid, 200 + key);
        let k = if w < 0.25 * r {
            Kit::WallHole
        } else if ground && w < 0.25 * r + 0.10 {
            Kit::Doorway
        } else if w < 0.25 * r + 0.30 {
            if ground {
                Kit::WallWin
            } else {
                Kit::Window
            }
        } else {
            Kit::Wall
        };
        self.place(k, x, y, z, rot, 1.0);
        if k == Kit::WallHole {
            self.place(
                Kit::RubbleField,
                x,
                self.ground(x, z) - 0.1,
                z,
                self.rnd(bid, 800 + key) * std::f32::consts::TAU,
                1.0,
            );
        }
    }

    /// The underground bunker: stair shaft down, tunnel corridor, stocked store room (Retro FPS kit).
    fn bunker(&mut self, x: f32, z: f32) {
        let depth = self.ground(x, z) - 5.0;
        self.place(Kit::FpsStairs, x, depth, z, 0.0, 1.0);
        for k in 0..4 {
            self.place(Kit::FpsTunnel, x, depth, z - 6.0 - k as f32 * 4.0, 0.0, 1.0);
        }
        self.place(Kit::FpsTunnelEnd, x, depth, z - 24.0, 0.0, 1.0);
        let (rx, rz) = (x + 8.0, z - 14.0);
        for i in -1..=1 {
            for j in -1..=1 {
                self.place(
                    Kit::FpsFloor,
                    rx + i as f32 * 4.0,
                    depth,
                    rz + j as f32 * 4.0,
                    0.0,
                    1.0,
                );
            }
        }
        for i in -1..=1i32 {
            self.place(Kit::FpsWall, rx + i as f32 * 4.0, depth, rz - 6.0, 0.0, 1.0);
            self.place(Kit::FpsWall, rx + i as f32 * 4.0, depth, rz + 6.0, 0.0, 1.0);
            self.place(
                Kit::FpsWall,
                rx + 6.0,
                depth,
                rz + i as f32 * 4.0,
                std::f32::consts::FRAC_PI_2,
                1.0,
            );
        }
        for k in 0..5u32 {
            self.place(
                Kit::FpsCrate,
                rx - 3.0 + hash01(k * 77) * 6.0,
                depth + 0.05,
                rz - 3.0 + hash01(k * 131) * 6.0,
                hash01(k * 313) * std::f32::consts::TAU,
                1.0,
            );
        }
        for k in 0..3u32 {
            self.place(
                Kit::FpsBarrel,
                rx + 3.0 - hash01(k * 991) * 2.0,
                depth + 0.05,
                rz + 2.0,
                0.0,
                1.0,
            );
        }
    }
}

// ── Environmental-storytelling overlay ─────────────────────────────────────────────────────────
//
// The manifest is the authority for macro layout, but a macro layout can still be visually mute.
// This layer is intentionally ADDITIVE and independent of manifest presence: a small vocabulary of
// verified assets is recombined into wordless, act-specific tableaus, then a lighter deterministic
// clutter pass joins those authored beats along the east→west route. Nothing here creates gameplay
// state or readable text. It is just geometry whose arrangement tells the story.

/// Deterministic first-grove center for the capture/editor survey; the runtime placement recipe
/// below uses exactly the same act seed and counter hash.
pub fn seasonal_grove_survey_anchor(act: &crate::world::ActMap) -> glam::Vec2 {
    let delta = act.goal - act.spawn;
    let dir = delta.normalize_or_zero();
    let side = glam::Vec2::new(-dir.y, dir.x);
    let key = 0xBA2E_0000u32 + act.id as u32 * 100;
    let random = hash01((act.seed ^ 0xE57A_0A11) ^ key.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA77);
    act.spawn + delta * 0.008 - side * (22.0 + random * 80.0)
}

/// Sparse, broken field/property boundary lines, not a forest fill. The authored macro map
/// remains authoritative. Act I previously had only 30 pines and no bare deciduous placements;
/// this additive layer also runs when that manifest loads. Later acts get bare April branch forms
/// and small emerging ground patches, informed by the Faminsky April 1945 image references in the
/// Astra audit. These are inferred seasonal arrangements, not claimed surveyed witness locations.
fn seasonal_boundary_groves(g: &mut Gen<'_>, act: &crate::world::ActMap, exclude_r: f32) {
    let delta = act.goal - act.spawn;
    let dir = delta.normalize_or_zero();
    let side = glam::Vec2::new(-dir.y, dir.x);
    let groups = if act.id == 1 { 18u32 } else { 14u32 };
    let trees = [Kit::BareTree5, Kit::BareTree7, Kit::DeadTree8, Kit::DeadTree9, Kit::Pine, Kit::Pine2];
    for group in 0..groups {
        let key = 0xBA2E_0000u32 + act.id as u32 * 100 + group;
        let t = 0.008 + group as f32 / (groups - 1) as f32 * 0.94;
        let sign = if group % 2 == 0 { -1.0 } else { 1.0 };
        let lateral = sign * (22.0 + g.rnd(key, 1) * 80.0);
        let center = act.spawn + delta * t + side * lateral;
        let count = 3 + (g.rnd(key, 2) * 5.0) as u32;
        for item in 0..count {
            // Small irregular lines with gaps retain broad open views across the steppe/fields.
            let along = (item as f32 - count as f32 * 0.5) * (5.0 + g.rnd(key, 3) * 3.0);
            let offset = (g.rnd(key, 10 + item * 7) - 0.5) * 5.0;
            let p = center + dir * along + side * offset;
            if p.length_squared() < exclude_r * exclude_r
                || p.x.abs() > act.half_extent_m - 24.0 || p.y.abs() > act.half_extent_m - 24.0 { continue; }
            let tree = trees[(g.rnd(key, 11 + item * 7) * trees.len() as f32) as usize % trees.len()];
            let scale = 0.68 + g.rnd(key, 12 + item * 7) * 0.58;
            let yaw = g.rnd(key, 13 + item * 7) * std::f32::consts::TAU;
            g.place_first(&[tree, Kit::DeadTree8], p.x, p.y, -0.025, yaw, scale);
            // Retained natural geometry beside trees, never a grid of road-blocking props.
            let choices = if act.id == 1 {
                &[Kit::Stump, Kit::FallenLog, Kit::Stone2, Kit::Stone3][..]
            } else {
                &[Kit::Stump, Kit::FallenLog, Kit::Stone2, Kit::Stone3, Kit::Fern, Kit::Fern2][..]
            };
            for piece in 0..3u32 {
                let a = g.rnd(key, 200 + item * 13 + piece * 3) * std::f32::consts::TAU;
                let radius = 1.5 + g.rnd(key, 201 + item * 13 + piece * 3) * 3.0;
                let q = p + glam::Vec2::new(a.cos(), a.sin()) * radius;
                if q.length_squared() < exclude_r * exclude_r { continue; }
                let choice = choices[(item + piece + group) as usize % choices.len()];
                let scale = match choice {
                    Kit::FallenLog => 0.30 + g.rnd(key, 410 + piece) * 0.30,
                    Kit::Stump => 0.55 + g.rnd(key, 420 + piece) * 0.30,
                    Kit::Fern | Kit::Fern2 => 0.35 + g.rnd(key, 430 + piece) * 0.30,
                    _ => 0.22 + g.rnd(key, 440 + piece) * 0.30,
                };
                g.place_first(&[choice], q.x, q.y, -0.035, a, scale);
            }
        }
    }
}

fn route_clutter(g: &mut Gen<'_>, act: &crate::world::ActMap, exclude_r: f32) {
    let delta = act.goal - act.spawn;
    let dir = delta.normalize_or_zero();
    let side = glam::Vec2::new(-dir.y, dir.x);
    let count = if act.homeland { 300u32 } else { 220u32 };
    let winter = [
        Kit::RubbleField,
        Kit::Brick,
        Kit::Gravel,
        Kit::Board,
        Kit::TrashCan,
        Kit::TinCan,
        Kit::WoodCrate,
        Kit::Bottle,
    ];
    let spring = [
        Kit::Grass,
        Kit::Grass2,
        Kit::Stump,
        Kit::Board,
        Kit::Bucket,
        Kit::TinCan,
        Kit::WoodCrate,
        Kit::Bottle,
    ];
    let choices = if act.homeland {
        &spring[..]
    } else {
        &winter[..]
    };

    for i in 0..count {
        // Keep the literal road centre readable: clutter tells you where people travelled without
        // turning the visual layer into an invisible collision wall.
        let t = 0.035 + g.rnd(0xC17A_0000 + act.id as u32, i * 11 + 1) * 0.93;
        let along = (g.rnd(0xC17A_1000 + act.id as u32, i * 11 + 2) - 0.5) * 70.0;
        let side_sign = if g.rnd(0xC17A_2000 + act.id as u32, i * 11 + 3) < 0.5 {
            -1.0
        } else {
            1.0
        };
        let lateral = side_sign * (38.0 + g.rnd(0xC17A_3000 + act.id as u32, i * 11 + 4) * 235.0);
        let p = act.spawn + delta * t + dir * along + side * lateral;
        if p.x.abs() > act.half_extent_m - 35.0 || p.y.abs() > act.half_extent_m - 35.0 {
            continue;
        }
        if p.length_squared() < exclude_r * exclude_r {
            continue;
        }
        let ki = (g.rnd(0xC17A_4000 + act.id as u32, i * 11 + 5) * choices.len() as f32) as usize
            % choices.len();
        let k = choices[ki];
        let scale = 0.72 + g.rnd(0xC17A_5000 + act.id as u32, i * 11 + 6) * 0.58;
        let yaw = g.rnd(0xC17A_6000 + act.id as u32, i * 11 + 7) * std::f32::consts::TAU;
        let fallback = if act.homeland {
            Kit::Grass
        } else {
            Kit::RubbleField
        };
        g.place_first(&[k, fallback, Kit::Brick], p.x, p.y, -0.06, yaw, scale);
    }

    // A sparse, partially broken communications line gives the lightweight-destruction path a
    // tall readable object without depending on an absent pole asset. It follows the broad authored
    // travel corridor, stays well off its center, and remains ordinary infrastructure rather than
    // an objective marker. The same procedural kit is movable as a scene-block asset later.
    let route_yaw = dir.x.atan2(dir.y);
    for i in 0..12u32 {
        let t = 0.10 + i as f32 / 11.0 * 0.80;
        let stagger = (g.rnd(0xC17B_0000 + act.id as u32, i) - 0.5) * 5.0;
        let p = act.spawn + delta * t + side * (31.0 + stagger);
        if p.x.abs() > act.half_extent_m - 28.0 || p.y.abs() > act.half_extent_m - 28.0 {
            continue;
        }
        if p.length_squared() < exclude_r * exclude_r {
            continue;
        }
        g.place_first(
            &[Kit::TelegraphPole],
            p.x,
            p.y,
            -0.04,
            route_yaw,
            0.88 + g.rnd(0xC17B_1000 + act.id as u32, i) * 0.16,
        );
    }
    // Two handcarts per act are enough to establish the physical vocabulary without turning every
    // route into a repeated prop row.
    for (i, t) in [0.31f32, 0.69].into_iter().enumerate() {
        let p = act.spawn + delta * t - side * (14.0 + i as f32 * 4.0);
        if p.x.abs() <= act.half_extent_m - 24.0
            && p.y.abs() <= act.half_extent_m - 24.0
            && p.length_squared() >= exclude_r * exclude_r
        {
            g.place_first(
                &[Kit::HandCart],
                p.x,
                p.y,
                -0.03,
                route_yaw + if i == 0 { 0.18 } else { -0.27 },
                0.94,
            );
        }
    }

    // Every landmark acquires a human-scale apron: bins, chairs, cans, boards and vegetation. The
    // macro silhouette still navigates; the last thirty metres finally look inhabited and fought in.
    for (li, lm) in act.landmarks.iter().enumerate() {
        let household = if act.homeland {
            [
                Kit::Grass,
                Kit::Grass2,
                Kit::FurnChair,
                Kit::Bench,
                Kit::Bucket,
                Kit::WoodCrate,
            ]
        } else {
            [
                Kit::RubbleField,
                Kit::Brick,
                Kit::FurnChair,
                Kit::TrashCan,
                Kit::Board,
                Kit::WoodCrate,
            ]
        };
        g.scatter_patch(
            lm.pos.x,
            lm.pos.y,
            0x1A00_0000 ^ ((act.id as u32) << 16) ^ li as u32,
            18,
            34.0,
            &household,
            0.78,
            1.18,
            -0.05,
            exclude_r,
        );
        let a = g.rnd(0x1A10_0000 + act.id as u32, li as u32) * std::f32::consts::TAU;
        let d = 18.0 + g.rnd(0x1A20_0000 + act.id as u32, li as u32) * 10.0;
        let (tx, tz) = (lm.pos.x + a.cos() * d, lm.pos.y + a.sin() * d);
        if tx * tx + tz * tz >= exclude_r * exclude_r {
            g.place_first(
                &[Kit::TrashCan, Kit::FpsBarrel, Kit::Bucket],
                tx,
                tz,
                -0.04,
                a,
                0.82,
            );
        }
    }
}

fn act1_story(g: &mut Gen<'_>, exclude_r: f32) {
    // HAUPTBAHNHOF FIELD HOSPITAL: orderly bed rows dissolve into luggage and bodies toward the
    // west. The arrangement reads triage, evacuation and abandonment without a label or cutscene.
    let (hx, hz) = (-842.0, -258.0);
    for row in 0..3u32 {
        for col in 0..5u32 {
            let x = hx + col as f32 * 3.15;
            let z = hz + row as f32 * 3.4;
            let yaw = if row == 2 && col >= 3 {
                1.18
            } else {
                std::f32::consts::FRAC_PI_2
            };
            g.place_first(
                &[Kit::FurnBed, Kit::Board, Kit::FpsCrate],
                x,
                z,
                0.02,
                yaw,
                0.92,
            );
            if (row + col) % 2 == 0 {
                g.place_first(
                    &[Kit::Deadman, Kit::Luggage, Kit::RubbleField],
                    x + 0.3,
                    z,
                    0.06,
                    yaw,
                    1.0,
                );
            }
        }
    }
    for i in 0..7u32 {
        g.place_first(
            &[Kit::Luggage, Kit::WoodCrate, Kit::RubbleStack],
            hx - 8.0 - i as f32 * 2.2,
            hz + 2.0 + (i & 1) as f32 * 1.3,
            0.02,
            g.rnd(0xA101, i) * std::f32::consts::TAU,
            0.9,
        );
    }
    for i in 0..4u32 {
        g.place_first(
            &[Kit::FirstAid, Kit::FpsCrate, Kit::WoodCrate],
            hx + 2.0 + i as f32 * 2.4,
            hz - 4.0,
            0.04,
            0.0,
            0.9,
        );
    }
    g.place_first(
        &[Kit::Radio, Kit::FpsCrate],
        hx + 4.5,
        hz - 2.8,
        0.08,
        0.3,
        1.0,
    );

    // THE ABSURD DROP FIELD: intact canisters give way to split empties and useless household
    // debris. A few jerrycans sit tantalisingly among far more cans and boxes.
    let (dx, dz) = (-1340.0, -860.0);
    g.scatter_patch(
        dx,
        dz,
        0xA110,
        22,
        43.0,
        &[
            Kit::SupplyCrate,
            Kit::EmptySupplyCrate,
            Kit::FpsBarrel,
            Kit::WoodCrate,
        ],
        0.82,
        1.18,
        -0.04,
        exclude_r,
    );
    g.scatter_patch(
        dx,
        dz,
        0xA111,
        34,
        36.0,
        &[
            Kit::TinCan,
            Kit::CannedFood,
            Kit::Bottle,
            Kit::Board,
            Kit::Luggage,
        ],
        0.75,
        1.15,
        -0.01,
        exclude_r,
    );
    for i in 0..3u32 {
        g.place_first(
            &[Kit::Jerrycan, Kit::FpsBarrel, Kit::WoodCrate],
            dx - 12.0 + i as f32 * 8.5,
            dz + 5.0 - i as f32 * 3.0,
            0.0,
            g.rnd(0xA112, i) * std::f32::consts::TAU,
            1.0,
        );
    }

    // THE LAST BUTCHER SITE: meat, saw, axe and a blackened bucket around a plank work surface.
    // The missing animal is the point — hunger has reduced the column to tools and scraps.
    let (bx, bz) = (-615.0, 825.0);
    g.place_first(
        &[Kit::Board, Kit::Plank, Kit::WoodCrate],
        bx,
        bz,
        0.02,
        0.7,
        1.1,
    );
    g.place_first_pose(
        &[Kit::Axe, Kit::Shovel, Kit::Board],
        bx + 0.8,
        bz - 0.4,
        0.08,
        0.4,
        0.0,
        0.7,
        1.0,
    );
    g.place_first_pose(
        &[Kit::HandSaw, Kit::Axe, Kit::Plank],
        bx - 0.9,
        bz + 0.2,
        0.06,
        1.0,
        0.0,
        0.5,
        1.0,
    );
    for i in 0..5u32 {
        g.place_first(
            &[Kit::Meat, Kit::CannedFood, Kit::TinCan],
            bx - 1.0 + i as f32 * 0.45,
            bz - 0.8,
            0.04,
            g.rnd(0xA120, i) * std::f32::consts::TAU,
            0.9,
        );
    }
    g.place_first(
        &[Kit::Bucket, Kit::FpsBarrel],
        bx + 2.0,
        bz + 0.7,
        0.0,
        0.0,
        0.9,
    );
    g.place_first(
        &[Kit::Deadman, Kit::Luggage, Kit::RubbleField],
        bx + 3.2,
        bz - 1.4,
        0.04,
        2.2,
        1.0,
    );
    g.scatter_patch(
        bx,
        bz,
        0xA121,
        13,
        12.0,
        &[Kit::TinCan, Kit::Bottle, Kit::Board, Kit::Brick],
        0.72,
        1.05,
        -0.02,
        exclude_r,
    );

    // THE TURRET LINE: three T-34 hulls buried into the old front, with fresh Soviet stores laid
    // over older German rubble. The two material layers encode autumn attack beneath winter return.
    for i in 0..3u32 {
        let x = 510.0 + i as f32 * 38.0;
        let z = 365.0 + (i & 1) as f32 * 12.0;
        g.place_first(
            &[Kit::WreckT34, Kit::WreckPz3, Kit::RubbleStack],
            x,
            z,
            -0.85,
            -std::f32::consts::FRAC_PI_2 + g.rnd(0xA130, i) * 0.35,
            1.0,
        );
        g.scatter_patch(
            x,
            z,
            0xA131 + i,
            14,
            15.0,
            &[
                Kit::FpsCrate,
                Kit::SupplyCrate,
                Kit::RubbleField,
                Kit::Brick,
                Kit::Deadman,
            ],
            0.82,
            1.12,
            -0.04,
            exclude_r,
        );
    }

    // ROSSOSHKA'S ERASED VILLAGE: only chimney stems retain the house grid. Domestic scraps and
    // fresh rubble sit between them; repetition makes the absent buildings visible.
    let (vx, vz) = (-1640.0, 610.0);
    for row in 0..3u32 {
        for col in 0..3u32 {
            let x = vx + col as f32 * 14.0;
            let z = vz + row as f32 * 13.0;
            g.place_first(
                &[Kit::Chimney, Kit::PillarBroken, Kit::RubbleStack],
                x,
                z,
                -0.12,
                g.rnd(0xA140, row * 3 + col) * 0.12,
                0.34,
            );
            g.scatter_patch(
                x,
                z,
                0xA150 + row * 3 + col,
                7,
                7.5,
                &[
                    Kit::RubbleField,
                    Kit::Brick,
                    Kit::FurnChair,
                    Kit::TinCan,
                    Kit::Board,
                ],
                0.72,
                1.06,
                -0.05,
                exclude_r,
            );
        }
    }
}

const ACT2_OLD_HOUSE_LANDMARK_XZ: (f32, f32) = (100.0, 200.0);
// The tracked manifest already puts the ruined manor and two furniture pieces directly on the
// marker. Keep the additive breakfast tableau in a clear outer yard: (155,312) is 124.8 m from the
// marker (inside discovery range), but its 28 m scatter stays outside the runtime city's protected
// 320 m origin core. It also has no manifest structure or furnishing within 35 m.
const ACT2_OLD_HOUSE_TABLEAU_XZ: (f32, f32) = (155.0, 312.0);

fn act2_story(g: &mut Gen<'_>, exclude_r: f32) {
    // THE OLD HOUSE AFTER BREAKFAST: the room's furniture has spilled into the ruined front yard.
    // A luggage trail points west and spring grass has begun to occupy the domestic outline.
    let (hx, hz) = ACT2_OLD_HOUSE_TABLEAU_XZ;
    g.place_first(&[Kit::FurnTable, Kit::WoodCrate], hx, hz, 0.02, 0.15, 1.0);
    for (i, (ox, oz, yaw)) in [
        (-2.0, 0.0, 1.57),
        (2.0, 0.0, -1.57),
        (0.0, -2.0, 0.0),
        (0.0, 2.0, 3.14),
    ]
    .into_iter()
    .enumerate()
    {
        let tipped = i == 3;
        g.place_first_pose(
            &[Kit::FurnChair, Kit::FpsCrate],
            hx + ox,
            hz + oz,
            0.03,
            yaw,
            if tipped { 0.85 } else { 0.0 },
            0.0,
            1.0,
        );
    }
    g.place_first(
        &[Kit::FurnBed, Kit::Board],
        hx + 7.0,
        hz + 4.0,
        0.03,
        1.57,
        1.0,
    );
    g.place_first(
        &[Kit::FurnCabinet, Kit::WoodCrate],
        hx - 6.0,
        hz + 4.0,
        0.03,
        0.0,
        1.0,
    );
    g.place_first(
        &[Kit::MailBox, Kit::WoodCrate],
        hx - 10.0,
        hz - 8.0,
        0.0,
        0.3,
        1.0,
    );
    for i in 0..9u32 {
        g.place_first(
            &[Kit::Luggage, Kit::WoodCrate, Kit::Board],
            hx - 4.0 - i as f32 * 2.5,
            hz - 3.0 + (i & 1) as f32 * 1.2,
            0.01,
            g.rnd(0xA201, i) * 6.28,
            0.9,
        );
    }
    g.scatter_patch(
        hx,
        hz,
        0xA202,
        42,
        28.0,
        &[
            Kit::Grass,
            Kit::Grass2,
            Kit::Bottle,
            Kit::TinCan,
            Kit::Board,
        ],
        0.72,
        1.22,
        -0.05,
        exclude_r,
    );

    // CANAL BRIDGE REPAIR THAT NEVER FINISHED: fresh timber, tools and fuel surround two mired
    // wrecks. Barricades point the player toward the intact approach while telling why it matters.
    let (cx, cz) = (-1490.0, 515.0);
    g.place_first(
        &[Kit::WreckT34, Kit::WreckPz3, Kit::RubbleStack],
        cx - 14.0,
        cz + 8.0,
        -0.28,
        0.3,
        1.0,
    );
    g.place_first(
        &[Kit::WreckIs2, Kit::WreckT34, Kit::RubbleStack],
        cx + 18.0,
        cz - 10.0,
        -0.42,
        2.7,
        1.0,
    );
    for i in 0..10u32 {
        g.place_first(
            &[Kit::Plank, Kit::Board, Kit::WoodCrate],
            cx - 12.0 + i as f32 * 2.7,
            cz + (i % 3) as f32 * 1.1,
            0.02,
            1.57 + g.rnd(0xA210, i) * 0.18,
            1.0,
        );
    }
    for i in 0..4u32 {
        g.place_first(
            &[Kit::Barricade, Kit::Fence, Kit::RubbleStack],
            cx - 18.0 + i as f32 * 12.0,
            cz - 18.0,
            0.0,
            0.0,
            1.0,
        );
    }
    g.scatter_patch(
        cx,
        cz,
        0xA211,
        32,
        30.0,
        &[
            Kit::Jerrycan,
            Kit::Toolbox,
            Kit::Tire,
            Kit::Shovel,
            Kit::WoodCrate,
            Kit::Deadman,
            Kit::Grass,
        ],
        0.8,
        1.14,
        -0.02,
        exclude_r,
    );

    // THE BOYS' TRENCH: no labels, just undersized domestic traces among fighting tools — wheel
    // rims, shovels, civilian luggage and bodies behind a thin improvised line.
    let (tx, tz) = (-690.0, -805.0);
    for i in 0..8u32 {
        g.place_first(
            &[Kit::Fence, Kit::Barricade, Kit::RubbleStack],
            tx + i as f32 * 4.0,
            tz,
            -0.08,
            0.0,
            0.9,
        );
    }
    g.scatter_patch(
        tx + 14.0,
        tz + 5.0,
        0xA220,
        34,
        22.0,
        &[
            Kit::Tire,
            Kit::Shovel,
            Kit::Luggage,
            Kit::WoodCrate,
            Kit::Deadman,
            Kit::TinCan,
        ],
        0.78,
        1.08,
        -0.03,
        exclude_r,
    );
    g.scatter_patch(
        tx + 14.0,
        tz + 5.0,
        0xA221,
        24,
        27.0,
        &[Kit::Grass, Kit::Grass2, Kit::Stump],
        0.72,
        1.3,
        -0.07,
        exclude_r,
    );
}

fn act3_story(g: &mut Gen<'_>, exclude_r: f32) {
    // THE JAMMED FOREST ROAD: every vehicle and every dropped possession faces west. The geometry
    // itself supplies direction and scale before the player ever consults the map.
    let (jx, jz) = (-860.0, 315.0);
    let wrecks = [
        Kit::WreckPz3,
        Kit::WreckPanther,
        Kit::WreckT34,
        Kit::WreckTiger2,
        Kit::WreckIs2,
    ];
    for (i, wreck) in wrecks.into_iter().enumerate() {
        let x = jx + i as f32 * 19.0;
        let z = jz + if i & 1 == 0 { -3.5 } else { 3.5 };
        g.place_first(
            &[wreck, Kit::WreckPz3, Kit::RubbleStack],
            x,
            z,
            -0.12,
            -std::f32::consts::FRAC_PI_2 + g.rnd(0xA301, i as u32) * 0.22,
            1.0,
        );
    }
    g.scatter_patch(
        jx + 38.0,
        jz,
        0xA302,
        74,
        62.0,
        &[
            Kit::Luggage,
            Kit::WoodCrate,
            Kit::Tire,
            Kit::Jerrycan,
            Kit::Board,
            Kit::TinCan,
            Kit::Deadman,
            Kit::Toolbox,
        ],
        0.75,
        1.14,
        -0.03,
        exclude_r,
    );
    g.scatter_patch(
        jx + 38.0,
        jz,
        0xA303,
        42,
        68.0,
        &[Kit::Grass, Kit::Grass2, Kit::Stump, Kit::RubbleField],
        0.72,
        1.3,
        -0.07,
        exclude_r,
    );

    // THE AUTOBAHN BLOCKING LINE: a deliberate anti-tank closure, distinct from the refugee jam.
    // Wrecks are nose-in across the carriageway; tools and opened crates behind them imply a crew
    // that prepared the position and then disappeared. This second road scene now lives far enough
    // away that it must be found on its own rather than triggering with the column above.
    let (ax, az) = (650.0, -300.0);
    for (i, (wreck, ox, oz, yaw)) in [
        (Kit::WreckT34, -22.0, -5.0, 0.18),
        (Kit::WreckPanther, -7.0, 3.5, -0.12),
        (Kit::WreckPz3, 10.0, -3.0, 0.10),
        (Kit::WreckIs2, 26.0, 4.0, -0.16),
    ]
    .into_iter()
    .enumerate()
    {
        g.place_first(
            &[wreck, Kit::Barricade, Kit::RubbleStack],
            ax + ox,
            az + oz,
            -0.10,
            yaw + std::f32::consts::FRAC_PI_2,
            1.0,
        );
        g.place_first(
            &[Kit::Barricade, Kit::Fence, Kit::Board],
            ax + ox + 5.0,
            az + oz - 7.0,
            -0.04,
            yaw,
            1.0,
        );
        if i & 1 == 0 {
            g.place_first(
                &[Kit::Deadman, Kit::Luggage, Kit::RubbleField],
                ax + ox - 2.5,
                az + oz + 5.5,
                0.02,
                yaw,
                1.0,
            );
        }
    }
    g.scatter_patch(
        ax,
        az - 10.0,
        0xA306,
        42,
        38.0,
        &[
            Kit::EmptySupplyCrate,
            Kit::Toolbox,
            Kit::Tire,
            Kit::Shovel,
            Kit::Board,
            Kit::TinCan,
        ],
        0.78,
        1.12,
        -0.03,
        exclude_r,
    );

    // THE DRY TIGER: visually intact heavy armour, every fuel vessel empty, tow attempt abandoned.
    let (tx, tz) = (-1265.0, -75.0);
    g.place_first(
        &[Kit::WreckTiger2, Kit::WreckPanther, Kit::RubbleStack],
        tx,
        tz,
        -0.08,
        -std::f32::consts::FRAC_PI_2,
        1.0,
    );
    for i in 0..8u32 {
        g.place_first(
            &[Kit::Jerrycan, Kit::EmptySupplyCrate, Kit::FpsBarrel],
            tx - 8.0 + i as f32 * 2.1,
            tz + 6.0 + (i & 1) as f32,
            0.0,
            g.rnd(0xA310, i) * 6.28,
            0.9,
        );
    }
    for i in 0..5u32 {
        g.place_first(
            &[Kit::Plank, Kit::Board, Kit::WoodCrate],
            tx + 6.0 + i as f32 * 2.0,
            tz - 4.0,
            0.01,
            -1.57,
            1.0,
        );
    }
    g.place_first(
        &[Kit::Toolbox, Kit::WoodCrate],
        tx + 3.5,
        tz + 5.0,
        0.02,
        0.4,
        1.0,
    );
    g.place_first(
        &[Kit::Tire, Kit::FpsBarrel],
        tx + 6.0,
        tz + 5.5,
        0.0,
        0.2,
        1.0,
    );

    // WALDFRIEDHOF: small identical crosses follow the same firebreak grid used as roads. Spring
    // grass interrupts the rows, making the battlefield/cemetery identity visible in one frame.
    let (gx, gz) = (-1460.0, -245.0);
    for row in 0..6u32 {
        for col in 0..9u32 {
            let x = gx + col as f32 * 2.45;
            let z = gz + row as f32 * 2.85;
            g.place_first(
                &[Kit::GraveCross, Kit::Stump, Kit::Brick],
                x,
                z,
                -0.02,
                0.0,
                0.82,
            );
            if (row * 9 + col) % 3 == 0 {
                g.place_first(
                    &[Kit::Grass, Kit::Grass2, Kit::Stone],
                    x + 0.7,
                    z + 0.5,
                    -0.07,
                    g.rnd(0xA320, row * 9 + col) * 6.28,
                    0.78,
                );
            }
        }
    }

    // A second, smaller column at the rail halt mixes military wreckage with bedroom furniture —
    // civilians and army sharing the same narrowing road, the signature horror of Halbe.
    let (cx, cz) = (420.0, 455.0);
    g.place_first(&[Kit::WreckPz3, Kit::RubbleStack], cx, cz, -0.1, -1.57, 1.0);
    g.place_first(
        &[Kit::WreckPanther, Kit::WreckT34, Kit::RubbleStack],
        cx + 22.0,
        cz + 2.5,
        -0.1,
        -1.45,
        1.0,
    );
    g.scatter_patch(
        cx + 10.0,
        cz,
        0xA330,
        38,
        34.0,
        &[
            Kit::FurnBed,
            Kit::FurnChair,
            Kit::Luggage,
            Kit::WoodCrate,
            Kit::Deadman,
            Kit::Board,
        ],
        0.8,
        1.08,
        -0.03,
        exclude_r,
    );
}

/// Drain `(kit, spatial-cell)` buckets into one immutable prototype per kit while preserving the
/// exact sorted page stream. This removes repeated CPU mesh clones and repeated GPU
/// vertex/index/material uploads without merging culling cells or changing a single transform.
fn grouped_asset_pages(
    g: &mut Gen<'_>,
    collect_story_cover: bool,
) -> (
    Vec<InstancedAssetPages>,
    Vec<InstancedAssetPages>,
    Vec<ScatterAssetPages>,
    Vec<(Vec3, Vec3)>,
) {
    // HashMap uses a per-process random state. Sorting makes prototype, page, gameplay-object and
    // renderer object-ID order deterministic across runs.
    let mut drained: Vec<_> = g.buckets.drain().collect();
    drained.sort_by_key(|((kit, bx, bz), _)| (*kit as u16, *bx, *bz));

    let mut grouped: Vec<(Kit, Vec<Vec<Mat4>>)> = Vec::new();
    for ((kit, _bx, _bz), transforms) in drained {
        if let Some((last_kit, pages)) = grouped.last_mut() {
            if *last_kit == kit {
                pages.push(transforms);
                continue;
            }
        }
        grouped.push((kit, vec![transforms]));
    }

    let mut batches = Vec::new();
    let mut tree_batches = Vec::new();
    let mut scatter_batches = Vec::new();
    let mut cover_aabbs = Vec::new();
    for (kit, pages) in grouped {
        let Some(Some(mesh)) = g.cache.remove(&kit) else {
            continue;
        };
        if collect_story_cover && story_hard_cover(kit) {
            cover_aabbs.extend(
                pages
                    .iter()
                    .flatten()
                    .map(|model| transformed_aabb(&mesh, *model)),
            );
        }
        let asset = InstancedAssetPages { kit, mesh, pages };
        if matches!(kit, Kit::Pine | Kit::Pine2 | Kit::BareTree5 | Kit::BareTree7
            | Kit::DeadTree8 | Kit::DeadTree9) {
            tree_batches.push(asset);
        } else if let Some(profile) = scatter_profile(kit) {
            scatter_batches.push(ScatterAssetPages {
                kit,
                mesh: asset.mesh,
                pages: asset.pages,
                profile,
            });
        } else {
            batches.push(asset);
        }
    }
    (batches, tree_batches, scatter_batches, cover_aabbs)
}

/// Deterministic, additive environmental-storytelling layer for the campaign acts.
///
/// Unlike `generate`, this must be called even when a worldbuilder manifest loaded successfully:
/// manifests own the macro world; this function supplies the always-on human evidence and
/// act-specific micro-dioramas. Output retains those spatial pages while grouping them behind one
/// immutable prototype per kit.
/// Act 0 / the dev garage intentionally returns an empty layer.
pub fn generate_story_overlay(
    act: &crate::world::ActMap,
    hf: &Heightfield,
    exclude_r: f32,
) -> GenWorld {
    if act.id == 0 || act.garage {
        return GenWorld {
            batches: Vec::new(),
            tree_batches: Vec::new(),
            scatter_batches: Vec::new(),
            cover_aabbs: Vec::new(),
            placements: 0,
        };
    }

    let mut g = Gen {
        hf,
        seed: act.seed ^ 0xE57A_0A11,
        cache: HashMap::new(),
        buckets: HashMap::new(),
        placements: 0,
        missing: 0,
    };
    route_clutter(&mut g, act, exclude_r);
    seasonal_boundary_groves(&mut g, act, exclude_r);
    match act.id {
        1 => act1_story(&mut g, exclude_r),
        2 => act2_story(&mut g, exclude_r),
        3 => act3_story(&mut g, exclude_r),
        _ => {}
    }

    let (batches, tree_batches, scatter_batches, cover_aabbs) = grouped_asset_pages(&mut g, true);
    let page_count = batches.iter().map(|asset| asset.pages.len()).sum::<usize>()
        + tree_batches
            .iter()
            .map(|asset| asset.pages.len())
            .sum::<usize>()
        + scatter_batches
            .iter()
            .map(|asset| asset.pages.len())
            .sum::<usize>();
    println!(
        "[worldgen/story] act {} '{}': {} placements in {} deterministic instanced pages over {} shared prototypes{}",
        act.id,
        act.name,
        g.placements,
        page_count,
        batches.len() + tree_batches.len() + scatter_batches.len(),
        if g.missing > 0 {
            format!(", {} optional assets missing/fell back", g.missing)
        } else {
            String::new()
        },
    );
    GenWorld {
        batches,
        tree_batches,
        scatter_batches,
        cover_aabbs,
        placements: g.placements,
    }
}

/// Generate the act-scale district for `act`. `exclude_r` keeps a clear circle around the origin
/// where the hand-tuned combat block (city::generate + dressing) already lives — the district
/// surrounds it rather than stacking on top.
pub fn generate(act: &crate::world::ActMap, hf: &Heightfield, exclude_r: f32) -> GenWorld {
    let mut g = Gen {
        hf,
        seed: act.seed ^ 0x00D1_57C1, // distinct stream from city.rs (same act seed family)
        cache: HashMap::new(),
        buckets: HashMap::new(),
        placements: 0,
        missing: 0,
    };
    let r = act.ruination;

    // 1. THE CITY DISTRICT: a dense shelled grid centred east of the origin (the escape's start),
    //    thinning raggedly at its edge. Blocks inside the exclusion circle are skipped.
    for bj in -7i32..=7 {
        for bi in -7i32..=7 {
            let cx = 600.0 + bi as f32 * PITCH;
            let cz = 100.0 + bj as f32 * PITCH;
            if (cx * cx + cz * cz).sqrt() < exclude_r {
                continue;
            }
            let bid = ((bj as u32 & 0xffff).wrapping_mul(73_856_093))
                ^ ((bi as u32 & 0xffff).wrapping_mul(19_349_663));
            let edge = bi.abs().max(bj.abs()) as f32 / 7.0;
            if g.rnd(bid, 1) < 0.25 * edge {
                continue;
            }
            g.building(cx, cz, bid, (r + 0.15 * edge).min(1.0));
        }
    }
    // 2. WEST FRINGE: villages + pines thinning toward the goal.
    for k in 0..60u32 {
        let x = -600.0 - hash01(g.seed ^ k.wrapping_mul(7919)) * 1400.0;
        let z = (hash01(g.seed ^ k.wrapping_mul(104_729)) - 0.5) * 3600.0;
        let v = hash01(g.seed ^ k.wrapping_mul(65_537));
        if v < 0.30 {
            g.building(x, z, 0xB00 + k, r * 0.6);
        } else if v < 0.45 {
            let gy = g.ground(x, z) - 0.2;
            g.place(
                Kit::Shed,
                x,
                gy,
                z,
                hash01(k * 31) * std::f32::consts::TAU,
                1.0,
            );
        } else {
            let gy = g.ground(x, z) - 0.1;
            let kind = if v < 0.8 { Kit::Pine } else { Kit::Pine2 };
            g.place(
                kind,
                x,
                gy,
                z,
                hash01(k * 17) * std::f32::consts::TAU,
                0.8 + hash01(k * 13) * 0.7,
            );
        }
    }
    // 3. A couple of landmark-adjacent big volumes (warehouse district anchor + garage row).
    let gy = g.ground(880.0, 1080.0) - 0.3;
    g.place(Kit::Warehouse, 880.0, gy, 1080.0, 0.0, 2.2);
    let gy2 = g.ground(-880.0, -320.0) - 0.3;
    g.place(
        Kit::Garage,
        -880.0,
        gy2,
        -320.0,
        std::f32::consts::FRAC_PI_2,
        1.2,
    );
    // 4. THE BUNKER, under the district's south-west edge.
    g.bunker(320.0, -420.0);

    // Keep spatial pages independent while sharing each kit's immutable GPU prototype.
    let (batches, tree_batches, scatter_batches, _) = grouped_asset_pages(&mut g, false);
    let page_count = batches.iter().map(|asset| asset.pages.len()).sum::<usize>()
        + tree_batches
            .iter()
            .map(|asset| asset.pages.len())
            .sum::<usize>()
        + scatter_batches
            .iter()
            .map(|asset| asset.pages.len())
            .sum::<usize>();
    let prototype_count = batches.len() + tree_batches.len() + scatter_batches.len();
    println!(
        "[worldgen] act {} '{}': {} placements in {} instanced pages ({} shared prototypes{})",
        act.id,
        act.name,
        g.placements,
        page_count,
        prototype_count,
        if g.missing > 0 {
            format!(", {} MISSING", g.missing)
        } else {
            String::new()
        }
    );
    GenWorld {
        batches,
        tree_batches,
        scatter_batches,
        cover_aabbs: Vec::new(),
        placements: g.placements,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pizza_worldgen_assets_installed() -> bool {
        if crate::resource::is_file(path_of(Kit::Wall)) {
            true
        } else {
            eprintln!("skipping: untracked Pizza worldgen assets are not installed");
            false
        }
    }

    fn flat_hf() -> Heightfield {
        Heightfield {
            res: 16,
            spacing: 400.0,
            heights: vec![0.0; 256],
            min_h: 0.0,
            max_h: 0.0,
            openings: Default::default(), detail: None,
        }
    }

    fn proxy_test_mesh() -> CpuMesh {
        crate::prims::box_mesh(
            Vec3::new(-2.0, 0.0, -0.2),
            Vec3::new(2.0, 3.0, 0.2),
            [128, 128, 128],
        )
    }

    fn point_in_any(point: Vec3, bounds: &[(Vec3, Vec3)]) -> bool {
        bounds
            .iter()
            .any(|(min, max)| (point.cmpge(*min) & point.cmple(*max)).all())
    }

    #[test]
    fn act2_old_house_tableau_is_discoverable_and_clear_of_the_runtime_core() {
        assert_eq!(ACT2_OLD_HOUSE_LANDMARK_XZ, (100.0, 200.0));
        let dx = ACT2_OLD_HOUSE_TABLEAU_XZ.0 - ACT2_OLD_HOUSE_LANDMARK_XZ.0;
        let dz = ACT2_OLD_HOUSE_TABLEAU_XZ.1 - ACT2_OLD_HOUSE_LANDMARK_XZ.1;
        let distance = dx.hypot(dz);
        assert_eq!(ACT2_OLD_HOUSE_TABLEAU_XZ, (155.0, 312.0));
        assert!(
            (12.0..=125.0).contains(&distance),
            "tableau must clear the 12 m manor footprint but remain discoverable: {distance:.1} m"
        );
        assert!(
            ACT2_OLD_HOUSE_TABLEAU_XZ
                .0
                .hypot(ACT2_OLD_HOUSE_TABLEAU_XZ.1)
                - 28.0
                >= 320.0,
            "the whole tableau scatter must stay outside the protected runtime core"
        );
    }

    #[test]
    fn manifest_collision_vocabulary_excludes_flat_and_decorative_assets() {
        let mesh = proxy_test_mesh();
        for path in [
            path_of(Kit::Floor),
            path_of(Kit::FloorHole),
            path_of(Kit::RoofPitch),
            path_of(Kit::RoofFlat),
            path_of(Kit::Stairs),
            path_of(Kit::FurnBed),
            path_of(Kit::FurnTable),
            path_of(Kit::RubbleField),
            path_of(Kit::PillarBroken),
            path_of(Kit::Pine),
            path_of(Kit::Grass),
        ] {
            assert!(
                manifest_structure_proxies(&path, &mesh, Mat4::IDENTITY).is_empty(),
                "decorative/flat manifest asset became hidden collision: {path}"
            );
        }
        assert_eq!(
            manifest_structure_proxies(&path_of(Kit::Wall), &mesh, Mat4::IDENTITY).len(),
            1
        );
        assert_eq!(
            manifest_structure_proxies(&path_of(Kit::Pillar), &mesh, Mat4::IDENTITY).len(),
            1
        );
    }

    #[test]
    fn manifest_doorway_proxies_leave_the_visible_ground_entrance_open() {
        let mesh = proxy_test_mesh();
        let proxies = manifest_structure_proxies(&path_of(Kit::Doorway), &mesh, Mat4::IDENTITY);
        assert_eq!(proxies.len(), 3);
        assert!(!point_in_any(Vec3::new(0.0, 1.4, 0.0), &proxies));
        assert!(point_in_any(Vec3::new(-1.4, 1.4, 0.0), &proxies));
        assert!(point_in_any(Vec3::new(0.0, 2.7, 0.0), &proxies));

        let through = crate::building_runtime::resolve_walk_xz_against_bounds(
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(0.0, 0.0, 1.0),
            &proxies,
        );
        assert!(
            through.z > 0.99,
            "door aperture was invisibly sealed: {through:?}"
        );
        let jamb = crate::building_runtime::resolve_walk_xz_against_bounds(
            Vec3::new(-1.4, 0.0, -1.0),
            Vec3::new(-1.4, 0.0, 1.0),
            &proxies,
        );
        assert!(
            jamb.z < -0.45,
            "visible doorway jamb was not physical: {jamb:?}"
        );
    }

    #[test]
    fn legacy_one_piece_buildings_are_hollow_and_enterable_not_union_boxes() {
        let mesh = proxy_test_mesh();
        for (kit, entrance) in [
            (Kit::Warehouse, Vec3::new(0.0, 1.3, 6.25)),
            (Kit::Garage, Vec3::new(0.0, 1.3, 2.95)),
            (Kit::Shed, Vec3::new(0.0, 1.3, 0.95)),
        ] {
            assert!(manifest_uses_compatibility_shell(&path_of(kit)));
            let proxies = manifest_structure_proxies(&path_of(kit), &mesh, Mat4::IDENTITY);
            assert_eq!(proxies.len(), 6, "{kit:?}");
            assert!(!point_in_any(entrance, &proxies), "{kit:?} entrance sealed");
            assert!(
                !point_in_any(Vec3::new(0.0, 1.3, 0.0), &proxies),
                "{kit:?} interior became a solid union AABB"
            );
        }
        assert!(!manifest_uses_compatibility_shell(&path_of(Kit::Wall)));
    }

    #[test]
    fn tracked_manifest_structure_proxy_counts_remain_bounded() {
        let mesh = proxy_test_mesh();
        for act_id in 1..=3 {
            let path = crate::act_manifest::path_for_act(act_id);
            let raw = crate::resource::read(&path).expect("tracked act manifest is readable");
            let value: serde_json::Value =
                serde_json::from_slice(&raw).expect("tracked act manifest parses");
            let assets: Vec<_> = value["assets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|asset| asset.as_str().unwrap())
                .collect();
            let mut structures = 0usize;
            let mut proxies = 0usize;
            for placement in value["placements"].as_array().unwrap() {
                let asset = placement[0].as_u64().unwrap() as usize;
                let count = manifest_structure_proxies(assets[asset], &mesh, Mat4::IDENTITY).len();
                structures += usize::from(count > 0);
                proxies += count;
            }
            assert!(structures > 500, "act {act_id} lost its physical town");
            assert!(
                proxies <= 12_000,
                "act {act_id} collision budget regressed to {proxies} linear proxies"
            );
            assert!(
                proxies >= structures,
                "every physical structure needs a proxy"
            );
        }
    }

    #[test]
    fn retired_manifest_model_ids_translate_to_local_semantic_recipes() {
        assert_eq!(
            replacement_kit_for_retired_path(RETIRED_DEADMAN_GLB),
            Some(Kit::Deadman)
        );
        for (path, expected) in RETIRED_WRECK_GLBS {
            assert_eq!(replacement_kit_for_retired_path(path), Some(expected));
        }
        for (part_index, path) in RETIRED_HALF_TIGER_GLBS.into_iter().enumerate() {
            assert_eq!(retired_tiger_part_index(path), Some(part_index));
            assert!(is_retired_semantic_model_path(path));
        }
        assert_eq!(
            replacement_kit_for_retired_path(path_of(Kit::Wall).as_str()),
            None
        );
        for kit in [
            Kit::Deadman,
            Kit::WreckT34,
            Kit::WreckPz3,
            Kit::WreckPanther,
            Kit::WreckTiger2,
            Kit::WreckIs2,
            Kit::WreckKv1,
        ] {
            let recipe = path_of(kit);
            assert!(
                recipe.starts_with("procedural:worldgen/"),
                "{kit:?}: {recipe}"
            );
            assert!(
                !recipe.ends_with(".glb"),
                "{kit:?} still selects an imported model"
            );
        }
    }

    #[test]
    fn half_tiger_manifest_ids_resolve_to_current_tiger_parts() {
        if !CURRENT_TIGER_PART_GLBS
            .iter()
            .all(|path| crate::resource::is_file(path))
        {
            eprintln!("skipping: current Tiger model payload is not installed");
            return;
        }
        for (index, retired) in RETIRED_HALF_TIGER_GLBS.into_iter().enumerate() {
            let replacement = load_module(retired).expect("semantic Tiger replacement loads");
            let expected = crate::gltf_load::load_glb(CURRENT_TIGER_PART_GLBS[index])
                .map(crate::tank::repaint_german_grey)
                .expect("current Tiger part loads");
            assert_eq!(
                replacement.vertices.len(),
                expected.vertices.len(),
                "{retired}"
            );
            assert_eq!(replacement.indices, expected.indices, "{retired}");
            assert_eq!(replacement.min, expected.min, "{retired}");
            assert_eq!(replacement.max, expected.max, "{retired}");
            assert!(replacement
                .vertices
                .iter()
                .zip(&expected.vertices)
                .all(|(left, right)| left.pos == right.pos && left.normal == right.normal));
            assert_eq!(
                replacement.texture.as_ref().map(|image| &image.rgba),
                expected.texture.as_ref().map(|image| &image.rgba),
                "{retired}"
            );
        }
    }

    #[test]
    fn procedural_corpse_is_a_grounded_deterministic_psx_box_assembly() {
        let a = procedural_corpse_mesh();
        let b = procedural_corpse_mesh();
        assert_eq!(a.vertices.len(), 6 * 24);
        assert_eq!(a.indices.len(), 6 * 36);
        assert_eq!(a.vertices.len(), b.vertices.len());
        assert_eq!(a.indices, b.indices);
        assert_eq!(a.min, Vec3::new(-0.46, 0.0, -0.95));
        assert_eq!(a.max, Vec3::new(0.46, 0.26, 0.66));
        assert!(a
            .vertices
            .iter()
            .zip(&b.vertices)
            .all(|(left, right)| left.pos == right.pos && left.normal == right.normal));
        let texture = a
            .texture
            .expect("procedural corpse has its own solid material");
        assert_eq!((texture.width, texture.height), (1, 1));
        assert_eq!(texture.rgba.as_slice(), [62, 66, 60, 255]);
    }

    #[test]
    fn retained_tank_wrecks_are_charred_cached_geometry_with_pizza_dressing() {
        if !pizza_worldgen_assets_installed()
            || !crate::resource::is_file("assets/tanks/tiger/baked/tiger_hull.glb")
            || !crate::resource::is_file("assets/tanks/T34-76/baked/t34_hull.glb")
        {
            eprintln!("skipping: retained tank/Pizza model payload is not installed");
            return;
        }
        let t34 = retained_wreck_mesh(Kit::WreckT34);
        let tiger = retained_wreck_mesh(Kit::WreckPz3);
        for (mesh, color) in [(t34, [31, 35, 30, 255]), (tiger, [36, 34, 30, 255])] {
            assert!(mesh.vertices.len() > 100);
            assert!(mesh.indices.len() > 100);
            assert!(mesh.max.x - mesh.min.x > 2.0);
            assert!(mesh.max.z - mesh.min.z > 4.0);
            let texture = mesh
                .texture
                .expect("charred wreck has a one-pixel material");
            assert_eq!((texture.width, texture.height), (1, 1));
            assert_eq!(texture.rgba.as_slice(), color);
        }

        let hf = flat_hf();
        let mut g = Gen {
            hf: &hf,
            seed: 0xC0FF_EE11,
            cache: HashMap::new(),
            buckets: HashMap::new(),
            placements: 0,
            missing: 0,
        };
        g.place(Kit::WreckT34, 900.0, 0.0, 700.0, 0.4, 1.0);
        assert_eq!(
            g.placements, 5,
            "one wreck + four deterministic Pizza debris pieces"
        );
        assert_eq!(g.missing, 0);
        let placed: std::collections::HashSet<_> =
            g.buckets.keys().map(|(kit, _, _)| *kit).collect();
        assert!(placed.contains(&Kit::WreckT34));
        assert!(placed.contains(&Kit::RubbleField));
        assert!(placed.contains(&Kit::RubbleStack));
        assert!(placed.contains(&Kit::Jerrycan));
        assert!(placed.contains(&Kit::Board));
    }

    #[test]
    fn generates_a_dense_deterministic_district() {
        if !pizza_worldgen_assets_installed() {
            return;
        }
        let hf = flat_hf();
        let act = crate::world::act(1);
        let a = generate(&act, &hf, 250.0);
        let b = generate(&act, &hf, 250.0);
        assert!(
            a.placements > 3000,
            "act-scale district should be thousands of placements (got {})",
            a.placements
        );
        assert_eq!(a.placements, b.placements, "same seed → identical world");
        // Bucketing keeps pages cullable even though immutable prototypes are shared.
        let page_count = a.batches.iter().flat_map(|asset| &asset.pages).count();
        assert!(
            page_count > 30,
            "expected spatially bucketed pages (got {page_count})"
        );
        let biggest = a
            .batches
            .iter()
            .flat_map(|asset| &asset.pages)
            .map(Vec::len)
            .max()
            .unwrap_or(0);
        assert!(
            biggest < a.placements / 2,
            "no single batch should hold most of the world"
        );
        // The exclusion circle stays clear (the hand-tuned combat block lives there).
        for asset in &a.batches {
            for mats in &asset.pages {
                for m in mats {
                    let p = m.transform_point3(glam::Vec3::ZERO);
                    let horiz = (p.x * p.x + p.z * p.z).sqrt();
                    assert!(
                        horiz > 230.0 || p.y < -1.0,
                        "placement at {p:?} inside the exclusion circle"
                    );
                }
            }
        }
    }

    #[test]
    fn story_overlay_is_always_additive_bucketed_and_act_specific() {
        if !pizza_worldgen_assets_installed() {
            return;
        }
        let hf = flat_hf();
        let exclude_r = 320.0;
        let mut counts = Vec::new();
        for id in 1..=3 {
            let act = crate::world::act(id);
            let w = generate_story_overlay(&act, &hf, exclude_r);
            assert!(
                w.placements > 300,
                "act {id} should add hundreds of human-scale story props (got {})",
                w.placements
            );
            let page_count = w.batches.iter().flat_map(|asset| &asset.pages).count();
            assert!(
                page_count > 20,
                "act {id} story props should remain spatially bucketed (got {page_count})"
            );
            assert!(
                !w.scatter_batches.is_empty(),
                "act {id} should register lightweight story props as pushables"
            );
            assert!(
                !w.cover_aabbs.is_empty(),
                "act {id} should register its wrecks and barricades as hard cover"
            );
            assert!(w
                .cover_aabbs
                .iter()
                .all(|(min, max)| { min.is_finite() && max.is_finite() && max.cmpgt(*min).any() }));
            let biggest = w
                .batches
                .iter()
                .flat_map(|asset| &asset.pages)
                .map(Vec::len)
                .chain(
                    w.scatter_batches
                        .iter()
                        .flat_map(|asset| &asset.pages)
                        .map(Vec::len),
                )
                .max()
                .unwrap_or(0);
            assert!(
                biggest < w.placements / 3,
                "act {id} unexpectedly collapsed most story props into one batch"
            );
            for asset in &w.batches {
                for mats in &asset.pages {
                    for m in mats {
                        let p = m.transform_point3(Vec3::ZERO);
                        assert!(
                            p.x * p.x + p.z * p.z >= (exclude_r - 0.1) * (exclude_r - 0.1),
                            "act {id} story placement {p:?} invaded the protected combat core"
                        );
                    }
                }
            }
            for asset in &w.scatter_batches {
                for page in &asset.pages {
                    for m in page {
                        let p = m.transform_point3(Vec3::ZERO);
                        assert!(
                            p.x * p.x + p.z * p.z >= (exclude_r - 0.1) * (exclude_r - 0.1),
                            "act {id} pushable placement {p:?} invaded the protected combat core"
                        );
                    }
                }
            }
            counts.push(w.placements);
        }
        assert!(
            counts[0] != counts[1] || counts[1] != counts[2],
            "acts should not receive a copy-pasted overlay: {counts:?}"
        );

        let arena = generate_story_overlay(&crate::world::act(0), &hf, exclude_r);
        assert_eq!(
            arena.placements, 0,
            "training arena stays free of campaign dioramas"
        );
        assert!(
            arena.batches.is_empty()
                && arena.tree_batches.is_empty()
                && arena.scatter_batches.is_empty()
        );
    }

    #[test]
    fn seasonal_bare_groves_route_as_trees_and_preserve_source_wood() {
        if !pizza_worldgen_assets_installed() { return; }
        for (kit, leaves) in [(Kit::BareTree5, "tree_5_leaves"), (Kit::BareTree7, "tree_7_leaves")] {
            assert!(crate::trees::is_live_tree_asset(&path_of(kit)));
            let whole = crate::gltf_load::load_glb(&path_of(kit)).unwrap();
            assert!(crate::gltf_load::load_glb_partitioned_roots(&path_of(kit), &[leaves]).is_err(),
                "the Tiger root-only API must keep rejecting descendant selections");
            let pages = crate::gltf_load::load_glb_partitioned_subtrees(&path_of(kit), &[leaves]).unwrap();
            let bare = load_kit(kit).unwrap();
            assert_eq!(bare.indices, pages[0].indices);
            assert_eq!(bare.vertices.len(), pages[0].vertices.len());
            assert_eq!(bare.indices.len() + pages[1].indices.len(), whole.indices.len());
            // Source traversal visits the woody trunk first. Selecting its descendant must not
            // drop the trunk's transform, change its texture coordinates or alter its geometry.
            for (wood, source) in bare.vertices.iter().zip(&whole.vertices) {
                assert_eq!(wood.pos, source.pos);
                assert_eq!(wood.normal, source.normal);
                assert_eq!(wood.uv, source.uv);
            }
            assert!(!bare.indices.is_empty() && bare.indices.len() < whole.indices.len());
            assert!(bare.max.y > 4.0 && bare.min.y.abs() < 0.05);
        }
        let hf = flat_hf();
        let act = crate::world::act(1);
        let mut g = Gen { hf: &hf, seed: act.seed ^ 0xE57A_0A11,
            cache: HashMap::new(), buckets: HashMap::new(), placements: 0, missing: 0 };
        seasonal_boundary_groves(&mut g, &act, 320.0);
        let (_, trees, _, _) = grouped_asset_pages(&mut g, false);
        let count: usize = trees.iter().flat_map(|asset| &asset.pages).map(Vec::len).sum();
        assert!((40..=126).contains(&count), "sparse bare winter trees: {count}");
        assert!(trees.iter().any(|asset| matches!(asset.kit, Kit::Pine | Kit::Pine2)));
        assert!(trees.iter().any(|asset| matches!(asset.kit, Kit::BareTree5 | Kit::BareTree7)));
        for matrix in trees.iter().flat_map(|asset| &asset.pages).flatten() {
            let p = matrix.w_axis.truncate();
            assert!(p.x * p.x + p.z * p.z >= 320.0f32.powi(2));
        }
    }

    #[test]
    fn story_overlay_transform_stream_is_deterministic() {
        if !pizza_worldgen_assets_installed() {
            return;
        }
        let hf = flat_hf();
        let act = crate::world::act(1);
        let a = generate_story_overlay(&act, &hf, 320.0);
        let b = generate_story_overlay(&act, &hf, 320.0);
        assert_eq!(a.placements, b.placements);
        assert_eq!(a.batches.len(), b.batches.len());
        assert_eq!(a.scatter_batches.len(), b.scatter_batches.len());
        for (aa, ba) in a.batches.iter().zip(&b.batches) {
            assert_eq!(aa.kit, ba.kit);
            assert_eq!(aa.pages.len(), ba.pages.len());
            for (am, bm) in aa.pages.iter().zip(&ba.pages) {
                assert_eq!(am.len(), bm.len());
                for (ax, bx) in am.iter().zip(bm) {
                    assert_eq!(
                        ax.to_cols_array(),
                        bx.to_cols_array(),
                        "same act seed must produce a byte-identical transform stream"
                    );
                }
            }
        }
        for (ap, bp) in a.scatter_batches.iter().zip(&b.scatter_batches) {
            assert_eq!(ap.kit, bp.kit);
            assert_eq!(ap.profile, bp.profile);
            assert_eq!(ap.pages.len(), bp.pages.len());
            for (am, bm) in ap.pages.iter().zip(&bp.pages) {
                assert_eq!(am.len(), bm.len());
                for (ax, bx) in am.iter().zip(bm) {
                    assert_eq!(
                        ax.to_cols_array(),
                        bx.to_cols_array(),
                        "same act seed must produce byte-identical pushable transforms"
                    );
                }
            }
        }
    }

    #[test]
    fn lightweight_scatter_never_becomes_static_cover() {
        assert_eq!(
            scatter_profile(Kit::Fence),
            Some(ScatterProfile {
                motion: ScatterMotion::Hinge,
                material: ScatterMaterial::Wood,
            })
        );
        assert_eq!(
            scatter_profile(Kit::FpsBarrel).map(|profile| profile.motion),
            Some(ScatterMotion::Roll)
        );
        assert_eq!(
            scatter_profile(Kit::HandCart).map(|profile| profile.motion),
            Some(ScatterMotion::Loose)
        );
        assert!(!story_hard_cover(Kit::Fence));
        assert!(story_hard_cover(Kit::Barricade));
        assert!(story_hard_cover(Kit::WreckTiger2));

        let timber_fence = path_of(Kit::Fence);
        assert_eq!(
            manifest_scatter_profile(&timber_fence).map(|profile| profile.motion),
            Some(ScatterMotion::Hinge)
        );
        assert!(
            manifest_scatter_profile("assets/kit/Modular Props/concrete_fence_hr_1.glb").is_none()
        );

        let pole = procedural_telegraph_pole_mesh();
        assert!(pole.min.y.abs() < 1.0e-4 && pole.max.y > 6.0);
        let cart = procedural_hand_cart_mesh();
        assert!(cart.min.y >= -0.01 && cart.max.z > 1.8);
    }
}
