//! Building kit: the registry from a blueprint module's semantic asset id to its source mesh and
//! material, shared by every generated building.
//!
//! Owned by the building grammar of the urban-intro team (docs/URBAN_INTRO_TEAM_20260928.md). The
//! first nine entries are the Pizza modules of the featured house; `building_runtime` keeps a
//! private copy of them until the runtime reads this registry. A new entry needs a runtime-catalog
//! review (docs/RUNTIME_ASSET_CATALOG.md), and its collision proxies are generated from its source
//! geometry with a measured error bound (CLAUDE.md, second principle, rule 3).
//!
//! Every kit asset is built only from reviewed Pizza GLBs (all are in
//! `catalog/runtime_assets.v1.json`, consumer `code.building-kit`):
//! - a whole GLB;
//! - a GLB restricted to some of its source primitives, selected by glTF material name (a window
//!   wall without its glazing, a gate frame without its leaf) — geometry and UVs are untouched;
//! - a composite: several such parts, each placed by a scale, a quarter-turn rotation about Y and a
//!   translation, merged into one mesh (a timber window bay from plank pieces, a dog-leg stair from
//!   two flights and a landing). A composite's parts must share one texture family, because the
//!   engine's loader renders a mesh with its first primitive's texture (gltf_load §material).
//!
//! Collision proxies (`proxies`) are axis-aligned boxes generated from the loaded source triangles
//! by conservative voxelisation, a documented fill rule, greedy box merging and snapping to the
//! mesh's own vertex planes. Each asset records the error bound its test asserts; the measured
//! value is the larger of the proxies' over-coverage (distance of covered points to the source
//! surface) and under-coverage (distance of uncovered solid to the proxies) on a grid of half the
//! voxel size, plus that grid's half-diagonal. Proxies are exact AABBs under the grammar's module
//! transforms, which scale non-uniformly and rotate only by quarter turns about Y; the world error
//! is the recorded bound times the module's largest scale factor.
#![allow(dead_code)] // wired by the runtime builder of the urban-intro team

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use glam::{EulerRot, Mat4, Quat, Vec3};

use crate::gltf_load::{CpuMesh, PrimitiveFilter};

/// Material class of a kit module, consumed by destruction (crush tier, shell damage) and sound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KitMaterial {
    Brick,
    Stone,
    Timber,
    Concrete,
    Metal,
}

/// How the solid behind the voxelised surface is filled before boxes are merged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProxyFill {
    /// Surface voxels only: sheets, rods and railings.
    Surface,
    /// Surface plus every voxel enclosed by it (closed meshes).
    Enclosed,
    /// Solid between the first and last surface voxel of every line along the axis: walls
    /// (between their faces, `Z`), slabs and flights (between top and soffit, `Y`), piers.
    AlongX,
    AlongY,
    AlongZ,
}

/// Proxy generation rule of one kit part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProxySpec {
    pub voxel_m: f32,
    pub fill: ProxyFill,
    /// Error bound in the part's source space that the kit test asserts.
    pub max_error_m: f32,
}

// Bounds by shape class, at 5 cm voxels except roof sheets (the reference grid is half the voxel,
// 2.5 cm, whose half-diagonal alone is 0.022 m). Axis-aligned faces snap onto the source planes; the remaining error comes from
// sloped or curved surfaces stepped at voxel size, sills and mouldings thinner than a voxel, and
// rods thinner than a voxel.
const WALL_PROXY: ProxySpec = ProxySpec {
    voxel_m: 0.05,
    fill: ProxyFill::AlongZ,
    max_error_m: 0.05,
};
/// Window walls: their 5 cm metal outer sill projects 0.1 m beyond the face.
const WINDOW_WALL_PROXY: ProxySpec = ProxySpec {
    voxel_m: 0.05,
    fill: ProxyFill::AlongZ,
    max_error_m: 0.08,
};
/// Walls with a sloped edge (gables) or diagonal planks (a boarded doorway).
const SLOPED_WALL_PROXY: ProxySpec = ProxySpec {
    voxel_m: 0.05,
    fill: ProxyFill::AlongZ,
    max_error_m: 0.11,
};
const SLAB_PROXY: ProxySpec = ProxySpec {
    voxel_m: 0.05,
    fill: ProxyFill::AlongY,
    max_error_m: 0.05,
};
/// Slabs with a curved hole, and rubble mounds.
const SLOPED_SLAB_PROXY: ProxySpec = ProxySpec {
    voxel_m: 0.05,
    fill: ProxyFill::AlongY,
    max_error_m: 0.11,
};
const PIER_PROXY: ProxySpec = ProxySpec {
    voxel_m: 0.05,
    fill: ProxyFill::AlongX,
    max_error_m: 0.06,
};
/// Stacks and piers with a moulded band.
const STACK_PROXY: ProxySpec = ProxySpec {
    voxel_m: 0.05,
    fill: ProxyFill::AlongX,
    max_error_m: 0.08,
};
/// Stair flights: horizontal treads snap exactly; the sloped soffit is stepped.
const FLIGHT_PROXY: ProxySpec = ProxySpec {
    voxel_m: 0.05,
    fill: ProxyFill::AlongY,
    max_error_m: 0.11,
};
/// Pitched and curved roof sheets, at 10 cm voxels: a sloped sheet steps once per voxel, and at
/// 5 cm a single roof section needed 390-1027 boxes, which every runtime query against thousands
/// of modules would pay for. At 10 cm the stepped sheet stays within 0.23 m of the source.
const SHEET_PROXY: ProxySpec = ProxySpec {
    voxel_m: 0.1,
    fill: ProxyFill::Surface,
    max_error_m: 0.23,
};
/// Rods, railings, slatted crates and bedsteads thinner than a voxel.
const ROD_PROXY: ProxySpec = ProxySpec {
    voxel_m: 0.05,
    fill: ProxyFill::Surface,
    max_error_m: 0.10,
};
const FURNITURE_PROXY: ProxySpec = ProxySpec {
    voxel_m: 0.05,
    fill: ProxyFill::AlongY,
    max_error_m: 0.06,
};

/// Placement of a part inside its kit asset: `T · R_xyz(rotation) · S`, as `LocalTransform`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartTransform {
    pub translation: [f32; 3],
    pub rotation_deg: [f32; 3],
    pub scale: [f32; 3],
}

pub const IDENTITY: PartTransform = PartTransform {
    translation: [0.0; 3],
    rotation_deg: [0.0; 3],
    scale: [1.0; 3],
};

impl PartTransform {
    pub fn matrix(&self) -> Mat4 {
        let r = self.rotation_deg.map(f32::to_radians);
        Mat4::from_scale_rotation_translation(
            Vec3::from_array(self.scale),
            Quat::from_euler(EulerRot::XYZ, r[0], r[1], r[2]),
            Vec3::from_array(self.translation),
        )
    }
}

/// One reviewed source GLB, restricted by `filter`, placed by `transform`.
#[derive(Clone, Copy, Debug)]
pub struct KitPart {
    pub path: &'static str,
    pub filter: PrimitiveFilter,
    pub transform: PartTransform,
    /// `None` for parts that carry no collision proxy (glazing grids and timber casements whose
    /// 0.05-0.1 m members leave the aperture they sit in open; see `docs/PHYSICAL_TRUTH_AUDIT.md`).
    pub proxy: Option<ProxySpec>,
}

#[derive(Clone, Copy, Debug)]
pub struct KitAsset {
    /// Semantic asset id used by `ModuleDef::asset_id`.
    pub id: &'static str,
    /// Primary source mesh, relative to the repository root (the first part). Build the kit mesh
    /// with [`load_mesh`], which applies the part filters and composite transforms.
    pub path: &'static str,
    pub material: KitMaterial,
    pub parts: &'static [KitPart],
    /// Axis-aligned bounds of the built mesh in asset space, verified against the loaded source.
    pub nominal_min: [f32; 3],
    pub nominal_max: [f32; 3],
    /// Through-openings (x0, x1, y0, y1) in the asset's XY plane: doorways, windows, gates.
    /// Verified to be free of source geometry and of collision proxies.
    pub openings: &'static [[f32; 4]],
}

impl KitAsset {
    pub fn nominal_size(&self) -> Vec3 {
        Vec3::from_array(self.nominal_max) - Vec3::from_array(self.nominal_min)
    }
}

const MP1: &str = "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/";

const fn whole(path: &'static str, proxy: Option<ProxySpec>) -> KitPart {
    KitPart {
        path,
        filter: PrimitiveFilter {
            include: &[],
            exclude: &[],
        },
        transform: IDENTITY,
        proxy,
    }
}

const fn filtered(
    path: &'static str,
    include: &'static [&'static str],
    exclude: &'static [&'static str],
    proxy: Option<ProxySpec>,
) -> KitPart {
    KitPart {
        path,
        filter: PrimitiveFilter { include, exclude },
        transform: IDENTITY,
        proxy,
    }
}

const fn placed(
    path: &'static str,
    include: &'static [&'static str],
    translation: [f32; 3],
    rotation_y_deg: f32,
    scale: [f32; 3],
    proxy: Option<ProxySpec>,
) -> KitPart {
    KitPart {
        path,
        filter: PrimitiveFilter {
            include,
            exclude: &[],
        },
        transform: PartTransform {
            translation,
            rotation_deg: [0.0, rotation_y_deg, 0.0],
            scale,
        },
        proxy,
    }
}

// Source paths. Every literal is a full `assets/pizza/...` path so the runtime-asset cook's
// code discovery (`tools/runtime_asset_cook.py`, consumer `code.building-kit`) sees it.
const WALL_HR_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/wall_hr_1.glb";
const WALL_HS_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/wall_hs_1.glb";
const WINDOW_HR_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/window_hr_1.glb";
const WINDOW_HS_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/window_hs_1.glb";
const WINDOWS_HR_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/windows_hr_1.glb";
const DOORWAY_HR_2_REGULAR: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/doorway_hr_2_regular.glb";
const DOORWAY_HR_2_WIDE: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/doorway_hr_2_wide.glb";
const DOORWAY_HR_3_REGULAR: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/doorway_hr_3_regular.glb";
const DOORWAY_HS_1_REGULAR: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/doorway_hs_1_regular.glb";
const GARAGE_DOOR_FRAME_HR_2: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/garage_door_frame_hr_2.glb";
const PILLAR_HR_4: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/pillar_hr_4.glb";
const PILLAR_HR_6: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/pillar_hr_6.glb";
const BEAM_HL_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/beam_hl_1.glb";
const BEAM_HC_HORIZONTAL_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/beam_hc_horizonatal_1.glb";
const WALL_TOP_PART_HR_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/wall_top_part_hr_1.glb";
const FLOOR_CEILING_HR_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/floor_ceiling_hr_1.glb";
const FLOOR_CEILING_HR_2_HOLE: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/floor_ceiling_hr_2_hole.glb";
const ROOF_MH_4_A_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/roof_mh_4_a_1.glb";
const ROOF_MH_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/roof_mh_1.glb";
const WAREHOUSE_HS_1_ROOF_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/warehouse_hs_1_roof_1.glb";
const PLATFORM_B_HANDRAIL_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/platform_b_handrail_1.glb";
const WALL_RX_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/wall_rx_1.glb";
const WALL_RX_1_QUARTER_WIDTH: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/wall_rx_1_1_4_width.glb";
const WALL_RX_1_THIRD_HEIGHT_HALF_WIDTH: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/wall_rx_1_1_3_height_1_2_width.glb";
const GRAVEL_PILE_HR_2: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Debris & Misc/gravel_pile_hr_2.glb";
const WOODEN_CRATE_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Props/wooden_crate_1.glb";
const SHELF_MX_7: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Props/shelf_mx_7.glb";
const STAIRS_5_CONCRETE: &str = "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/Modular Structures/stairs_5_concrete.glb";
const STAIRS_5_WOOD: &str = "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/Modular Structures/stairs_5_wood.glb";
const FLOOR_CEILING_1: &str = "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/Modular Structures/floor_ceiling_1.glb";
const WINDOW_1_SINGLE_FRAME_ONLY: &str = "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/Modular Structures/window_1_single_frame_only.glb";
const WINDOW_1_SINGLE_NO_GLASS: &str = "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/Modular Structures/window_1_single_no_glass.glb";
const WARDROBE_MP_1: &str = "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/Furniture/wardrobe_mp_1.glb";
const BED_1_FRAME: &str = "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/Furniture/bed_1_frame.glb";
const CEILING_LAMP_FPS_1: &str = "assets/pizza/_extracted/Modular Retro FPS Kit v1.5/Modular Retro FPS Kit/Models/other-formats/GLB/ceiling_lamp_fps_1.glb";

/// Window wall without its glazing panel and inner timber sill board (the metal outer sill and the
/// concrete lintel stay): the burned or blown-out window, and the host of a separate casement.
const NO_GLAZING: &[&str] = &["window_hm_2", "planks_hr_4"];
const NO_GLAZING_PAIR: &[&str] = &["window_hr_1", "planks_hr_4"];
/// The corrugated sheet and its battens of `roof_mh_4_a_1`; the skylights, steel ring beam,
/// trims and brick gable fillers are left out.
const ROOF_SHEET: &[&str] = &["roof_hr_5", "planks_hr_3"];

const STAIR_FLIGHT_SCALE: [f32; 3] = [0.6, 0.5, 0.735];

pub const KIT: &[KitAsset] = &[
    // ---- The featured house's nine modules (ids kept) ----
    KitAsset {
        id: "pizza.structure.wall",
        path: WALL_HR_1,
        material: KitMaterial::Brick,
        parts: &[whole(WALL_HR_1, Some(WALL_PROXY))],
        nominal_min: [-2.0, 0.0, -0.2],
        nominal_max: [2.0, 3.0, 0.2],
        openings: &[],
    },
    KitAsset {
        id: "pizza.structure.window_frame",
        path: WINDOW_1_SINGLE_FRAME_ONLY,
        material: KitMaterial::Timber,
        parts: &[whole(WINDOW_1_SINGLE_FRAME_ONLY, Some(WINDOW_WALL_PROXY))],
        nominal_min: [-2.03, 0.0, -0.1],
        nominal_max: [2.03, 3.0, 0.1],
        openings: &[[-0.56, 0.56, 0.9, 2.5]],
    },
    KitAsset {
        id: "pizza.structure.doorway",
        path: DOORWAY_HR_2_WIDE,
        material: KitMaterial::Brick,
        parts: &[whole(DOORWAY_HR_2_WIDE, Some(WALL_PROXY))],
        nominal_min: [-2.0, 0.0, -0.21],
        nominal_max: [2.0, 3.0, 0.21],
        openings: &[[-0.8, 0.8, 0.0, 2.5]],
    },
    KitAsset {
        id: "pizza.structure.floor",
        path: FLOOR_CEILING_HR_1,
        material: KitMaterial::Concrete,
        parts: &[whole(FLOOR_CEILING_HR_1, Some(SLAB_PROXY))],
        nominal_min: [-2.0, 0.0, -2.0],
        nominal_max: [2.0, 0.4, 2.0],
        openings: &[],
    },
    KitAsset {
        id: "pizza.structure.floor_hole",
        path: FLOOR_CEILING_HR_2_HOLE,
        material: KitMaterial::Concrete,
        parts: &[whole(FLOOR_CEILING_HR_2_HOLE, Some(SLOPED_SLAB_PROXY))],
        nominal_min: [-2.0, 0.0, -2.0],
        nominal_max: [2.0, 0.4, 2.0],
        openings: &[],
    },
    KitAsset {
        id: "pizza.structure.roof",
        path: ROOF_MH_4_A_1,
        material: KitMaterial::Metal,
        parts: &[whole(ROOF_MH_4_A_1, Some(SHEET_PROXY))],
        nominal_min: [-4.0, 0.0, -2.0],
        nominal_max: [4.0, 2.0, 2.0],
        openings: &[],
    },
    KitAsset {
        id: "pizza.structure.stairs",
        path: STAIRS_5_CONCRETE,
        material: KitMaterial::Concrete,
        parts: &[whole(STAIRS_5_CONCRETE, Some(FLIGHT_PROXY))],
        nominal_min: [-1.0, -0.2, -2.0],
        nominal_max: [1.0, 3.0, 2.076_583],
        openings: &[],
    },
    KitAsset {
        id: "pizza.props.shelf",
        path: SHELF_MX_7,
        material: KitMaterial::Timber,
        parts: &[whole(SHELF_MX_7, Some(ROD_PROXY))],
        nominal_min: [-0.31, 0.0, -0.21],
        nominal_max: [0.31, 1.81, 0.21],
        openings: &[],
    },
    KitAsset {
        id: "pizza.retro_fps.asset.ceiling_lamp_fps_1",
        path: CEILING_LAMP_FPS_1,
        material: KitMaterial::Metal,
        parts: &[whole(CEILING_LAMP_FPS_1, Some(ROD_PROXY))],
        nominal_min: [-0.4, -0.09, -0.4],
        nominal_max: [0.4, 0.0, 0.4],
        openings: &[],
    },
    // ---- Masonry (MP2 "hr" red brick, "hs" dark brick) ----
    // Plain dark-brick bay: plinths, semi-basements and rusticated ground floors (P G6: 08-13).
    KitAsset {
        id: "urban.brick.wall_dark",
        path: WALL_HS_1,
        material: KitMaterial::Brick,
        parts: &[whole(WALL_HS_1, Some(WALL_PROXY))],
        nominal_min: [-2.0, 0.0, -0.2],
        nominal_max: [2.0, 3.0, 0.2],
        openings: &[],
    },
    // One 2.00 × 1.47 m window, sill 1.03 m, lintel, metal outer sill on +Z; no glazing.
    KitAsset {
        id: "urban.brick.window",
        path: WINDOW_HR_1,
        material: KitMaterial::Brick,
        parts: &[filtered(WINDOW_HR_1, &[], NO_GLAZING, Some(WINDOW_WALL_PROXY))],
        nominal_min: [-2.0, 0.0, -0.21],
        nominal_max: [2.0, 3.0, 0.295],
        openings: &[[-1.0, 1.0, 1.031, 2.5]],
    },
    KitAsset {
        id: "urban.brick.window_dark",
        path: WINDOW_HS_1,
        material: KitMaterial::Brick,
        parts: &[filtered(WINDOW_HS_1, &[], NO_GLAZING, Some(WINDOW_WALL_PROXY))],
        nominal_min: [-2.0, 0.0, -0.21],
        nominal_max: [2.0, 3.0, 0.295],
        openings: &[[-1.0, 1.0, 1.031, 2.5]],
    },
    // Two 0.80 × 1.50 m windows grouped between wide piers (P G2: 07-15 tall grouped variant).
    KitAsset {
        id: "urban.brick.window_pair",
        path: WINDOWS_HR_1,
        material: KitMaterial::Brick,
        parts: &[filtered(WINDOWS_HR_1, &[], NO_GLAZING_PAIR, Some(WINDOW_WALL_PROXY))],
        nominal_min: [-2.0, 0.0, -0.21],
        nominal_max: [2.0, 3.0, 0.295],
        openings: &[[-1.0, -0.2, 1.012, 2.5], [0.2, 1.0, 1.012, 2.5]],
    },
    // 1.00 × 2.10 m doorway (interior doors, house entrances).
    KitAsset {
        id: "urban.brick.door",
        path: DOORWAY_HR_2_REGULAR,
        material: KitMaterial::Brick,
        parts: &[whole(DOORWAY_HR_2_REGULAR, Some(WALL_PROXY))],
        nominal_min: [-2.0, 0.0, -0.21],
        nominal_max: [2.0, 3.0, 0.21],
        openings: &[[-0.5, 0.5, 0.0, 2.1]],
    },
    KitAsset {
        id: "urban.brick.door_dark",
        path: DOORWAY_HS_1_REGULAR,
        material: KitMaterial::Brick,
        parts: &[whole(DOORWAY_HS_1_REGULAR, Some(WALL_PROXY))],
        nominal_min: [-2.0, 0.0, -0.21],
        nominal_max: [2.0, 3.0, 0.21],
        openings: &[[-0.5, 0.5, 0.0, 2.1]],
    },
    // Doorway closed by nailed planks: a barricaded entrance (R1 §7.2 [D: S3 p. 157]).
    KitAsset {
        id: "urban.brick.door_boarded",
        path: DOORWAY_HR_3_REGULAR,
        material: KitMaterial::Brick,
        parts: &[whole(DOORWAY_HR_3_REGULAR, Some(SLOPED_WALL_PROXY))],
        nominal_min: [-2.0, 0.0, -0.21],
        nominal_max: [2.0, 3.0, 0.26],
        openings: &[],
    },
    // 3.0 × 2.5 m gate opening without its leaves: workshop gates and yard gateways.
    KitAsset {
        id: "urban.brick.gate",
        path: GARAGE_DOOR_FRAME_HR_2,
        material: KitMaterial::Brick,
        parts: &[filtered(GARAGE_DOOR_FRAME_HR_2, &[], &["garage_door_hm_8"], Some(WALL_PROXY))],
        nominal_min: [-2.0, 0.0, -0.2],
        nominal_max: [2.0, 3.0, 0.2],
        openings: &[[-1.5, 1.5, 0.0, 2.5]],
    },
    // 1.0 × 3.0 m brick pier with coping: workshop piers, stove bodies.
    KitAsset {
        id: "urban.brick.pier",
        path: PILLAR_HR_4,
        material: KitMaterial::Brick,
        parts: &[whole(PILLAR_HR_4, Some(PIER_PROXY))],
        nominal_min: [-0.5, 0.0, -0.5],
        nominal_max: [0.5, 3.0, 0.5],
        openings: &[],
    },
    // 1.0 × 6.0 m brick shaft: chimney stacks (P G10, G13: 04-11, 07-14).
    KitAsset {
        id: "urban.brick.stack",
        path: PILLAR_HR_6,
        material: KitMaterial::Brick,
        parts: &[whole(PILLAR_HR_6, Some(STACK_PROXY))],
        nominal_min: [-0.5, 0.0, -0.5],
        nominal_max: [0.5, 6.0, 0.5],
        openings: &[],
    },
    // 4.0 × 0.26 × 0.30 m brick band hanging below its origin: cornices, string courses,
    // wooden-house plinths (P G5: 04-05, 07-14).
    KitAsset {
        id: "urban.brick.band",
        path: BEAM_HL_1,
        material: KitMaterial::Brick,
        parts: &[whole(BEAM_HL_1, Some(PIER_PROXY))],
        nominal_min: [-2.0, -0.258, -0.15],
        nominal_max: [2.0, 0.0, 0.15],
        openings: &[],
    },
    // Three unscaled bands: a 0.77 m brick parapet (P G4: 07-08, 07-16; 0.8-1.0 m).
    KitAsset {
        id: "urban.brick.parapet",
        path: BEAM_HL_1,
        material: KitMaterial::Brick,
        parts: &[
            placed(BEAM_HL_1, &[], [0.0, 0.258, 0.0], 0.0, [1.0; 3], Some(PIER_PROXY)),
            placed(BEAM_HL_1, &[], [0.0, 0.516, 0.0], 0.0, [1.0; 3], Some(PIER_PROXY)),
            placed(BEAM_HL_1, &[], [0.0, 0.774, 0.0], 0.0, [1.0; 3], Some(PIER_PROXY)),
        ],
        nominal_min: [-2.0, 0.0, -0.15],
        nominal_max: [2.0, 0.774, 0.15],
        openings: &[],
    },
    // Right-triangle gable half: 4 m run rising 1 m toward +X, 0.4 m thick; scaled to any pitch.
    KitAsset {
        id: "urban.brick.gable",
        path: WALL_TOP_PART_HR_1,
        material: KitMaterial::Brick,
        parts: &[whole(WALL_TOP_PART_HR_1, Some(SLOPED_WALL_PROXY))],
        nominal_min: [-2.0, 0.0, -0.2],
        nominal_max: [2.0, 1.0, 0.2],
        openings: &[],
    },
    // ---- Metal ----
    KitAsset {
        id: "urban.metal.window_grid",
        path: WINDOW_HR_1,
        material: KitMaterial::Metal,
        parts: &[filtered(WINDOW_HR_1, &["window_hm_2"], &[], None)],
        nominal_min: [-1.0, 1.024, -0.1],
        nominal_max: [1.0, 2.5, 0.1],
        openings: &[],
    },
    // Gabled sheet-iron roof section, span 7.84 m on X, ridge along Z, 1.95 m rise (26.4°).
    KitAsset {
        id: "urban.metal.roof",
        path: ROOF_MH_4_A_1,
        material: KitMaterial::Metal,
        parts: &[filtered(ROOF_MH_4_A_1, ROOF_SHEET, &[], Some(SHEET_PROXY))],
        nominal_min: [-3.922, 0.0, -1.95],
        nominal_max: [3.922, 1.95, 1.95],
        openings: &[],
    },
    // Mono-pitch sheet roof (sheds, lean-tos), high side on -X.
    KitAsset {
        id: "urban.metal.roof_lean",
        path: ROOF_MH_1,
        material: KitMaterial::Metal,
        parts: &[filtered(ROOF_MH_1, &["roof_hr_5", "planks_hr_3"], &[], Some(SHEET_PROXY))],
        nominal_min: [-1.95, 0.0, -1.95],
        nominal_max: [1.922, 1.331, 1.95],
        openings: &[],
    },
    // Segmental sheet roof on ribs, span 8 m (P G4: 04-19 workshop).
    KitAsset {
        id: "urban.metal.roof_vault",
        path: WAREHOUSE_HS_1_ROOF_1,
        material: KitMaterial::Metal,
        parts: &[whole(WAREHOUSE_HS_1_ROOF_1, Some(SHEET_PROXY))],
        nominal_min: [-4.0, 0.0, -4.0],
        nominal_max: [4.0, 1.657, 4.0],
        openings: &[],
    },
    // 8 m rolled steel I-beam: workshop lintels on piers (P G3, L12: 04-19).
    KitAsset {
        id: "urban.metal.beam",
        path: BEAM_HC_HORIZONTAL_1,
        material: KitMaterial::Metal,
        parts: &[whole(BEAM_HC_HORIZONTAL_1, Some(PIER_PROXY))],
        nominal_min: [-4.0, 0.0, -0.15],
        nominal_max: [4.0, 0.4, 0.15],
        openings: &[],
    },
    KitAsset {
        id: "urban.metal.bedstead",
        path: BED_1_FRAME,
        material: KitMaterial::Metal,
        parts: &[whole(BED_1_FRAME, Some(ROD_PROXY))],
        nominal_min: [-1.35, 0.0, -0.7],
        nominal_max: [1.35, 1.27, 0.7],
        openings: &[],
    },
    // ---- Concrete ----
    // Dog-leg concrete stair for one storey: flight A (+X half) rises from the floor landing edge
    // at z = -2.1 to a mid landing at +Z, flight B (-X half) returns to z = -2.1 one storey up.
    // Nominal 2.6 m wide, 3.0 m rise, 4.2 m deep; the grammar scales it to the stair hall.
    KitAsset {
        id: "urban.concrete.stair_dogleg",
        path: STAIRS_5_CONCRETE,
        material: KitMaterial::Concrete,
        parts: &[
            placed(STAIRS_5_CONCRETE, &[], [0.65, 0.0, -0.573], 180.0, STAIR_FLIGHT_SCALE, Some(FLIGHT_PROXY)),
            placed(FLOOR_CEILING_HR_1, &[], [0.0, 1.3, 1.5], 0.0, [0.65, 0.5, 0.3], Some(SLAB_PROXY)),
            placed(STAIRS_5_CONCRETE, &[], [-0.65, 1.5, -0.63], 0.0, STAIR_FLIGHT_SCALE, Some(FLIGHT_PROXY)),
        ],
        nominal_min: [-1.3, -0.1, -2.1],
        nominal_max: [1.3, 3.0, 2.1],
        openings: &[],
    },
    // Balcony: 3.0 × 1.1 m slab 0.2 m thick projecting on +Z from its wall face at z = 0, with
    // steel railings on three sides (P G1, G11: 04-11, 08-13).
    KitAsset {
        id: "urban.concrete.balcony",
        path: FLOOR_CEILING_HR_1,
        material: KitMaterial::Concrete,
        parts: &[
            placed(FLOOR_CEILING_HR_1, &[], [0.0, 0.0, 0.55], 0.0, [0.75, 0.5, 0.275], Some(SLAB_PROXY)),
            placed(PLATFORM_B_HANDRAIL_1, &[], [0.0, 0.2, 1.06], 90.0, [1.0, 1.0, 0.72], Some(ROD_PROXY)),
            placed(PLATFORM_B_HANDRAIL_1, &[], [-1.46, 0.2, 0.55], 0.0, [1.0, 1.0, 0.26], Some(ROD_PROXY)),
            placed(PLATFORM_B_HANDRAIL_1, &[], [1.40, 0.2, 0.55], 0.0, [1.0, 1.0, 0.26], Some(ROD_PROXY)),
        ],
        nominal_min: [-1.5, 0.0, 0.0],
        nominal_max: [1.5, 1.4, 1.1],
        openings: &[],
    },
    // ---- Timber (MP2 "rx" boarded pieces; MP1 wood) ----
    KitAsset {
        id: "urban.timber.wall",
        path: WALL_RX_1,
        material: KitMaterial::Timber,
        parts: &[whole(WALL_RX_1, Some(WALL_PROXY))],
        nominal_min: [-2.0, 0.0, -0.2],
        nominal_max: [2.0, 3.0, 0.2],
        openings: &[],
    },
    // Boarded window bay: two 1.25 m posts, a 1.0 m sill panel and a 0.6 m lintel panel leave a
    // 1.5 × 1.4 m window at sill 1.0 m (scaled to a 2.4-2.7 m bay: 0.9-1.0 × 1.4 m, R1 §3.3).
    KitAsset {
        id: "urban.timber.window",
        path: WALL_RX_1_QUARTER_WIDTH,
        material: KitMaterial::Timber,
        parts: &[
            placed(WALL_RX_1_QUARTER_WIDTH, &[], [-1.375, 0.0, 0.0], 0.0, [1.25, 1.0, 1.0], Some(WALL_PROXY)),
            placed(WALL_RX_1_QUARTER_WIDTH, &[], [1.375, 0.0, 0.0], 0.0, [1.25, 1.0, 1.0], Some(WALL_PROXY)),
            placed(WALL_RX_1_THIRD_HEIGHT_HALF_WIDTH, &[], [0.75, 0.0, 0.0], 0.0, [0.75, 1.0, 1.0], Some(WALL_PROXY)),
            placed(WALL_RX_1_THIRD_HEIGHT_HALF_WIDTH, &[], [0.75, 2.4, 0.0], 0.0, [0.75, 0.6, 1.0], Some(WALL_PROXY)),
        ],
        nominal_min: [-2.0, 0.0, -0.2],
        nominal_max: [2.0, 3.0, 0.2],
        openings: &[[-0.75, 0.75, 1.0, 2.4]],
    },
    // Boarded door bay: 1.3 × 2.0 m opening (0.85 m in a 2.6 m bay, R1 §3.3 0.8-0.9 × 1.9-2.0).
    KitAsset {
        id: "urban.timber.door",
        path: WALL_RX_1_QUARTER_WIDTH,
        material: KitMaterial::Timber,
        parts: &[
            placed(WALL_RX_1_QUARTER_WIDTH, &[], [-1.325, 0.0, 0.0], 0.0, [1.35, 1.0, 1.0], Some(WALL_PROXY)),
            placed(WALL_RX_1_QUARTER_WIDTH, &[], [1.325, 0.0, 0.0], 0.0, [1.35, 1.0, 1.0], Some(WALL_PROXY)),
            placed(WALL_RX_1_THIRD_HEIGHT_HALF_WIDTH, &[], [0.65, 2.0, 0.0], 0.0, [0.65, 1.0, 1.0], Some(WALL_PROXY)),
        ],
        nominal_min: [-2.0, 0.0, -0.2],
        nominal_max: [2.0, 3.0, 0.2],
        openings: &[[-0.65, 0.65, 0.0, 2.0]],
    },
    // Board floor on joists: the boards and the ceiling below them, 2 cm apart. The joist depth
    // between them is not modelled (see the physical-truth ledger).
    KitAsset {
        id: "urban.timber.floor",
        path: FLOOR_CEILING_1,
        material: KitMaterial::Timber,
        parts: &[whole(FLOOR_CEILING_1, Some(SLAB_PROXY))],
        nominal_min: [-2.02, -0.01, -2.02],
        nominal_max: [2.02, 0.01, 2.02],
        openings: &[],
    },
    // Dog-leg timber stair, same layout as `urban.concrete.stair_dogleg`, board landing.
    KitAsset {
        id: "urban.timber.stair_dogleg",
        path: STAIRS_5_WOOD,
        material: KitMaterial::Timber,
        parts: &[
            placed(STAIRS_5_WOOD, &[], [0.65, 0.0, -0.596], 180.0, STAIR_FLIGHT_SCALE, Some(FLIGHT_PROXY)),
            placed(FLOOR_CEILING_1, &[], [0.0, 1.49, 1.5], 0.0, [0.644, 1.0, 0.297], Some(SLAB_PROXY)),
            placed(STAIRS_5_WOOD, &[], [-0.65, 1.5, -0.63], 0.0, STAIR_FLIGHT_SCALE, Some(FLIGHT_PROXY)),
        ],
        nominal_min: [-1.3, -0.1, -2.1],
        nominal_max: [1.3, 3.0, 2.1],
        openings: &[],
    },
    // Straight timber flight: steep root-cellar stairs of wooden houses.
    KitAsset {
        id: "urban.timber.stair_flight",
        path: STAIRS_5_WOOD,
        material: KitMaterial::Timber,
        parts: &[whole(STAIRS_5_WOOD, Some(FLIGHT_PROXY))],
        nominal_min: [-1.0, -0.2, -2.0],
        nominal_max: [1.0, 3.0, 2.045],
        openings: &[],
    },
    // Timber casement with its glazing bars and no glass (P G11: frames kept in shelled blocks).
    KitAsset {
        id: "urban.timber.window_frame",
        path: WINDOW_1_SINGLE_NO_GLASS,
        material: KitMaterial::Timber,
        parts: &[filtered(WINDOW_1_SINGLE_NO_GLASS, &["door_frame_wood_1"], &[], None)],
        nominal_min: [-0.64, 0.82, -0.1],
        nominal_max: [0.64, 2.58, 0.1],
        openings: &[],
    },
    // Double-leaf timber gate for `urban.brick.gate`.
    KitAsset {
        id: "urban.timber.gate_leaf",
        path: GARAGE_DOOR_FRAME_HR_2,
        material: KitMaterial::Timber,
        parts: &[filtered(GARAGE_DOOR_FRAME_HR_2, &["garage_door_hm_8"], &[], Some(WALL_PROXY))],
        nominal_min: [-1.5, 0.0, -0.02],
        nominal_max: [1.5, 2.5, 0.02],
        openings: &[],
    },
    KitAsset {
        id: "urban.timber.wardrobe",
        path: WARDROBE_MP_1,
        material: KitMaterial::Timber,
        parts: &[whole(WARDROBE_MP_1, Some(FURNITURE_PROXY))],
        nominal_min: [-0.67, 0.0, -0.32],
        nominal_max: [0.67, 2.1, 0.32],
        openings: &[],
    },
    KitAsset {
        id: "urban.timber.crate",
        path: WOODEN_CRATE_1,
        material: KitMaterial::Timber,
        parts: &[whole(WOODEN_CRATE_1, Some(ROD_PROXY))],
        nominal_min: [-0.75, 0.0, -0.57],
        nominal_max: [0.72, 0.93, 0.57],
        openings: &[],
    },
    // ---- Rubble ----
    // Low mound of broken brick and ash: the burned floors, roofs and partitions that fell into a
    // gutted interior (R1 §6.3: 0.3-1.0 m; P G17). Scaled to the room it covers.
    KitAsset {
        id: "urban.rubble.layer",
        path: GRAVEL_PILE_HR_2,
        material: KitMaterial::Brick,
        parts: &[whole(GRAVEL_PILE_HR_2, Some(SLOPED_SLAB_PROXY))],
        nominal_min: [-0.756, 0.0, -0.737],
        nominal_max: [0.73, 0.279, 0.92],
        openings: &[],
    },
];

pub fn asset(id: &str) -> Option<&'static KitAsset> {
    KIT.iter().find(|asset| asset.id == id)
}

pub fn asset_path(id: &str) -> Option<&'static str> {
    asset(id).map(|asset| asset.path)
}

/// Every distinct source GLB the kit reads.
pub fn source_paths() -> Vec<&'static str> {
    let mut paths: Vec<_> = KIT
        .iter()
        .flat_map(|asset| asset.parts.iter().map(|part| part.path))
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths
}

fn load_part(part: &KitPart) -> anyhow::Result<CpuMesh> {
    let mesh = if part.filter.include.is_empty() && part.filter.exclude.is_empty() {
        crate::gltf_load::load_glb(part.path)?
    } else {
        crate::gltf_load::load_glb_filtered(part.path, part.filter)?
    };
    Ok(if part.transform == IDENTITY {
        mesh
    } else {
        mesh.transformed(part.transform.matrix())
    })
}

/// Build a kit asset's mesh in asset space: the only correct way to load a kit id (filters and
/// composites are applied here). A composite keeps its first part's textures (see module docs).
pub fn load_mesh(id: &str) -> anyhow::Result<CpuMesh> {
    let asset = asset(id).ok_or_else(|| anyhow::anyhow!("unknown building kit asset {id:?}"))?;
    let mut merged: Option<CpuMesh> = None;
    for part in asset.parts {
        let mesh = load_part(part)
            .map_err(|error| anyhow::anyhow!("kit asset {id}: part {}: {error}", part.path))?;
        merged = Some(match merged {
            None => mesh,
            Some(mut base) => {
                let offset = base.vertices.len() as u32;
                base.vertices.extend_from_slice(&mesh.vertices);
                base.indices.extend(mesh.indices.iter().map(|index| index + offset));
                base.min = base.min.min(mesh.min);
                base.max = base.max.max(mesh.max);
                base
            }
        });
    }
    merged.ok_or_else(|| anyhow::anyhow!("kit asset {id} has no parts"))
}

/// Collision proxies of one kit asset in asset space.
#[derive(Clone, Debug)]
pub struct KitProxies {
    /// Axis-aligned boxes (min, max).
    pub boxes: Vec<(Vec3, Vec3)>,
    /// Measured error (see module docs), already multiplied by each composite part's largest
    /// scale factor; the kit test asserts it against the parts' `max_error_m`.
    pub error_m: f32,
    /// Largest `max_error_m × part scale` of the asset's parts.
    pub bound_m: f32,
    /// Volume of the proxies' union (boxes of one part never overlap; parts of a composite may
    /// touch), m³: the solid volume a failed module turns into rubble (P G17).
    pub volume_m3: f32,
}

fn proxy_cache() -> &'static Mutex<HashMap<&'static str, Arc<KitProxies>>> {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, Arc<KitProxies>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Collision proxies generated from the asset's source triangles (cached per asset id).
pub fn proxies(id: &str) -> anyhow::Result<Arc<KitProxies>> {
    let asset = asset(id).ok_or_else(|| anyhow::anyhow!("unknown building kit asset {id:?}"))?;
    if let Some(found) = proxy_cache().lock().expect("kit proxy cache").get(asset.id) {
        return Ok(found.clone());
    }
    let mut boxes = Vec::new();
    let mut error_m = 0.0_f32;
    let mut bound_m = 0.0_f32;
    let mut volume_m3 = 0.0_f32;
    for part in asset.parts {
        let Some(spec) = part.proxy else {
            continue;
        };
        let mesh = if part.filter.include.is_empty() && part.filter.exclude.is_empty() {
            crate::gltf_load::load_glb(part.path)?
        } else {
            crate::gltf_load::load_glb_filtered(part.path, part.filter)?
        };
        let generated = proxies_from_triangles(&mesh_triangles(&mesh), spec);
        let matrix = part.transform.matrix();
        let scale = Vec3::from_array(part.transform.scale).abs().max_element();
        for (min, max) in generated.boxes {
            let (a, b) = transform_aabb(matrix, min, max);
            volume_m3 += (b - a).x * (b - a).y * (b - a).z;
            boxes.push((a, b));
        }
        error_m = error_m.max(generated.error_m * scale);
        bound_m = bound_m.max(spec.max_error_m * scale);
    }
    let result = Arc::new(KitProxies {
        boxes,
        error_m,
        bound_m,
        volume_m3,
    });
    proxy_cache()
        .lock()
        .expect("kit proxy cache")
        .insert(asset.id, result.clone());
    Ok(result)
}

/// Transform an AABB by an affine matrix (exact for quarter-turn rotations).
pub fn transform_aabb(matrix: Mat4, min: Vec3, max: Vec3) -> (Vec3, Vec3) {
    let mut out_min = Vec3::splat(f32::INFINITY);
    let mut out_max = Vec3::splat(f32::NEG_INFINITY);
    for corner in 0..8 {
        let p = Vec3::new(
            if corner & 1 == 0 { min.x } else { max.x },
            if corner & 2 == 0 { min.y } else { max.y },
            if corner & 4 == 0 { min.z } else { max.z },
        );
        let q = matrix.transform_point3(p);
        out_min = out_min.min(q);
        out_max = out_max.max(q);
    }
    (out_min, out_max)
}

pub fn mesh_triangles(mesh: &CpuMesh) -> Vec<[Vec3; 3]> {
    mesh.indices
        .chunks_exact(3)
        .filter_map(|tri| {
            let get = |i: u32| mesh.vertices.get(i as usize).map(|v| Vec3::from_array(v.pos));
            Some([get(tri[0])?, get(tri[1])?, get(tri[2])?])
        })
        .collect()
}

/// Result of [`proxies_from_triangles`] in the triangles' space.
#[derive(Clone, Debug)]
pub struct GeneratedProxies {
    pub boxes: Vec<(Vec3, Vec3)>,
    pub error_m: f32,
}

struct VoxelGrid {
    origin: Vec3,
    size: f32,
    n: [usize; 3],
    cells: Vec<bool>,
}

impl VoxelGrid {
    fn new(min: Vec3, max: Vec3, size: f32) -> Self {
        // At least one empty voxel of padding on every side, so the flood fill can walk round the
        // solid. The half-voxel offset puts faces at the source's round coordinates in the middle
        // of a voxel, so exactly one layer marks them instead of the two a shared boundary would.
        let origin = min - Vec3::splat(size * 1.5);
        let extent = (max + Vec3::splat(size * 1.5)) - origin;
        let n = [
            ((extent.x / size).ceil() as usize).max(1),
            ((extent.y / size).ceil() as usize).max(1),
            ((extent.z / size).ceil() as usize).max(1),
        ];
        Self {
            origin,
            size,
            n,
            cells: vec![false; n[0] * n[1] * n[2]],
        }
    }

    fn index(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.n[1] + y) * self.n[0] + x
    }

    fn get(&self, x: usize, y: usize, z: usize) -> bool {
        self.cells[self.index(x, y, z)]
    }

    fn set(&mut self, x: usize, y: usize, z: usize) {
        let i = self.index(x, y, z);
        self.cells[i] = true;
    }

    fn cell_min(&self, x: usize, y: usize, z: usize) -> Vec3 {
        self.origin + Vec3::new(x as f32, y as f32, z as f32) * self.size
    }

    fn cell_centre(&self, x: usize, y: usize, z: usize) -> Vec3 {
        self.cell_min(x, y, z) + Vec3::splat(self.size * 0.5)
    }

    fn mark_surface(&mut self, triangles: &[[Vec3; 3]]) {
        let half = Vec3::splat(self.size * 0.5);
        for tri in triangles {
            let lo = tri[0].min(tri[1]).min(tri[2]);
            let hi = tri[0].max(tri[1]).max(tri[2]);
            let clamp = |v: f32, axis: usize| -> usize {
                let cell = ((v - self.origin[axis]) / self.size).floor();
                cell.clamp(0.0, (self.n[axis] - 1) as f32) as usize
            };
            let (x0, x1) = (clamp(lo.x, 0), clamp(hi.x, 0));
            let (y0, y1) = (clamp(lo.y, 1), clamp(hi.y, 1));
            let (z0, z1) = (clamp(lo.z, 2), clamp(hi.z, 2));
            for z in z0..=z1 {
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        if self.get(x, y, z) {
                            continue;
                        }
                        let centre = self.cell_centre(x, y, z);
                        if triangle_box_overlap(centre, half, tri) {
                            self.set(x, y, z);
                        }
                    }
                }
            }
        }
    }

    fn fill(&mut self, fill: ProxyFill) {
        match fill {
            ProxyFill::Surface => {}
            ProxyFill::Enclosed => {
                let mut outside = vec![false; self.cells.len()];
                let mut stack = vec![(0usize, 0usize, 0usize)];
                outside[0] = true;
                while let Some((x, y, z)) = stack.pop() {
                    let neighbours = [
                        (x.wrapping_sub(1), y, z),
                        (x + 1, y, z),
                        (x, y.wrapping_sub(1), z),
                        (x, y + 1, z),
                        (x, y, z.wrapping_sub(1)),
                        (x, y, z + 1),
                    ];
                    for (nx, ny, nz) in neighbours {
                        if nx >= self.n[0] || ny >= self.n[1] || nz >= self.n[2] {
                            continue;
                        }
                        let i = self.index(nx, ny, nz);
                        if !outside[i] && !self.cells[i] {
                            outside[i] = true;
                            stack.push((nx, ny, nz));
                        }
                    }
                }
                for (cell, out) in self.cells.iter_mut().zip(outside) {
                    if !out {
                        *cell = true;
                    }
                }
            }
            ProxyFill::AlongX | ProxyFill::AlongY | ProxyFill::AlongZ => {
                let axis = match fill {
                    ProxyFill::AlongX => 0,
                    ProxyFill::AlongY => 1,
                    _ => 2,
                };
                let (a, b) = match axis {
                    0 => (1, 2),
                    1 => (0, 2),
                    _ => (0, 1),
                };
                for j in 0..self.n[b] {
                    for i in 0..self.n[a] {
                        let at = |k: usize| {
                            let mut c = [0usize; 3];
                            c[axis] = k;
                            c[a] = i;
                            c[b] = j;
                            c
                        };
                        let mut first = None;
                        let mut last = None;
                        for k in 0..self.n[axis] {
                            let c = at(k);
                            if self.get(c[0], c[1], c[2]) {
                                first.get_or_insert(k);
                                last = Some(k);
                            }
                        }
                        if let (Some(first), Some(last)) = (first, last) {
                            for k in first..=last {
                                let c = at(k);
                                self.set(c[0], c[1], c[2]);
                            }
                        }
                    }
                }
            }
        }
    }

}

/// Greedy merge of solid voxels into disjoint boxes (x runs, then rows, then slabs).
fn greedy_boxes(grid: &VoxelGrid) -> Vec<(Vec3, Vec3)> {
    let [nx, ny, nz] = grid.n;
    let mut used = vec![false; grid.cells.len()];
    let free = |used: &Vec<bool>, x: usize, y: usize, z: usize| {
        let i = grid.index(x, y, z);
        grid.cells[i] && !used[i]
    };
    let mut boxes = Vec::new();
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                if !free(&used, x, y, z) {
                    continue;
                }
                let mut x1 = x;
                while x1 + 1 < nx && free(&used, x1 + 1, y, z) {
                    x1 += 1;
                }
                let mut y1 = y;
                'rows: while y1 + 1 < ny {
                    for xi in x..=x1 {
                        if !free(&used, xi, y1 + 1, z) {
                            break 'rows;
                        }
                    }
                    y1 += 1;
                }
                let mut z1 = z;
                'slabs: while z1 + 1 < nz {
                    for yi in y..=y1 {
                        for xi in x..=x1 {
                            if !free(&used, xi, yi, z1 + 1) {
                                break 'slabs;
                            }
                        }
                    }
                    z1 += 1;
                }
                for zi in z..=z1 {
                    for yi in y..=y1 {
                        for xi in x..=x1 {
                            let i = grid.index(xi, yi, zi);
                            used[i] = true;
                        }
                    }
                }
                boxes.push((
                    grid.cell_min(x, y, z),
                    grid.cell_min(x1 + 1, y1 + 1, z1 + 1),
                ));
            }
        }
    }
    boxes
}

/// Move each box face onto the nearest plane of the source's own vertex coordinates when that
/// plane lies within one voxel: voxel faces of an axis-aligned wall, slab or opening edge become
/// the source's exact faces. The measured error covers any face this moves the wrong way.
fn snap_boxes(boxes: &mut [(Vec3, Vec3)], triangles: &[[Vec3; 3]], voxel: f32) {
    let mut planes: [Vec<f32>; 3] = Default::default();
    for tri in triangles {
        for p in tri {
            for axis in 0..3 {
                planes[axis].push(p[axis]);
            }
        }
    }
    for list in &mut planes {
        list.sort_by(f32::total_cmp);
        list.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    }
    let nearest = |list: &[f32], v: f32| -> Option<f32> {
        let i = list.partition_point(|&p| p < v);
        [i.checked_sub(1), Some(i)]
            .into_iter()
            .flatten()
            .filter_map(|j| list.get(j).copied())
            .filter(|p| (p - v).abs() <= voxel * 1.001)
            .min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()))
    };
    for (min, max) in boxes.iter_mut() {
        for axis in 0..3 {
            if let Some(v) = nearest(&planes[axis], min[axis]) {
                min[axis] = v;
            }
            if let Some(v) = nearest(&planes[axis], max[axis]) {
                max[axis] = v;
            }
            if max[axis] - min[axis] < 1e-4 {
                // Snapping collapsed a one-voxel box onto a single plane (a sheet): keep it a
                // sheet of real zero thickness plus a millimetre so it still intersects rays.
                max[axis] = min[axis] + 1e-3;
            }
        }
    }
}

fn point_box_distance(p: Vec3, min: Vec3, max: Vec3) -> f32 {
    let d = (min - p).max(p - max).max(Vec3::ZERO);
    d.length()
}

/// Generate proxies from triangles with `spec`, and measure their error against a reference
/// voxelisation at half the voxel size (see module docs).
pub fn proxies_from_triangles(triangles: &[[Vec3; 3]], spec: ProxySpec) -> GeneratedProxies {
    if triangles.is_empty() {
        return GeneratedProxies {
            boxes: Vec::new(),
            error_m: 0.0,
        };
    }
    let lo = triangles
        .iter()
        .flat_map(|t| t.iter())
        .fold(Vec3::splat(f32::INFINITY), |a, &b| a.min(b));
    let hi = triangles
        .iter()
        .flat_map(|t| t.iter())
        .fold(Vec3::splat(f32::NEG_INFINITY), |a, &b| a.max(b));
    let mut grid = VoxelGrid::new(lo, hi, spec.voxel_m);
    grid.mark_surface(triangles);
    grid.fill(spec.fill);
    let mut boxes = greedy_boxes(&grid);
    snap_boxes(&mut boxes, triangles, spec.voxel_m);

    // Error against a finer reference solid built with the same fill rule.
    let fine = spec.voxel_m * 0.5;
    let mut reference = VoxelGrid::new(lo, hi, fine);
    reference.mark_surface(triangles);
    reference.fill(spec.fill);
    let inside_any = |p: Vec3| {
        boxes
            .iter()
            .any(|(min, max)| p.cmpge(*min).all() && p.cmple(*max).all())
    };
    let mut over = 0.0_f32;
    let mut under = 0.0_f32;
    for z in 0..reference.n[2] {
        for y in 0..reference.n[1] {
            for x in 0..reference.n[0] {
                let p = reference.cell_centre(x, y, z);
                let solid = reference.get(x, y, z);
                let covered = inside_any(p);
                if covered && !solid {
                    let d = triangles
                        .iter()
                        .map(|t| point_triangle_distance(p, t))
                        .fold(f32::INFINITY, f32::min);
                    over = over.max(d);
                } else if solid && !covered {
                    let d = boxes
                        .iter()
                        .map(|(min, max)| point_box_distance(p, *min, *max))
                        .fold(f32::INFINITY, f32::min);
                    under = under.max(d);
                }
            }
        }
    }
    GeneratedProxies {
        boxes,
        error_m: over.max(under) + fine * 3.0_f32.sqrt() * 0.5,
    }
}

/// Akenine-Möller triangle/box overlap by the separating axis theorem; touching counts.
pub fn triangle_box_overlap(centre: Vec3, half: Vec3, tri: &[Vec3; 3]) -> bool {
    let v0 = tri[0] - centre;
    let v1 = tri[1] - centre;
    let v2 = tri[2] - centre;
    let e0 = v1 - v0;
    let e1 = v2 - v1;
    let e2 = v0 - v2;
    let axes = [Vec3::X, Vec3::Y, Vec3::Z];
    for edge in [e0, e1, e2] {
        for axis in axes {
            let a = axis.cross(edge);
            if a.length_squared() < 1e-12 {
                continue;
            }
            let p0 = v0.dot(a);
            let p1 = v1.dot(a);
            let p2 = v2.dot(a);
            let r = half.x * a.x.abs() + half.y * a.y.abs() + half.z * a.z.abs();
            if p0.min(p1).min(p2) > r || p0.max(p1).max(p2) < -r {
                return false;
            }
        }
    }
    for axis in 0..3 {
        let lo = v0[axis].min(v1[axis]).min(v2[axis]);
        let hi = v0[axis].max(v1[axis]).max(v2[axis]);
        if lo > half[axis] || hi < -half[axis] {
            return false;
        }
    }
    let normal = e0.cross(e1);
    if normal.length_squared() < 1e-14 {
        // Degenerate triangle: the axis tests above already bound it.
        return true;
    }
    let d = normal.dot(v0);
    let r = half.x * normal.x.abs() + half.y * normal.y.abs() + half.z * normal.z.abs();
    d.abs() <= r
}

/// Exact distance from a point to a triangle (Ericson, Real-Time Collision Detection 5.1.5).
pub fn point_triangle_distance(p: Vec3, tri: &[Vec3; 3]) -> f32 {
    let (a, b, c) = (tri[0], tri[1], tri[2]);
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return (p - a).length();
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return (p - b).length();
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (p - (a + ab * v)).length();
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return (p - c).length();
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (p - (a + ac * w)).length();
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (p - (b + (c - b) * w)).length();
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    (p - (a + ab * v + ac * w)).length()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building_blueprint::SemanticId;

    fn assets_installed() -> bool {
        source_paths()
            .iter()
            .all(|path| crate::resource::is_file(path))
    }

    #[test]
    fn kit_ids_are_unique_semantic_ids_and_every_source_mesh_is_on_disk() {
        let mut seen = std::collections::BTreeSet::new();
        for asset in KIT {
            SemanticId::new(asset.id).expect("kit id is a semantic id");
            assert!(seen.insert(asset.id), "duplicate kit id {}", asset.id);
            assert!(!asset.parts.is_empty(), "{} has no parts", asset.id);
            assert_eq!(asset.path, asset.parts[0].path, "{} primary path", asset.id);
            for part in asset.parts {
                assert!(
                    std::path::Path::new(part.path).is_file(),
                    "kit mesh missing: {}",
                    part.path
                );
                let r = part.transform.rotation_deg;
                assert!(
                    r[0] == 0.0 && r[2] == 0.0 && (r[1] / 90.0).fract() == 0.0,
                    "{}: part rotations must be quarter turns about Y to keep proxies exact",
                    asset.id
                );
            }
        }
        assert_eq!(asset_path("pizza.structure.wall"), Some(KIT[0].path));
        assert!(asset("pizza.structure.nonexistent").is_none());
    }

    #[test]
    fn triangle_box_overlap_and_point_distance_agree_on_simple_cases() {
        let tri = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        assert!(triangle_box_overlap(Vec3::new(0.2, 0.2, 0.0), Vec3::splat(0.05), &tri));
        assert!(!triangle_box_overlap(Vec3::new(0.2, 0.2, 0.2), Vec3::splat(0.05), &tri));
        assert!(!triangle_box_overlap(Vec3::new(0.9, 0.9, 0.0), Vec3::splat(0.05), &tri));
        assert!((point_triangle_distance(Vec3::new(0.2, 0.2, 0.5), &tri) - 0.5).abs() < 1e-6);
        assert!((point_triangle_distance(Vec3::new(-1.0, 0.0, 0.0), &tri) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_closed_box_mesh_becomes_one_exact_proxy() {
        // Twelve triangles of a 2 × 1 × 0.5 box: the proxy must be that box.
        let (lo, hi) = (Vec3::new(-1.0, 0.0, -0.25), Vec3::new(1.0, 1.0, 0.25));
        let c = |i: usize| {
            Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            )
        };
        let quads = [
            [0, 1, 3, 2],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 3, 7, 6],
            [0, 2, 6, 4],
            [1, 3, 7, 5],
        ];
        let mut tris = Vec::new();
        for q in quads {
            tris.push([c(q[0]), c(q[1]), c(q[2])]);
            tris.push([c(q[0]), c(q[2]), c(q[3])]);
        }
        for fill in [ProxyFill::Enclosed, ProxyFill::AlongZ] {
            let generated = proxies_from_triangles(
                &tris,
                ProxySpec {
                    voxel_m: 0.05,
                    fill,
                    max_error_m: 0.05,
                },
            );
            assert_eq!(generated.boxes.len(), 1, "{fill:?}: {:?}", generated.boxes);
            let (min, max) = generated.boxes[0];
            assert!((min - lo).abs().max_element() < 1e-4, "{min:?}");
            assert!((max - hi).abs().max_element() < 1e-4, "{max:?}");
            assert!(generated.error_m < 0.03, "{fill:?} error {}", generated.error_m);
        }
    }

    #[test]
    fn every_kit_asset_loads_matches_its_nominal_bounds_and_keeps_its_openings_open() {
        if !assets_installed() {
            eprintln!("skipping: Pizza kit assets are not installed");
            return;
        }
        for asset in KIT {
            let mesh = load_mesh(asset.id).unwrap_or_else(|e| panic!("{}: {e}", asset.id));
            assert!(!mesh.indices.is_empty(), "{} is empty", asset.id);
            if asset.nominal_min[0].is_finite() {
                let lo = Vec3::from_array(asset.nominal_min);
                let hi = Vec3::from_array(asset.nominal_max);
                assert!(
                    (mesh.min - lo).abs().max_element() < 0.012
                        && (mesh.max - hi).abs().max_element() < 0.012,
                    "{} nominal bounds {lo:?}..{hi:?} but source {:?}..{:?}",
                    asset.id,
                    mesh.min,
                    mesh.max
                );
            }
            let triangles = mesh_triangles(&mesh);
            let proxies = proxies(asset.id).unwrap();
            for opening in asset.openings {
                // Sample the opening shrunk by 4 cm: no source triangle may cover it in the XY
                // projection, and no proxy may block a ray straight through it.
                let [x0, x1, y0, y1] = *opening;
                let steps = 24;
                for i in 0..=steps {
                    for j in 0..=steps {
                        let x = x0 + 0.04 + (x1 - x0 - 0.08) * i as f32 / steps as f32;
                        let y = y0 + 0.04 + (y1 - y0 - 0.08) * j as f32 / steps as f32;
                        let covered = triangles.iter().any(|t| {
                            let p = |v: Vec3| glam::Vec2::new(v.x, v.y);
                            point_in_triangle_2d(glam::Vec2::new(x, y), p(t[0]), p(t[1]), p(t[2]))
                        });
                        assert!(!covered, "{}: source geometry inside opening at ({x}, {y})", asset.id);
                        let blocked = proxies.boxes.iter().any(|(min, max)| {
                            x > min.x && x < max.x && y > min.y && y < max.y
                        });
                        assert!(!blocked, "{}: proxy blocks opening at ({x}, {y})", asset.id);
                    }
                }
            }
        }
    }

    #[test]
    fn every_kit_proxy_set_is_within_its_measured_error_bound() {
        if !assets_installed() {
            eprintln!("skipping: Pizza kit assets are not installed");
            return;
        }
        let mut report = Vec::new();
        let mut failures = Vec::new();
        for asset in KIT {
            let proxies = proxies(asset.id).unwrap();
            report.push(format!(
                "{:40} boxes {:4} error {:.3} m (bound {:.3}) volume {:.2} m3",
                asset.id,
                proxies.boxes.len(),
                proxies.error_m,
                proxies.bound_m,
                proxies.volume_m3
            ));
            if proxies.error_m > proxies.bound_m + 1e-6 {
                failures.push(format!(
                    "{}: measured proxy error {} exceeds its bound {}",
                    asset.id, proxies.error_m, proxies.bound_m
                ));
            }
            let has_proxy_part = asset.parts.iter().any(|part| part.proxy.is_some());
            if proxies.boxes.is_empty() == has_proxy_part {
                failures.push(format!("{}: proxy presence does not match its parts", asset.id));
            }
        }
        eprintln!("{}", report.join("\n"));
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn every_kit_source_is_in_the_reviewed_runtime_catalog() {
        let catalog: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string("catalog/runtime_assets.v1.json").unwrap())
                .unwrap();
        let entries = catalog["assets"].as_array().unwrap();
        for path in source_paths() {
            let entry = entries
                .iter()
                .find(|entry| entry["source"].as_str() == Some(path))
                .unwrap_or_else(|| panic!("kit source {path} has no runtime-catalog entry"));
            assert_eq!(entry["license"].as_str(), Some("pizzadoggy-local-purchase"), "{path}");
            let consumers: Vec<_> = entry["required_by"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|v| v.as_str())
                .collect();
            assert!(consumers.contains(&"code.building-kit"), "{path}: {consumers:?}");
        }
    }

    fn point_in_triangle_2d(p: glam::Vec2, a: glam::Vec2, b: glam::Vec2, c: glam::Vec2) -> bool {
        let d = (b - a).perp_dot(c - a);
        if d.abs() < 1e-12 {
            return false;
        }
        let u = (b - p).perp_dot(c - p) / d;
        let v = (c - p).perp_dot(a - p) / d;
        let w = 1.0 - u - v;
        u > 1e-6 && v > 1e-6 && w > 1e-6
    }
}
