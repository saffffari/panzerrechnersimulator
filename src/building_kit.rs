//! Building kit: the registry from a blueprint module's semantic asset id to its source mesh and
//! material, shared by every generated building.
//!
//! Owned by the building grammar of the urban-intro team (docs/URBAN_INTRO_TEAM_20260928.md). The
//! first nine entries are the Pizza modules of the featured house. This is the only asset registry
//! of the building runtime: blueprints name semantic asset ids, and every source path and material
//! is resolved here. A new entry needs a runtime-catalog
//! review (docs/RUNTIME_ASSET_CATALOG.md), and its collision proxies are generated from its source
//! geometry with a measured error bound (CLAUDE.md, second principle, rule 3).
//!
//! Every kit asset is built only from reviewed Pizza GLBs (all are in
//! `catalog/runtime_assets.v1.json`, consumer `code.building-kit`):
//! - a whole GLB;
//! - a GLB restricted to some of its source primitives, selected by glTF material name (a window
//!   wall without its glazing, a gate frame without its leaf) — geometry and UVs are untouched;
//! - a GLB cut to half-spaces in its own space (a gable triangle cut from a plank wall, a parapet
//!   strip cut from a brick wall): triangles are clipped, and positions, normals, UVs and tangents
//!   along a cut are interpolated on the source edges, so the texture keeps its scale where the
//!   grammar would otherwise squash it. The cut through a wall's two faces is closed by a cap at
//!   the source's UV density (`clip_mesh`);
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

use crate::gltf_load::{CpuImage, CpuMesh, PrimitiveFilter, Vertex};

/// Material class of a kit module, consumed by destruction (crush tier, shell damage) and sound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KitMaterial {
    Brick,
    Stone,
    Timber,
    Concrete,
    Metal,
    /// A snow deposit (`deposit_asset`): no solids, no rubble.
    Snow,
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
/// A slab hanging at 58°: its sloped faces step once per voxel in a column.
const HANGING_SLAB_PROXY: ProxySpec = ProxySpec {
    voxel_m: 0.05,
    fill: ProxyFill::AlongY,
    max_error_m: 0.12,
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

/// One reviewed source GLB, restricted by `filter`, cut by `clip`, placed by `transform`.
#[derive(Clone, Copy, Debug)]
pub struct KitPart {
    pub path: &'static str,
    pub filter: PrimitiveFilter,
    /// Half-spaces `[nx, ny, nz, d]` meaning `n · p <= d`, in the source GLB's own space, that
    /// the part's triangles are cut to (see [`clip_mesh`]). Empty for an uncut part.
    pub clip: &'static [[f32; 4]],
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
        clip: &[],
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
        clip: &[],
        transform: IDENTITY,
        proxy,
    }
}

const fn clipped(path: &'static str, clip: &'static [[f32; 4]], proxy: Option<ProxySpec>) -> KitPart {
    KitPart {
        path,
        filter: PrimitiveFilter {
            include: &[],
            exclude: &[],
        },
        clip,
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
        clip: &[],
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
const ROOF_HR_3_MIDDLE_ANGLE: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/roof_hr_3_middle_angle.glb";
const PLATFORM_B_HANDRAIL_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/platform_b_handrail_1.glb";
const WALL_RX_1: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/wall_rx_1.glb";
const WALL_RX_1_QUARTER_WIDTH: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/wall_rx_1_1_4_width.glb";
const WALL_RX_1_THIRD_HEIGHT_HALF_WIDTH: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Modular Structures/wall_rx_1_1_3_height_1_2_width.glb";
const GRAVEL_PILE_HR_2: &str = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/Debris & Misc/gravel_pile_hr_2.glb";
// The plain board crate of the first Mega Pack. The Mega Pack II `Props/wooden_crate_1.glb` it
// replaced (verifier, 2026-09-29) maps red "DO NOT STACK" and "FRAGILE" stencils onto all four
// sides: readable modern English text in the world (CLAUDE.md first principle; P S10).
const WOODEN_CRATE_PLAIN: &str = "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/Large Props/wooden_crate_1.glb";
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
/// `wall_hr_1` cut to its lowest metre: a brick strip with the wall's own brick scale.
const STRIP_CLIP: &[[f32; 4]] = &[[0.0, 1.0, 0.0, 1.0]];
/// `wall_rx_1` cut to the triangle under two lines from its top centre (0, 3) to its bottom
/// corners (±2, 0): a symmetric gable of boards at the wall's own board scale.
const GABLE_CLIP: &[[f32; 4]] = &[[1.5, 1.0, 0.0, 3.0], [-1.5, 1.0, 0.0, 3.0]];

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
    // 4.0 × 1.0 × 0.4 m brick strip cut from the plain wall bay, so a 0.8-1.0 m parapet
    // (P G4: 07-08, 07-16) or a 0.65 m wooden-house plinth (P G6: 03-04) keeps the wall's brick
    // courses instead of squashing a 3 m bay to a third of its height.
    KitAsset {
        id: "urban.brick.wall_strip",
        path: WALL_HR_1,
        material: KitMaterial::Brick,
        parts: &[clipped(WALL_HR_1, STRIP_CLIP, Some(WALL_PROXY))],
        nominal_min: [-2.0, 0.0, -0.2],
        nominal_max: [2.0, 1.0, 0.2],
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
    // One pitched slope of corrugated sheet iron on its purlins: 4 m run rising 1.06 m toward +X
    // (1.0 m to the underside), 4 m along Z. A gabled roof is two slopes meeting at the ridge,
    // each carried by its own eaves wall (P G4: 03-04, 07-12, 07-01; G16).
    KitAsset {
        id: "urban.metal.roof",
        path: ROOF_HR_3_MIDDLE_ANGLE,
        material: KitMaterial::Metal,
        parts: &[placed(ROOF_HR_3_MIDDLE_ANGLE, &[], [0.0, -0.05, 0.0], 0.0, [1.0; 3], Some(SHEET_PROXY))],
        nominal_min: [-2.0, 0.0, -2.0],
        nominal_max: [2.0, 1.06, 2.0],
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
    // What a failed reinforced-concrete slab leaves on a wall that stands (P G9, G12: "floor stubs
    // (concrete) ... showing on the inner faces", [04-04], [07-16]): the slab's edge strip along
    // its +Z side, broken off 0.30-0.62 m from the wall in six ragged segments. The grammar
    // places it with the failed slab's thickness scale along the standing wall.
    KitAsset {
        id: "urban.concrete.slab_stub",
        path: FLOOR_CEILING_HR_1,
        material: KitMaterial::Concrete,
        parts: &[
            clipped(FLOOR_CEILING_HR_1, &[[-1.0, 0.0, 0.0, 2.0], [1.0, 0.0, 0.0, -1.333_333], [0.0, 0.0, -1.0, -1.45]], Some(SLAB_PROXY)),
            clipped(FLOOR_CEILING_HR_1, &[[-1.0, 0.0, 0.0, 1.333_333], [1.0, 0.0, 0.0, -0.666_667], [0.0, 0.0, -1.0, -1.65]], Some(SLAB_PROXY)),
            clipped(FLOOR_CEILING_HR_1, &[[-1.0, 0.0, 0.0, 0.666_667], [1.0, 0.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.38]], Some(SLAB_PROXY)),
            clipped(FLOOR_CEILING_HR_1, &[[-1.0, 0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.666_667], [0.0, 0.0, -1.0, -1.58]], Some(SLAB_PROXY)),
            clipped(FLOOR_CEILING_HR_1, &[[-1.0, 0.0, 0.0, -0.666_667], [1.0, 0.0, 0.0, 1.333_333], [0.0, 0.0, -1.0, -1.50]], Some(SLAB_PROXY)),
            clipped(FLOOR_CEILING_HR_1, &[[-1.0, 0.0, 0.0, -1.333_333], [1.0, 0.0, 0.0, 2.0], [0.0, 0.0, -1.0, -1.70]], Some(SLAB_PROXY)),
        ],
        nominal_min: [-2.0, 0.0, 1.38],
        nominal_max: [2.0, 0.4, 2.0],
        openings: &[],
    },
    // A reinforced-concrete slab hanging from its bars at the edge of a collapse (P G9: "hang from
    // their bars at a broken edge", [04-04], [07-16]): the 4 × 4 m slab thinned to 0.18 m and
    // turned 58° down about its top +Z edge (the hinge at y = 0.18, z = 2), its far edge 3.3 m
    // below; the underside at the hinge swings 0.15 m into the wall it hangs from. The grammar
    // places the hinge on the standing wall's inner face at the failed slab's floor level and
    // flattens the angle by the module's Y and Z scale to the storeys open below it.
    KitAsset {
        id: "urban.concrete.slab_hanging",
        path: FLOOR_CEILING_HR_1,
        material: KitMaterial::Concrete,
        parts: &[KitPart {
            path: FLOOR_CEILING_HR_1,
            filter: PrimitiveFilter { include: &[], exclude: &[] },
            clip: &[],
            transform: PartTransform {
                translation: [0.0, -1.611_4, 1.092_8],
                rotation_deg: [-58.0, 0.0, 0.0],
                scale: [1.0, 0.45, 1.0],
            },
            proxy: Some(HANGING_SLAB_PROXY),
        }],
        nominal_min: [-2.0, -3.307, -0.12],
        nominal_max: [2.0, 0.18, 2.153],
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
    // Boarded gable: the plank wall cut to a triangle 4 m wide and 3 m high, apex over its
    // centre; scaled to the roof pitch (P G4: 03-04 boarded gable ends).
    KitAsset {
        id: "urban.timber.gable",
        path: WALL_RX_1,
        material: KitMaterial::Timber,
        parts: &[clipped(WALL_RX_1, GABLE_CLIP, Some(SLOPED_WALL_PROXY))],
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
    // A board crate with battens, 0.72 × 0.36 × 0.50 m, without markings.
    KitAsset {
        id: "urban.timber.crate",
        path: WOODEN_CRATE_PLAIN,
        material: KitMaterial::Timber,
        parts: &[whole(WOODEN_CRATE_PLAIN, Some(ROD_PROXY))],
        nominal_min: [-0.36, 0.0, -0.252],
        nominal_max: [0.36, 0.36, 0.252],
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

/// A kit asset by id: one of `KIT`, or a registered variant of one (`variant_assets`).
pub fn asset(id: &str) -> Option<&'static KitAsset> {
    KIT.iter()
        .find(|asset| asset.id == id)
        .or_else(|| {
            let registry = registry();
            registry.by_id.get(id).map(|&i| &registry.assets[i])
        })
        .or_else(|| deposit_entry(id))
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

/// A part's source mesh in its own space: filtered and cut, not yet placed.
fn load_source(part: &KitPart) -> anyhow::Result<CpuMesh> {
    let mesh = if part.filter.include.is_empty() && part.filter.exclude.is_empty() {
        crate::gltf_load::load_glb(part.path)?
    } else {
        crate::gltf_load::load_glb_filtered(part.path, part.filter)?
    };
    Ok(clip_mesh(mesh, part.clip))
}

fn lerp_vertex(a: &Vertex, b: &Vertex, t: f32) -> Vertex {
    let mix3 = |p: [f32; 3], q: [f32; 3]| Vec3::from_array(p).lerp(Vec3::from_array(q), t);
    let normal = mix3(a.normal, b.normal).normalize_or_zero();
    let tangent = mix3([a.tangent[0], a.tangent[1], a.tangent[2]], [b.tangent[0], b.tangent[1], b.tangent[2]])
        .normalize_or_zero();
    Vertex {
        pos: mix3(a.pos, b.pos).to_array(),
        normal: normal.to_array(),
        uv: [a.uv[0] + (b.uv[0] - a.uv[0]) * t, a.uv[1] + (b.uv[1] - a.uv[1]) * t],
        tangent: [tangent.x, tangent.y, tangent.z, a.tangent[3]],
    }
}

/// Cut a mesh to the half-spaces `n · p <= d` (`[nx, ny, nz, d]`), one plane after another.
/// Each triangle is clipped (Sutherland–Hodgman) and the remaining convex polygon is fanned;
/// attributes of a new vertex are interpolated along the source edge it lies on, so the texture
/// keeps its scale. The kit's wall bays are open slabs (two faces and a crown), so each cut is
/// closed the way a slab is: the cut edges on the plane are merged into straight segments, and
/// every pair of parallel, equal segments offset across the plane (the two faces of a wall) is
/// bridged by a cap facing `+n`, textured at the source's own UV density.
pub fn clip_mesh(mesh: CpuMesh, planes: &[[f32; 4]]) -> CpuMesh {
    if planes.is_empty() {
        return mesh;
    }
    const EPS: f32 = 1e-4;
    let density = uv_density(&mesh);
    let mut triangles: Vec<[Vertex; 3]> = mesh
        .indices
        .chunks_exact(3)
        .map(|t| [mesh.vertices[t[0] as usize], mesh.vertices[t[1] as usize], mesh.vertices[t[2] as usize]])
        .collect();
    for plane in planes {
        let raw = Vec3::new(plane[0], plane[1], plane[2]);
        let (normal, offset) = (raw / raw.length(), plane[3] / raw.length());
        let side = |v: &Vertex| normal.dot(Vec3::from_array(v.pos)) - offset;
        let mut kept = Vec::with_capacity(triangles.len());
        let mut segments: Vec<(Vertex, Vertex)> = Vec::new();
        for tri in &triangles {
            let coplanar = tri.iter().all(|v| side(v).abs() <= EPS);
            let mut polygon: Vec<Vertex> = Vec::with_capacity(5);
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                let (da, db) = (side(&a), side(&b));
                if da <= EPS {
                    polygon.push(a);
                }
                if (da < -EPS && db > EPS) || (da > EPS && db < -EPS) {
                    polygon.push(lerp_vertex(&a, &b, da / (da - db)));
                }
            }
            if polygon.len() < 3 {
                continue;
            }
            if !coplanar {
                for k in 0..polygon.len() {
                    let (a, b) = (polygon[k], polygon[(k + 1) % polygon.len()]);
                    let long = (Vec3::from_array(a.pos) - Vec3::from_array(b.pos)).length() > EPS;
                    if long && side(&a).abs() <= EPS && side(&b).abs() <= EPS {
                        segments.push((a, b));
                    }
                }
            }
            for k in 1..polygon.len() - 1 {
                kept.push([polygon[0], polygon[k], polygon[k + 1]]);
            }
        }
        let caps = cap_segments(merge_collinear(segments), normal, density);
        kept.extend(caps);
        triangles = kept;
    }
    let mut vertices: Vec<Vertex> = Vec::with_capacity(triangles.len() * 3);
    let mut indices: Vec<u32> = Vec::with_capacity(triangles.len() * 3);
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for tri in triangles {
        for v in tri {
            indices.push(vertices.len() as u32);
            min = min.min(Vec3::from_array(v.pos));
            max = max.max(Vec3::from_array(v.pos));
            vertices.push(v);
        }
    }
    CpuMesh {
        vertices,
        indices,
        min,
        max,
        ..mesh
    }
}

/// Texture units per metre of a mesh (total UV edge length over total position edge length).
fn uv_density(mesh: &CpuMesh) -> f32 {
    let (mut uv, mut xyz) = (0.0_f32, 0.0_f32);
    for tri in mesh.indices.chunks_exact(3) {
        for k in 0..3 {
            let (a, b) = (&mesh.vertices[tri[k] as usize], &mesh.vertices[tri[(k + 1) % 3] as usize]);
            let dp = (Vec3::from_array(a.pos) - Vec3::from_array(b.pos)).length();
            if dp > 1e-4 {
                xyz += dp;
                uv += ((a.uv[0] - b.uv[0]).powi(2) + (a.uv[1] - b.uv[1]).powi(2)).sqrt();
            }
        }
    }
    if xyz > 0.0 {
        uv / xyz
    } else {
        0.25
    }
}

/// Join cut segments that continue each other in a straight line (a face split into triangles
/// is cut into several collinear pieces).
fn merge_collinear(mut segments: Vec<(Vertex, Vertex)>) -> Vec<(Vertex, Vertex)> {
    let p = |v: &Vertex| Vec3::from_array(v.pos);
    let close = |a: Vec3, b: Vec3| (a - b).length() <= 1e-4;
    loop {
        let mut merged = None;
        'search: for i in 0..segments.len() {
            for j in 0..segments.len() {
                if i == j {
                    continue;
                }
                let (a, b) = segments[i];
                let (c, e) = segments[j];
                let di = (p(&b) - p(&a)).normalize_or_zero();
                let dj = (p(&e) - p(&c)).normalize_or_zero();
                if di.cross(dj).length() > 1e-3 {
                    continue;
                }
                // Shared endpoint, the two pieces on either side of it.
                let joined = if close(p(&b), p(&c)) && di.dot(dj) > 0.0 {
                    Some((a, e))
                } else if close(p(&b), p(&e)) && di.dot(dj) < 0.0 {
                    Some((a, c))
                } else if close(p(&a), p(&c)) && di.dot(dj) < 0.0 {
                    Some((b, e))
                } else if close(p(&a), p(&e)) && di.dot(dj) > 0.0 {
                    Some((c, b))
                } else {
                    None
                };
                if let Some(segment) = joined {
                    merged = Some((i, j, segment));
                    break 'search;
                }
            }
        }
        let Some((i, j, segment)) = merged else {
            return segments;
        };
        let (hi, lo) = (i.max(j), i.min(j));
        segments.swap_remove(hi);
        segments.swap_remove(lo);
        segments.push(segment);
    }
}

/// Caps between parallel, equal cut segments offset across the cut plane (the two faces of a
/// slab): two triangles each, facing `normal`, UVs continued from the first segment across the
/// cap at the source's density.
fn cap_segments(segments: Vec<(Vertex, Vertex)>, normal: Vec3, density: f32) -> Vec<[Vertex; 3]> {
    let p = |v: &Vertex| Vec3::from_array(v.pos);
    let mut used = vec![false; segments.len()];
    let mut caps = Vec::new();
    for i in 0..segments.len() {
        if used[i] {
            continue;
        }
        let (a, b) = segments[i];
        let (pa, pb) = (p(&a), p(&b));
        let len = (pb - pa).length();
        let dir = (pb - pa) / len.max(1e-9);
        let mid = (pa + pb) * 0.5;
        let mut best: Option<(usize, f32)> = None;
        for j in 0..segments.len() {
            if j == i || used[j] {
                continue;
            }
            let (c, e) = segments[j];
            let (pc, pe) = (p(&c), p(&e));
            let other = pe - pc;
            let offset = (pc + pe) * 0.5 - mid;
            let parallel = dir.cross(other.normalize_or_zero()).length() < 1e-3;
            let same = (other.length() - len).abs() < 1e-3;
            let across = offset.dot(dir).abs() < 1e-3 && offset.dot(normal).abs() < 1e-4 && offset.length() > 1e-4;
            if parallel && same && across && best.map_or(true, |(_, d)| offset.length() < d) {
                best = Some((j, offset.length()));
            }
        }
        let Some((j, width)) = best else {
            continue;
        };
        used[i] = true;
        used[j] = true;
        let (c, e) = segments[j];
        let offset = (p(&c) + p(&e)) * 0.5 - mid;
        // Opposite corners: the end of segment j nearest to each end of segment i.
        let (qa, qb) = if (p(&c) - (pa + offset)).length() < (p(&e) - (pa + offset)).length() {
            (p(&c), p(&e))
        } else {
            (p(&e), p(&c))
        };
        let along = dir;
        let corner = |pos: Vec3, from: &Vertex, shift: f32| Vertex {
            pos: pos.to_array(),
            normal: normal.to_array(),
            uv: [from.uv[0], from.uv[1] + shift * density],
            tangent: [along.x, along.y, along.z, 1.0],
        };
        let (v0, v1, v2, v3) = (corner(pa, &a, 0.0), corner(pb, &b, 0.0), corner(qb, &b, width), corner(qa, &a, width));
        let facing = (pb - pa).cross(qa - pa).dot(normal);
        if facing > 0.0 {
            caps.push([v0, v1, v3]);
            caps.push([v1, v2, v3]);
        } else {
            caps.push([v0, v3, v1]);
            caps.push([v1, v3, v2]);
        }
    }
    caps
}

fn load_part(part: &KitPart) -> anyhow::Result<CpuMesh> {
    let mesh = load_source(part)?;
    Ok(if part.transform == IDENTITY {
        mesh
    } else {
        mesh.transformed(part.transform.matrix())
    })
}

/// Build a kit asset's mesh in asset space: the only correct way to load a kit id (filters and
/// composites are applied here, and a variant's damage and deposits). A composite keeps its first
/// part's textures (see module docs).
pub fn load_mesh(id: &str) -> anyhow::Result<CpuMesh> {
    if let Some(variant) = variant(id) {
        return cached_mesh(id, || variant_mesh(variant));
    }
    if let Some(host) = id.strip_prefix(DEPOSIT_PREFIX) {
        return cached_mesh(id, || deposit_mesh(host));
    }
    base_mesh(id)
}

/// Built meshes of the bases, variants and deposits, kept for the process: a variant rebuilds
/// from its base and a deposit from its host, and each base's source GLB is imported and its
/// textures decoded once instead of once per variant.
fn cached_mesh(id: &str, build: impl FnOnce() -> anyhow::Result<CpuMesh>) -> anyhow::Result<CpuMesh> {
    static CACHE: OnceLock<Mutex<HashMap<String, CpuMesh>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().expect("kit mesh cache").get(id) {
        return Ok(found.clone());
    }
    let mesh = build()?;
    cache.lock().expect("kit mesh cache").insert(id.to_owned(), mesh.clone());
    Ok(mesh)
}

fn base_mesh(id: &str) -> anyhow::Result<CpuMesh> {
    cached_mesh(id, || build_base_mesh(id))
}

fn build_base_mesh(id: &str) -> anyhow::Result<CpuMesh> {
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

// ── Damaged, sooted and snowed variants of the wall bays (ruin detail, round 2) ─────────────────
//
// A variant is a kit asset of its own (its own semantic id, so a module names its state) built from
// one base bay: the base mesh, the damage's cuts (`damage::damage_cuts`: holes, breach edges,
// ragged tops, pits and spalls, joist pockets), snow on its ledges, and every triangle pointed into
// the texture atlas copy its deposit selects (clean, soot, snow). Variants of one damage share
// their collision: their proxies are generated from the base with the structural cuts (holes,
// edges, tops) and measured against the surface with every cut, so pits, spalls and pockets stay
// within the recorded error. The shapes table holds one entry per base and damage
// (`{base}.v.{damage}`); `shape` maps every variant to it.

pub use damage::Damage;

/// How the fire left a bay (P G9, G15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Burn {
    Clean,
    /// A burned storey's wall on an outer line: soot inside the room, over each opening's head
    /// outside, and a lighter general darkening of the rest of its outer face.
    Sooted,
    /// Sooted, with the pockets the burned timber joists left at its foot.
    Burnt,
    /// A wall between two burned rooms: soot on both faces.
    SootedBoth,
}

impl Burn {
    pub const ALL: [Burn; 4] = [Burn::Clean, Burn::Sooted, Burn::Burnt, Burn::SootedBoth];

    pub fn token(self) -> &'static str {
        match self {
            Burn::Clean => "clean",
            Burn::Sooted => "soot",
            Burn::Burnt => "burnt",
            Burn::SootedBoth => "soot2",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BayKind {
    Window,
    Wall,
    Door,
}

/// The bays that take variants and which variants each kind has: windows take every damage and
/// the outer-line fires, always with snow on their sills (the district is January 1943, P L8);
/// plain bays take every damage and every fire; doorways take the fires only.
const VARIANT_BASES: &[(&str, BayKind)] = &[
    ("urban.brick.window", BayKind::Window),
    ("urban.brick.window_dark", BayKind::Window),
    ("pizza.structure.wall", BayKind::Wall),
    ("urban.brick.wall_dark", BayKind::Wall),
    ("urban.brick.door", BayKind::Door),
    ("urban.brick.door_dark", BayKind::Door),
    ("pizza.structure.doorway", BayKind::Door),
];

/// One registered variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Variant {
    pub base: &'static str,
    pub damage: Damage,
    pub burn: Burn,
    pub snow: bool,
}

fn variant_name(base: &str, damage: Damage, burn: Burn, snow: bool) -> String {
    format!("{base}.v.{}.{}.{}", damage.token(), burn.token(), if snow { "snow" } else { "bare" })
}

fn collision_name(base: &str, damage: Damage) -> String {
    format!("{base}.v.{}", damage.token())
}

struct Registry {
    assets: Vec<KitAsset>,
    variants: Vec<Variant>,
    by_id: HashMap<&'static str, usize>,
    /// (base, damage) of every collision entry, in id order.
    collision: Vec<(&'static str, Damage, &'static str)>,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut assets = Vec::new();
        let mut variants = Vec::new();
        let mut collision = Vec::new();
        for &(base, kind) in VARIANT_BASES {
            let source = KIT.iter().find(|a| a.id == base).expect("variant base in KIT");
            let damages: &[Damage] = match kind {
                BayKind::Door => &[Damage::Whole],
                _ => &Damage::ALL,
            };
            let burns: &[Burn] = match kind {
                BayKind::Window => &[Burn::Clean, Burn::Sooted, Burn::Burnt],
                BayKind::Wall => &Burn::ALL,
                BayKind::Door => &[Burn::Sooted, Burn::Burnt, Burn::SootedBoth],
            };
            // Every variant is the winter district's: snow lies on whatever ledges it has.
            let snow = true;
            for &damage in damages {
                if damage != Damage::Whole {
                    let id: &'static str = Box::leak(collision_name(base, damage).into_boxed_str());
                    collision.push((base, damage, id));
                }
                for &burn in burns {
                    // A whole, clean bay without a sill has no ledge: it is the base itself.
                    if damage == Damage::Whole && burn == Burn::Clean && kind != BayKind::Window {
                        continue;
                    }
                    // A breach-edged, arched, broken-topped or holed bay shows its fire's soot but
                    // not the joist pockets (they stay with whole and pitted bays): fewer variants
                    // for the few such bays, fewer instanced draws.
                    if burn == Burn::Burnt && !matches!(damage, Damage::Whole | Damage::Pitted) {
                        continue;
                    }
                    let id: &'static str = Box::leak(variant_name(base, damage, burn, snow).into_boxed_str());
                    assets.push(KitAsset { id, ..*source });
                    variants.push(Variant { base, damage, burn, snow });
                }
            }
        }
        collision.sort_by(|a, b| a.2.cmp(b.2));
        let by_id = assets.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
        Registry { assets, variants, by_id, collision }
    })
}

/// Every registered variant asset.
pub fn variant_assets() -> &'static [KitAsset] {
    &registry().assets
}

pub fn variant(id: &str) -> Option<&'static Variant> {
    let registry = registry();
    registry.by_id.get(id).map(|&i| &registry.variants[i])
}

/// The registered id of `base` in the given state, or the nearest one it has: a doorway keeps no
/// shell damage, a clean whole bare bay is the base itself.
pub fn variant_asset(base: &str, damage: Damage, burn: Burn, snow: bool) -> Option<&'static str> {
    let registry = registry();
    let (_, kind) = VARIANT_BASES.iter().find(|(b, _)| *b == base)?;
    let damage = if *kind == BayKind::Door { Damage::Whole } else { damage };
    let burn = if *kind == BayKind::Window && burn == Burn::SootedBoth { Burn::Sooted } else { burn };
    let burn = if burn == Burn::Burnt && !matches!(damage, Damage::Whole | Damage::Pitted) { Burn::Sooted } else { burn };
    if !snow {
        // Every registered variant is a winter one; a bay out of the winter keeps its base.
        return None;
    }
    let name = variant_name(base, damage, burn, snow);
    registry.by_id.get(name.as_str()).map(|&i| registry.assets[i].id)
}

/// Whether the asset is a bay that takes damage, soot and snow variants.
pub fn takes_variants(id: &str) -> bool {
    VARIANT_BASES.iter().any(|(base, _)| *base == id)
}

/// Whether the bay takes shell damage variants too (window and plain bays; doorways take the
/// fire's only).
pub fn takes_damage(id: &str) -> bool {
    VARIANT_BASES.iter().any(|(base, kind)| *base == id && *kind != BayKind::Door)
}

/// The base asset a variant was built from (a base's own id otherwise): its rubble, material and
/// sound are the base's.
pub fn base_of(id: &str) -> &str {
    variant(id).map_or(id, |v| v.base)
}

/// The shape-table id carrying a variant's collision.
fn collision_of(id: &str) -> Option<&'static str> {
    let v = variant(id)?;
    if v.damage == Damage::Whole {
        return Some(v.base);
    }
    registry()
        .collision
        .iter()
        .find(|(base, damage, _)| *base == v.base && *damage == v.damage)
        .map(|(_, _, id)| *id)
}

/// The asset frame of a base bay as the damage layouts see it.
fn bay_shape(asset: &KitAsset) -> damage::BayShape {
    let kind = VARIANT_BASES.iter().find(|(b, _)| *b == asset.id).map(|(_, k)| *k);
    damage::BayShape {
        opening: asset.openings.first().copied(),
        sill: kind == Some(BayKind::Window),
        height: asset.nominal_max[1] - asset.nominal_min[1],
    }
}

fn damage_cuts_for(base: &str, damage: Damage, pockets: bool) -> anyhow::Result<Vec<damage::Cut>> {
    let asset = asset(base).ok_or_else(|| anyhow::anyhow!("unknown variant base {base:?}"))?;
    // One seed per base and damage: every burn of a damage has the same holes, edges and pits.
    let seed = damage::seed_of(&collision_name(base, damage));
    Ok(damage::damage_cuts(bay_shape(asset), damage, pockets, seed))
}

/// The base bay with the damage's structural cuts only: what collision sees.
fn collision_mesh(base: &str, damage: Damage) -> anyhow::Result<CpuMesh> {
    let mesh = base_mesh(base)?;
    let cuts: Vec<damage::Cut> = damage_cuts_for(base, damage, false)?.into_iter().filter(|c| c.structural).collect();
    Ok(damage::cut_mesh(&mesh, &cuts))
}

/// The base bay with every cut of the damage and of a burnt floor's pockets: the surface the
/// collision's error is measured against.
fn surface_mesh(base: &str, damage: Damage, pockets: bool) -> anyhow::Result<CpuMesh> {
    let mesh = base_mesh(base)?;
    Ok(damage::cut_mesh(&mesh, &damage_cuts_for(base, damage, pockets)?))
}

fn variant_mesh(v: &Variant) -> anyhow::Result<CpuMesh> {
    let asset = asset(v.base).ok_or_else(|| anyhow::anyhow!("unknown variant base {:?}", v.base))?;
    let base = base_mesh(v.base)?;
    let shape = bay_shape(asset);
    let cuts = damage_cuts_for(v.base, v.damage, v.burn == Burn::Burnt)?;
    let mesh = damage::cut_mesh(&base, &cuts);
    let mut triangles: Vec<[Vertex; 3]> = mesh
        .indices
        .chunks_exact(3)
        .map(|t| [mesh.vertices[t[0] as usize], mesh.vertices[t[1] as usize], mesh.vertices[t[2] as usize]])
        .collect();
    let mut snowed = vec![false; triangles.len()];
    if v.snow {
        // About 6 cm of snow on the ledges of the bay as the shelling and the fire left it: the
        // sills, the treads of a stepped breach edge, the bottom of a hole, and a broken top
        // (whose crown the fallen bay above no longer covers; an unbroken crown is the next
        // storey's base). The ledges are those of the structural cuts: the few centimetres of a
        // pit's or a spall's lip carry none (P L8, V11: snow on ledges, sills and wall tops).
        let structural: Vec<damage::Cut> = cuts.iter().filter(|c| c.structural).cloned().collect();
        let ledges = damage::cut_mesh(&base, &structural);
        let below = if matches!(v.damage, Damage::TopA | Damage::TopB) { f32::INFINITY } else { shape.height - 0.1 };
        let snow = damage::ledge_snow(&ledges, below, 0.05, 0.06 / damage::TYPICAL_SCALE.y);
        snowed.extend(std::iter::repeat(true).take(snow.len()));
        triangles.extend(snow);
    }
    let texture = base
        .texture
        .clone()
        .ok_or_else(|| anyhow::anyhow!("variant base {} has no texture", v.base))?;
    let atlas = damage::atlas(v.base, &texture);
    let fire = match v.burn {
        Burn::Clean => damage::Fire::Clean,
        Burn::Sooted | Burn::Burnt => damage::Fire::Outer,
        Burn::SootedBoth => damage::Fire::Both,
    };
    let heads = if fire == damage::Fire::Outer { damage::head_zones(shape) } else { Vec::new() };
    let painted = damage::paint_regions(triangles, &snowed, fire, &heads, &atlas);
    let mut out = damage::mesh_from_triangles(painted, &base);
    out.texture = Some(atlas.image);
    Ok(out)
}

/// Collision proxies of a variant or a collision entry: generated from the base with the
/// structural cuts, their error measured against the surface with every cut.
fn variant_proxies(id: &str) -> anyhow::Result<Option<Arc<KitProxies>>> {
    let registry = registry();
    let (base, damage) = if let Some(&(base, damage, _)) = registry.collision.iter().find(|(_, _, c)| *c == id) {
        (base, damage)
    } else if let Some(v) = variant(id) {
        if v.damage == Damage::Whole {
            return proxies(v.base).map(Some);
        }
        (v.base, v.damage)
    } else {
        return Ok(None);
    };
    let key = collision_of_pair(base, damage);
    if let Some(found) = proxy_cache().lock().expect("kit proxy cache").get(key) {
        return Ok(Some(found.clone()));
    }
    let source = asset(base).ok_or_else(|| anyhow::anyhow!("unknown variant base {base:?}"))?;
    anyhow::ensure!(source.parts.len() == 1, "variant base {base} must be a single part");
    let spec = source.parts[0].proxy.ok_or_else(|| anyhow::anyhow!("variant base {base} has no proxy spec"))?;
    let collision = collision_mesh(base, damage)?;
    let reference = surface_mesh(base, damage, true)?;
    let generated = proxies_against(&mesh_triangles(&collision), &mesh_triangles(&reference), spec);
    let volume_m3 = generated.boxes.iter().map(|(a, b)| (*b - *a).x * (*b - *a).y * (*b - *a).z).sum();
    let result = Arc::new(KitProxies {
        boxes: generated.boxes,
        error_m: generated.error_m,
        bound_m: spec.max_error_m + VARIANT_SURFACE_ERROR_M,
        volume_m3,
    });
    proxy_cache().lock().expect("kit proxy cache").insert(key, result.clone());
    Ok(Some(result))
}

fn collision_of_pair(base: &str, damage: Damage) -> &'static str {
    registry()
        .collision
        .iter()
        .find(|(b, d, _)| *b == base && *d == damage)
        .map(|(_, _, id)| *id)
        .expect("registered collision entry")
}

// ── Snow on exposed tops ─────────────────────────────────────────────────────────────────────────
//
// The snow lying on a module's top once nothing stands on it (a ruin's wall crowns, parapets,
// cornices, stacks, gable slopes, the top balcony; P L8, V11: "on every ledge, wall top and
// cornice", [04-02], [04-28], [07-01]) is an asset of its own, `urban.snow.on.<host>`, placed with
// the host's transform as an attached `Debris` module: it has no solids, fails with its host, and
// its mesh is `damage::ledge_snow` on the host's topmost surfaces with the district's snow albedo.

const DEPOSIT_PREFIX: &str = "urban.snow.on.";

fn deposits() -> &'static Mutex<HashMap<String, &'static KitAsset>> {
    static DEPOSITS: OnceLock<Mutex<HashMap<String, &'static KitAsset>>> = OnceLock::new();
    DEPOSITS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The hosts whose tops carry a deposit: the brick bays and their variants, parapet strips,
/// cornice bands, gable halves, stacks, piers and balconies. (Boarded gables and walls are steeper
/// or open-topped in their asset frame and take none.)
const DEPOSIT_HOSTS: &[&str] = &[
    "urban.brick.window",
    "urban.brick.window_dark",
    "pizza.structure.wall",
    "urban.brick.wall_dark",
    "urban.brick.door",
    "urban.brick.door_dark",
    "pizza.structure.doorway",
    "urban.brick.wall_strip",
    "urban.brick.band",
    "urban.brick.gable",
    "urban.brick.stack",
    "urban.brick.pier",
    "urban.concrete.balcony",
];

/// The asset of the snow on `host`'s top, registered on first use (`None` for a host without a
/// top to carry it).
pub fn deposit_asset(host: &str) -> Option<&'static str> {
    if host.starts_with(DEPOSIT_PREFIX) || !DEPOSIT_HOSTS.contains(&base_of(host)) {
        return None;
    }
    // A variant's crown is its base's (holes, pits, spalls, pockets and soot lie below the
    // crown, an arch is at the foot, a broken top carries its own snow, and a breach edge's last
    // step is at most 0.07 of the bay wide at the crown, about 5 cm): one deposit per base serves
    // them all.
    let host = variant(host).map_or(host, |v| v.base);
    let id = format!("{DEPOSIT_PREFIX}{host}");
    let mut map = deposits().lock().expect("deposit registry");
    if let Some(found) = map.get(&id) {
        return Some(found.id);
    }
    let source = asset(host)?;
    let leaked_id: &'static str = Box::leak(id.clone().into_boxed_str());
    let entry: &'static KitAsset = Box::leak(Box::new(KitAsset {
        id: leaked_id,
        material: KitMaterial::Snow,
        openings: &[],
        ..*source
    }));
    map.insert(id, entry);
    Some(leaked_id)
}

fn deposit_entry(id: &str) -> Option<&'static KitAsset> {
    let host = id.strip_prefix(DEPOSIT_PREFIX)?;
    deposit_asset(host)?;
    deposits().lock().expect("deposit registry").get(id).copied()
}

/// The shared one-texel snow texture of every deposit (the district's aged snow, 0.86 linear).
fn snow_texture() -> CpuImage {
    static TEXEL: OnceLock<CpuImage> = OnceLock::new();
    TEXEL
        .get_or_init(|| {
            let [r, g, b] = damage::SNOW_SRGB;
            CpuImage::new(1, 1, vec![r, g, b, 255])
        })
        .clone()
}

/// The host's topmost surfaces carry the deposit: everything within 0.12 m of its top, or a
/// sloped top (a gable's), whole.
fn deposit_mesh(host: &str) -> anyhow::Result<CpuMesh> {
    let mesh = load_mesh(host)?;
    let sloped = host.starts_with(kit_ids::GABLE_BRICK);
    let above = if sloped { f32::NEG_INFINITY } else { mesh.max.y - 0.12 };
    let snow = damage::ledge_snow_between(&mesh, above, f32::INFINITY, 0.05, 0.08 / damage::TYPICAL_SCALE.y);
    let mut out = damage::mesh_from_triangles(snow, &mesh);
    out.texture = Some(snow_texture());
    out.metallic_roughness_texture = None;
    out.normal_texture = None;
    out.metallic_factor = 0.0;
    out.roughness_factor = 1.0;
    anyhow::ensure!(!out.vertices.is_empty(), "no top to lay snow on in {host}");
    Ok(out)
}

mod kit_ids {
    pub const GABLE_BRICK: &str = "urban.brick.gable";
}

/// How far a variant's surface cuts may reach into its proxies beyond the base's own bound:
/// the deepest pit (0.09 m), spall (0.12 m) or joist pocket (0.25 m) of `damage::damage_cuts`,
/// at the typical module depth scale. The kit test asserts every variant's measured error
/// against its base's bound plus this.
pub const VARIANT_SURFACE_ERROR_M: f32 = 0.25 / damage::TYPICAL_SCALE.z;

/// Collision proxies generated from the asset's source triangles (cached per asset id).
pub fn proxies(id: &str) -> anyhow::Result<Arc<KitProxies>> {
    if let Some(found) = variant_proxies(id)? {
        return Ok(found);
    }
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
        if !part_quarter_turn(part) {
            // An oblique part (the hanging slab) is voxelised where it lies: its source boxes
            // turned would only be their bounding boxes.
            let placed = load_part(part)?;
            let generated = proxies_from_triangles(&mesh_triangles(&placed), spec);
            for (a, b) in generated.boxes {
                volume_m3 += (b - a).x * (b - a).y * (b - a).z;
                boxes.push((a, b));
            }
            error_m = error_m.max(generated.error_m);
            bound_m = bound_m.max(spec.max_error_m);
            continue;
        }
        let mesh = load_source(part)?;
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

/// Whether a part is placed by a quarter turn about Y (its source-space proxies stay exact boxes
/// when turned).
fn part_quarter_turn(part: &KitPart) -> bool {
    let r = part.transform.rotation_deg;
    r[0] == 0.0 && r[2] == 0.0 && (r[1] / 90.0).fract() == 0.0
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
    proxies_against(triangles, triangles, spec)
}

/// Proxies generated from `triangles`, their error measured against the solid of `reference`
/// (the same fill rule at half the voxel size): a damaged bay's proxies see its holes and breach
/// edges but not its pits, spalls and joist pockets, and the measured error then includes how far
/// those surface cuts reach into the proxies.
pub fn proxies_against(triangles: &[[Vec3; 3]], reference: &[[Vec3; 3]], spec: ProxySpec) -> GeneratedProxies {
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
    let reference_triangles = reference;
    let mut reference = VoxelGrid::new(lo, hi, fine);
    reference.mark_surface(reference_triangles);
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
                    let d = reference_triangles
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

// ── Runtime shapes (urban-intro F1 2026-09-28; integrator 2026-09-29) ─────────────────────────
// What the building runtime needs to know about an asset's geometry to derive collision, walking,
// stairs and fixtures from a blueprint without loading a mesh, in the asset's own frame (metres,
// +Y up). It has two sources:
// - measured geometry, generated from the kit's built meshes: the bounds of `load_mesh`, the
//   collision proxies of `proxies` and their measured error. `cargo run -- urban-kit-shapes`
//   writes it into the tracked table `building_kit_shapes.rs`, so a query never loads a mesh;
//   `generated_shapes_match_the_live_generator` rebuilds the table from the installed meshes and
//   fails on any drift. Every asset uses these generated proxies. (Until the integration the
//   featured house's nine assets carried hand-declared boxes instead: a roof and a stair flight
//   as their bounding boxes, a holed slab closing its stair opening, and a doorway whose collision
//   left 0.3 m of each visible brick jamb open because its aperture was taken as 2.2 m where the
//   jambs stand at ±0.8 m.)
// - declared roles (`ROLES`): which assets carry feet on their top face, how a stair's flights and
//   landings climb, and where a fixture's light sits. Each is checked against the built mesh by
//   `declared_roles_lie_on_their_meshes_within_the_declared_error`.
// An asset without generated geometry has no shape, and the runtime refuses to instantiate a
// structural module on it.

/// Axis-aligned box in an asset's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KitBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// A walkable top face (floor and ceiling slabs): its height above the asset origin. The cell the
/// face covers is the shape's `bounds` in XZ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KitFloor {
    pub top_y: f32,
}

/// The walking corridor of a straight stair flight in XZ. Feet climb linearly from the bottom edge
/// to the top edge between the two floors the stair connects; this is the declared analytic tread
/// surface (see `tread_error_m`), not the stepped mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KitStair {
    pub corridor_min: [f32; 2],
    pub corridor_max: [f32; 2],
    /// Unit XZ direction of ascent in the asset frame (one of ±X, ±Z).
    pub ascent: [f32; 2],
    /// Largest vertical gap between the analytic surface and a stepped tread top, measured.
    pub tread_error_m: f32,
}

/// One walking segment of a stair with several flights (a dog-leg: two flights and the landing
/// between them), in the asset frame. Feet on a flight climb linearly along `ascent` across the
/// corridor from `rise[0]` to `rise[1]` of the stair's whole climb (the storeys it joins); a
/// landing (`ascent` zero, `rise[0] == rise[1]`) is flat. The segments tile the stair's walking
/// surface: a flight's top edge meets the landing's edge. The analytic surface departs from the
/// stepped treads by at most `tread_error_m`, measured in the asset frame (the world error scales
/// with the module's vertical scale).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KitFlight {
    pub corridor_min: [f32; 2],
    pub corridor_max: [f32; 2],
    /// Unit XZ direction of ascent (one of ±X, ±Z), or zero for a landing.
    pub ascent: [f32; 2],
    /// Share of the stair's climb at the bottom and at the top edge of the segment.
    pub rise: [f32; 2],
    pub tread_error_m: f32,
}

/// A modelled light fixture: a compact source below the fixture mesh, in the asset frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KitLight {
    pub offset: [f32; 3],
    pub radius_m: f32,
    pub color: [f32; 3],
    pub intensity: f32,
}

/// The declared walking or fixture role of a kit asset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KitRole {
    pub id: &'static str,
    pub floor: Option<KitFloor>,
    pub stair: Option<KitStair>,
    pub flights: &'static [KitFlight],
    pub light: Option<KitLight>,
}

/// Measured geometry of one kit asset, generated into `building_kit_shapes.rs`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeneratedShape {
    pub id: &'static str,
    /// Axis-aligned bounds of the built mesh (`load_mesh`).
    pub bounds: KitBox,
    /// Measured error of the proxies (`KitProxies::error_m`).
    pub solid_error_m: f32,
    /// The generated collision proxies (`proxies`).
    pub solids: &'static [KitBox],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KitShape {
    pub id: &'static str,
    /// Axis-aligned bounds of the built mesh, measured.
    pub bounds: KitBox,
    /// Solid collision, damage and sight proxies, generated from the source triangles. The
    /// runtime gives none to cabinets, fixtures and debris.
    pub solids: &'static [KitBox],
    /// Largest distance by which the solids depart from the visible mesh, measured (see the
    /// module docs); the world error is this times the module's largest scale factor.
    pub solid_error_m: f32,
    pub floor: Option<KitFloor>,
    pub stair: Option<KitStair>,
    pub flights: &'static [KitFlight],
    pub light: Option<KitLight>,
}

const fn kbox(min: [f32; 3], max: [f32; 3]) -> KitBox {
    KitBox { min, max }
}

const NO_ROLE: KitRole = KitRole {
    id: "",
    floor: None,
    stair: None,
    flights: &[],
    light: None,
};

/// Walking segments of the kit's dog-leg stairs (`urban.concrete.stair_dogleg`,
/// `urban.timber.stair_dogleg`): flight A on the +X half rises from the floor landing edge at
/// z = -2.1 to the mid landing at z = 0.9, the landing spans the stair's width to z = 2.1, flight B
/// on the -X half returns to z = -2.1 one storey up (see their `KIT` entries).
const DOGLEG_CONCRETE_FLIGHTS: &[KitFlight] = &[
    KitFlight {
        corridor_min: [0.05, -2.1],
        corridor_max: [1.25, 0.9],
        ascent: [0.0, 1.0],
        rise: [0.0, 0.5],
        tread_error_m: 0.17,
    },
    KitFlight {
        corridor_min: [-1.3, 0.9],
        corridor_max: [1.3, 2.1],
        ascent: [0.0, 0.0],
        rise: [0.5, 0.5],
        tread_error_m: 0.01,
    },
    KitFlight {
        corridor_min: [-1.25, -2.1],
        corridor_max: [-0.05, 0.9],
        ascent: [0.0, -1.0],
        rise: [0.5, 1.0],
        tread_error_m: 0.17,
    },
];
const DOGLEG_TIMBER_FLIGHTS: &[KitFlight] = DOGLEG_CONCRETE_FLIGHTS;

/// Straight Pizza flights (`stairs_5_concrete`, `stairs_5_wood`): ten 0.3 m risers from z = +2.0
/// up to z = -2.0. The corridor is 0.15 m wider than the treads on each side and ends 0.2 m inside
/// the flight at both ends; the linear surface lies up to the measured error below a tread top
/// just past a riser (feet on the stair sink into the visible step by up to that much).
const STRAIGHT_FLIGHT: KitStair = KitStair {
    corridor_min: [-1.15, -1.8],
    corridor_max: [1.15, 1.8],
    ascent: [0.0, -1.0],
    tread_error_m: 0.48,
};

pub const ROLES: &[KitRole] = &[
    KitRole {
        id: "pizza.structure.floor",
        floor: Some(KitFloor { top_y: 0.4 }),
        ..NO_ROLE
    },
    KitRole {
        id: "pizza.structure.floor_hole",
        floor: Some(KitFloor { top_y: 0.4 }),
        ..NO_ROLE
    },
    KitRole {
        id: "pizza.structure.stairs",
        stair: Some(STRAIGHT_FLIGHT),
        ..NO_ROLE
    },
    KitRole {
        id: "pizza.retro_fps.asset.ceiling_lamp_fps_1",
        // The source sits 0.22 m below the ceiling fixture so the room, not the fixture's own
        // housing, receives its light.
        light: Some(KitLight {
            offset: [0.0, -0.22, 0.0],
            radius_m: 4.5,
            color: [1.0, 0.70, 0.44],
            intensity: 6.0,
        }),
        ..NO_ROLE
    },
    KitRole {
        id: "urban.timber.floor",
        floor: Some(KitFloor { top_y: 0.01 }),
        ..NO_ROLE
    },
    KitRole {
        id: "urban.timber.stair_flight",
        stair: Some(STRAIGHT_FLIGHT),
        ..NO_ROLE
    },
    KitRole {
        id: "urban.concrete.stair_dogleg",
        flights: DOGLEG_CONCRETE_FLIGHTS,
        ..NO_ROLE
    },
    KitRole {
        id: "urban.timber.stair_dogleg",
        flights: DOGLEG_TIMBER_FLIGHTS,
        ..NO_ROLE
    },
];

#[path = "building_kit_shapes.rs"]
mod generated;

#[path = "building_kit_damage.rs"]
pub mod damage;

pub fn role(id: &str) -> Option<&'static KitRole> {
    ROLES.iter().find(|role| role.id == id)
}

/// Every kit asset's runtime shape: its generated geometry with its declared role.
pub fn shapes() -> &'static [KitShape] {
    static SHAPES: OnceLock<Vec<KitShape>> = OnceLock::new();
    SHAPES.get_or_init(|| {
        generated::GENERATED
            .iter()
            .map(|geometry| {
                let role = role(geometry.id).copied().unwrap_or(NO_ROLE);
                KitShape {
                    id: geometry.id,
                    bounds: geometry.bounds,
                    solids: geometry.solids,
                    solid_error_m: geometry.solid_error_m,
                    floor: role.floor,
                    stair: role.stair,
                    flights: role.flights,
                    light: role.light,
                }
            })
            .collect()
    })
}

/// The runtime shape of a kit asset; a variant takes its collision entry's (`{base}.v.{damage}`,
/// or the base's for an undamaged variant).
pub fn shape(id: &str) -> Option<&'static KitShape> {
    shapes()
        .iter()
        .find(|shape| shape.id == id)
        .or_else(|| {
            let key = collision_of(id)?;
            shapes().iter().find(|shape| shape.id == key)
        })
        .or_else(|| deposit_shape(id))
}

/// A snow deposit's shape: no solids and no role, its host's bounds raised by the snow's depth
/// (with a margin) so a building's broad-phase box holds it.
fn deposit_shape(id: &str) -> Option<&'static KitShape> {
    static SHAPES: OnceLock<Mutex<HashMap<String, &'static KitShape>>> = OnceLock::new();
    let host = id.strip_prefix(DEPOSIT_PREFIX)?;
    let cache = SHAPES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().expect("deposit shapes").get(id) {
        return Some(found);
    }
    let host_shape = shape(host)?;
    let mut bounds = host_shape.bounds;
    bounds.max[1] += 0.12;
    let entry: &'static KitShape = Box::leak(Box::new(KitShape {
        id: deposit_asset(host)?,
        bounds,
        solids: &[],
        solid_error_m: 0.0,
        floor: None,
        stair: None,
        flights: &[],
        light: None,
    }));
    cache.lock().expect("deposit shapes").insert(id.to_owned(), entry);
    Some(entry)
}

/// Every id the generated shapes table lists, in its order: `KIT`, then the variants' collision
/// entries.
pub fn shape_table_ids() -> Vec<&'static str> {
    KIT.iter()
        .map(|asset| asset.id)
        .chain(registry().collision.iter().map(|(_, _, id)| *id))
        .collect()
}

/// The mesh whose bounds a shape-table entry records: the asset's own, or for a variants'
/// collision entry its visible clean variant (snow on its ledges included; every burn of a damage
/// has the same bounds, since soot is texels and joist pockets lie inside the wall).
fn shape_mesh(id: &str) -> anyhow::Result<CpuMesh> {
    if let Some(&(base, damage, _)) = registry().collision.iter().find(|(_, _, c)| *c == id) {
        let visible = load_mesh(&variant_name(base, damage, Burn::Clean, true))?;
        let mut collision = collision_mesh(base, damage)?;
        collision.min = collision.min.min(visible.min);
        collision.max = collision.max.max(visible.max);
        return Ok(collision);
    }
    load_mesh(id)
}

/// The text of `building_kit_shapes.rs`, generated from the installed meshes: every kit asset's
/// built-mesh bounds, proxies and measured proxy error.
pub fn generated_shapes_source() -> anyhow::Result<String> {
    use std::fmt::Write as _;
    let mut out = String::new();
    writeln!(out, "//! @generated by `cargo run -- urban-kit-shapes` from `building_kit::load_mesh` and")?;
    writeln!(out, "//! `building_kit::proxies`; do not edit. The test")?;
    writeln!(out, "//! `building_kit::tests::generated_shapes_match_the_live_generator` rebuilds it from the")?;
    writeln!(out, "//! installed meshes and fails on any drift.")?;
    writeln!(out, "#![cfg_attr(rustfmt, rustfmt_skip)]")?;
    writeln!(out)?;
    writeln!(out, "use super::{{GeneratedShape, KitBox}};")?;
    writeln!(out)?;
    writeln!(out, "const fn b(min: [f32; 3], max: [f32; 3]) -> KitBox {{")?;
    writeln!(out, "    KitBox {{ min, max }}")?;
    writeln!(out, "}}")?;
    writeln!(out)?;
    writeln!(out, "pub(super) const GENERATED: &[GeneratedShape] = &[")?;
    let triple = |v: [f32; 3]| format!("[{:?}, {:?}, {:?}]", v[0], v[1], v[2]);
    for id in shape_table_ids() {
        let mesh = shape_mesh(id)?;
        let proxies = proxies(id)?;
        writeln!(out, "    GeneratedShape {{")?;
        writeln!(out, "        id: {:?},", id)?;
        writeln!(
            out,
            "        bounds: b({}, {}),",
            triple(mesh.min.to_array()),
            triple(mesh.max.to_array())
        )?;
        writeln!(out, "        solid_error_m: {:?},", proxies.error_m)?;
        if proxies.boxes.is_empty() {
            writeln!(out, "        solids: &[],")?;
        } else {
            writeln!(out, "        solids: &[")?;
            for (min, max) in &proxies.boxes {
                writeln!(out, "            b({}, {}),", triple(min.to_array()), triple(max.to_array()))?;
            }
            writeln!(out, "        ],")?;
        }
        writeln!(out, "    }},")?;
    }
    writeln!(out, "];")?;
    Ok(out)
}

/// Write each asset's built mesh (`load_mesh`, damage and deposits included) for the offline
/// previews: `{outdir}/{prefix}{id}.mesh.json` with positions, normals, UVs, indices and the name
/// of its texture, written once per distinct image as `{prefix}tex_NN.png`. Returns id → mesh file
/// name. The Blender preview builds these meshes as the engine does instead of re-deriving them.
pub fn export_meshes(ids: &[String], outdir: &str, prefix: &str) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    // The texel `Arc`s stay held while the export runs, so a freed image's address can never be
    // taken for another's.
    let mut textures: Vec<(Arc<Vec<u8>>, String)> = Vec::new();
    let mut out = std::collections::BTreeMap::new();
    for id in ids {
        let mesh = load_mesh(id)?;
        let texture = match &mesh.texture {
            Some(image) => {
                let name = match textures.iter().find(|(held, _)| Arc::ptr_eq(held, &image.rgba)) {
                    Some((_, name)) => name.clone(),
                    None => {
                        let name = format!("{prefix}tex_{:02}.png", textures.len());
                        let rgba = image::RgbaImage::from_raw(image.width, image.height, image.rgba.to_vec())
                            .ok_or_else(|| anyhow::anyhow!("texture of {id} has the wrong size"))?;
                        rgba.save(format!("{outdir}/{name}"))?;
                        textures.push((image.rgba.clone(), name.clone()));
                        name
                    }
                };
                serde_json::Value::String(name)
            }
            None => serde_json::Value::Null,
        };
        let flat3 = |f: &dyn Fn(&Vertex) -> [f32; 3]| mesh.vertices.iter().flat_map(|v| f(v)).collect::<Vec<f32>>();
        let json = serde_json::json!({
            "id": id,
            "positions": flat3(&|v| v.pos),
            "normals": flat3(&|v| v.normal),
            "uvs": mesh.vertices.iter().flat_map(|v| v.uv).collect::<Vec<f32>>(),
            "indices": mesh.indices,
            "texture": texture,
        });
        let name = format!("{prefix}{id}.mesh.json");
        std::fs::write(format!("{outdir}/{name}"), serde_json::to_string(&json)?)?;
        out.insert(id.clone(), name);
    }
    Ok(out)
}

/// Write `building_kit_shapes.rs` (the path is the tracked source file unless given).
pub fn write_generated_shapes(path: &str) -> anyhow::Result<()> {
    let source = generated_shapes_source()?;
    std::fs::write(path, &source)?;
    let ids = shape_table_ids();
    let boxes: usize = ids.iter().map(|id| proxies(id).map(|p| p.boxes.len()).unwrap_or(0)).sum();
    println!(
        "[urban-kit-shapes] {path}: {} kit assets and {} variant collision entries ({} variants), {boxes} proxy boxes",
        KIT.len(),
        ids.len() - KIT.len(),
        variant_assets().len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building_blueprint::SemanticId;

    /// The tracked table of generated shapes is exactly what the generator builds from the
    /// installed meshes today, and it covers every kit asset once.
    #[test]
    fn generated_shapes_match_the_live_generator() {
        let ids: Vec<&str> = shapes().iter().map(|shape| shape.id).collect();
        let kit = shape_table_ids();
        assert_eq!(ids, kit, "building_kit_shapes.rs must list every kit asset in KIT order, then the variants' collision entries");
        for shape in shapes() {
            for solid in shape.solids {
                for axis in 0..3 {
                    assert!(solid.min[axis] <= solid.max[axis], "{}: inverted solid", shape.id);
                    assert!(
                        solid.min[axis] >= shape.bounds.min[axis] - shape.solid_error_m - 1e-4
                            && solid.max[axis] <= shape.bounds.max[axis] + shape.solid_error_m + 1e-4,
                        "{}: solid leaves the mesh bounds by more than the measured error",
                        shape.id
                    );
                }
            }
        }
        if !assets_installed() {
            eprintln!("skipping the live comparison: Pizza kit assets are not installed");
            return;
        }
        let live = generated_shapes_source().unwrap();
        let tracked = std::fs::read_to_string("src/building_kit_shapes.rs").unwrap();
        assert!(
            live == tracked,
            "src/building_kit_shapes.rs is stale: regenerate it with `cargo run -- urban-kit-shapes`"
        );
    }

    /// Every declared role belongs to a kit asset and lies on its built mesh: floor tops are the
    /// mesh tops, straight flights and dog-leg segments stay within their declared tread error
    /// of the stepped treads, and a fixture's light sits under its housing.
    #[test]
    fn declared_roles_lie_on_their_meshes_within_the_declared_error() {
        let mut seen = std::collections::BTreeSet::new();
        for role in ROLES {
            assert!(seen.insert(role.id), "duplicate role {}", role.id);
            let shape = shape(role.id).unwrap_or_else(|| panic!("role {} has no kit asset", role.id));
            if let Some(floor) = role.floor {
                assert!((floor.top_y - shape.bounds.max[1]).abs() < 1e-3, "{}: floor top", role.id);
            }
            if let Some(stair) = role.stair {
                let length = (stair.ascent[0].powi(2) + stair.ascent[1].powi(2)).sqrt();
                assert!((length - 1.0).abs() < 1e-6 && (stair.ascent[0] == 0.0 || stair.ascent[1] == 0.0));
                assert!(stair.corridor_min[0] < stair.corridor_max[0] && stair.corridor_min[1] < stair.corridor_max[1]);
            }
            let mut previous_top: Option<f32> = None;
            for flight in role.flights {
                let length = (flight.ascent[0].powi(2) + flight.ascent[1].powi(2)).sqrt();
                if length == 0.0 {
                    assert_eq!(flight.rise[0], flight.rise[1], "{}: a landing is flat", role.id);
                } else {
                    assert!((length - 1.0).abs() < 1e-6 && (flight.ascent[0] == 0.0 || flight.ascent[1] == 0.0));
                    assert!(flight.rise[0] < flight.rise[1], "{}: a flight climbs", role.id);
                }
                if let Some(top) = previous_top {
                    assert_eq!(flight.rise[0], top, "{}: segments join at one height", role.id);
                }
                previous_top = Some(flight.rise[1]);
            }
            if let Some(first) = role.flights.first() {
                assert_eq!(first.rise[0], 0.0, "{}: flights start at the lower floor", role.id);
                assert_eq!(previous_top, Some(1.0), "{}: flights end at the upper floor", role.id);
                assert!(first.rise[0] >= 0.0);
            }
            if !assets_installed() {
                eprintln!("skipping mesh checks: Pizza kit assets are not installed");
                continue;
            }
            let mesh = load_mesh(role.id).unwrap();
            if let Some(stair) = role.stair {
                // The exact top surface of the mesh along the flight's centre line against the linear
                // corridor ramp from the bottom edge (height 0, the lower floor) to the top edge (the
                // mesh top, the upper floor).
                let (lo, hi) = along_range(&stair);
                let mut worst = 0.0_f32;
                for sample in 0..=720 {
                    let s = lo + (hi - lo) * sample as f32 / 720.0;
                    let point = [s * stair.ascent[0], s * stair.ascent[1]];
                    let Some(tread) = top_surface(&mesh, point) else { continue };
                    let ramp = shape.bounds.max[1] * (s - lo) / (hi - lo);
                    worst = worst.max((tread - ramp).abs());
                }
                let top = top_surface(&mesh, [hi * stair.ascent[0], hi * stair.ascent[1]]).unwrap();
                assert!((top - shape.bounds.max[1]).abs() < 1e-3, "{}: stair ascends the wrong way", role.id);
                println!("{}: analytic tread surface departs at most {worst:.3} m from the treads", role.id);
                assert!(worst <= stair.tread_error_m, "{}: tread error {worst}", role.id);
            }
            for (index, flight) in role.flights.iter().enumerate() {
                // Sample the segment's centre line (a landing: its whole corridor) against the mesh's
                // exact top surface; the climb is the mesh height (the stair's bottom is at y = 0).
                let climb = shape.bounds.max[1];
                let axis = glam::Vec2::from_array(flight.ascent);
                let (min, max) = (glam::Vec2::from_array(flight.corridor_min), glam::Vec2::from_array(flight.corridor_max));
                let centre = (min + max) * 0.5;
                let mut worst = 0.0_f32;
                let mut sampled = 0;
                for i in 0..=48 {
                    for j in 0..=(if axis == glam::Vec2::ZERO { 12 } else { 0 }) {
                        let u = i as f32 / 48.0;
                        let v = j as f32 / 12.0;
                        let point = if axis == glam::Vec2::ZERO {
                            glam::Vec2::new(min.x + (max.x - min.x) * u, min.y + (max.y - min.y) * v)
                        } else if axis.x != 0.0 {
                            glam::Vec2::new(min.x + (max.x - min.x) * u, centre.y)
                        } else {
                            glam::Vec2::new(centre.x, min.y + (max.y - min.y) * u)
                        };
                        let Some(tread) = top_surface(&mesh, point.to_array()) else { continue };
                        let t = if axis == glam::Vec2::ZERO {
                            0.0
                        } else {
                            let along = [min.dot(axis), max.dot(axis)];
                            let (lo, hi) = (along[0].min(along[1]), along[0].max(along[1]));
                            ((point.dot(axis) - lo) / (hi - lo)).clamp(0.0, 1.0)
                        };
                        let analytic = climb * (flight.rise[0] + t * (flight.rise[1] - flight.rise[0]));
                        worst = worst.max((tread - analytic).abs());
                        sampled += 1;
                    }
                }
                assert!(sampled > 20, "{} segment {index}: the corridor misses the mesh", role.id);
                println!("{} segment {index}: analytic surface departs at most {worst:.3} m", role.id);
                assert!(worst <= flight.tread_error_m, "{} segment {index}: error {worst}", role.id);
            }
            if let Some(light) = role.light {
                assert!(light.offset[1] < shape.bounds.min[1], "{}: the light hangs under its housing", role.id);
            }
        }
    }

    /// Range of the ascent coordinate over the stair corridor: (bottom edge, top edge).
    fn along_range(stair: &KitStair) -> (f32, f32) {
        let corners = [
            [stair.corridor_min[0], stair.corridor_min[1]],
            [stair.corridor_max[0], stair.corridor_max[1]],
        ];
        let along: Vec<f32> = corners
            .iter()
            .map(|c| c[0] * stair.ascent[0] + c[1] * stair.ascent[1])
            .collect();
        (along[0].min(along[1]), along[0].max(along[1]))
    }

    /// Highest point of the mesh above the XZ point: the largest height of any non-vertical
    /// triangle whose XZ projection contains it.
    fn top_surface(mesh: &crate::gltf_load::CpuMesh, point: [f32; 2]) -> Option<f32> {
        let mut best: Option<f32> = None;
        for tri in mesh.indices.chunks_exact(3) {
            let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| mesh.vertices[i as usize].pos);
            let (ax, az, bx, bz, cx, cz) = (a[0], a[2], b[0], b[2], c[0], c[2]);
            let det = (bz - cz) * (ax - cx) + (cx - bx) * (az - cz);
            if det.abs() < 1e-9 {
                continue;
            }
            let l1 = ((bz - cz) * (point[0] - cx) + (cx - bx) * (point[1] - cz)) / det;
            let l2 = ((cz - az) * (point[0] - cx) + (ax - cx) * (point[1] - cz)) / det;
            let l3 = 1.0 - l1 - l2;
            if l1 < -1e-5 || l2 < -1e-5 || l3 < -1e-5 {
                continue;
            }
            let y = l1 * a[1] + l2 * b[1] + l3 * c[1];
            best = Some(best.map_or(y, |current: f32| current.max(y)));
        }
        best
    }

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
                // Parts turned only by quarter turns about Y keep their source-space proxies
                // exact; the one oblique part (the hanging slab) is voxelised where it lies
                // (`proxies`), and must be a single-part asset so nothing else shares its boxes.
                if !part_quarter_turn(part) {
                    assert_eq!(asset.id, "urban.concrete.slab_hanging", "{}: oblique part", asset.id);
                    assert_eq!(asset.parts.len(), 1, "{}: an oblique part stands alone", asset.id);
                }
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
    fn clipping_keeps_the_texture_scale_and_cuts_exactly_on_the_plane() {
        // A unit quad in XY with UV = XY, cut to x + y <= 1: half its area remains, every vertex
        // lies inside, and UVs still equal positions (the texture is not rescaled).
        let v = |x: f32, y: f32| Vertex {
            pos: [x, y, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [x, y],
            tangent: [1.0, 0.0, 0.0, 1.0],
        };
        let mesh = CpuMesh::basic(
            vec![v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)],
            vec![0, 1, 2, 0, 2, 3],
            None,
            Vec3::ZERO,
            Vec3::new(1.0, 1.0, 0.0),
        );
        let cut = clip_mesh(mesh, &[[1.0, 1.0, 0.0, 1.0]]);
        let area: f32 = mesh_triangles(&cut)
            .iter()
            .map(|t| (t[1] - t[0]).cross(t[2] - t[0]).length() * 0.5)
            .sum();
        assert!((area - 0.5).abs() < 1e-5, "area {area}");
        for vertex in &cut.vertices {
            assert!(vertex.pos[0] + vertex.pos[1] <= 1.0 + 1e-5, "{:?}", vertex.pos);
            assert!((vertex.uv[0] - vertex.pos[0]).abs() < 1e-6 && (vertex.uv[1] - vertex.pos[1]).abs() < 1e-6);
        }
        assert!((cut.max - Vec3::new(1.0, 1.0, 0.0)).abs().max_element() < 1e-6);
        // Two planes leave the gable triangle under (0, 1) of the quad's top edge.
        let quad = CpuMesh::basic(
            vec![v(-1.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(-1.0, 1.0)],
            vec![0, 1, 2, 0, 2, 3],
            None,
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        );
        let gable = clip_mesh(quad, &[[1.0, 1.0, 0.0, 1.0], [-1.0, 1.0, 0.0, 1.0]]);
        let area: f32 = mesh_triangles(&gable)
            .iter()
            .map(|t| (t[1] - t[0]).cross(t[2] - t[0]).length() * 0.5)
            .sum();
        assert!((area - 1.0).abs() < 1e-5, "gable area {area}");
        // A slab of two faces 0.2 m apart, each split along a different diagonal, cut at
        // y <= 0.5: the cut is closed by a 2.0 × 0.2 m cap facing +Y.
        let slab = CpuMesh::basic(
            vec![
                v(-1.0, 0.0),
                v(1.0, 0.0),
                v(1.0, 1.0),
                v(-1.0, 1.0),
                Vertex { pos: [-1.0, 0.0, -0.2], ..v(-1.0, 0.0) },
                Vertex { pos: [1.0, 0.0, -0.2], ..v(1.0, 0.0) },
                Vertex { pos: [1.0, 1.0, -0.2], ..v(1.0, 1.0) },
                Vertex { pos: [-1.0, 1.0, -0.2], ..v(-1.0, 1.0) },
            ],
            vec![0, 1, 2, 0, 2, 3, 4, 7, 5, 5, 7, 6],
            None,
            Vec3::new(-1.0, 0.0, -0.2),
            Vec3::new(1.0, 1.0, 0.0),
        );
        let cut = clip_mesh(slab, &[[0.0, 2.0, 0.0, 1.0]]);
        let cap: Vec<[Vec3; 3]> = mesh_triangles(&cut)
            .into_iter()
            .filter(|t| (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero().dot(Vec3::Y) > 0.999)
            .collect();
        let cap_area: f32 = cap.iter().map(|t| (t[1] - t[0]).cross(t[2] - t[0]).length() * 0.5).sum();
        assert!((cap_area - 0.4).abs() < 1e-5, "cap area {cap_area}");
        assert!(cap.iter().flatten().all(|p| (p.y - 0.5).abs() < 1e-5));
        assert!((cut.max.y - 0.5).abs() < 1e-5);
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

    /// Every registered variant builds from its base with its damage and deposits: its bounds are
    /// its shape's (the runtime refuses any other), every variant of a base shares one atlas image,
    /// its UVs point into the atlas copy its deposit selects, and snow lies on window sills.
    #[test]
    fn every_variant_builds_with_its_shape_bounds_shared_atlas_and_deposits() {
        use damage::Region;
        if !assets_installed() {
            eprintln!("skipping: Pizza kit assets are not installed");
            return;
        }
        let mut atlases: HashMap<&str, Arc<Vec<u8>>> = HashMap::new();
        for (asset, v) in registry().assets.iter().zip(&registry().variants) {
            let mesh = load_mesh(asset.id).unwrap_or_else(|e| panic!("{}: {e}", asset.id));
            let shape = shape(asset.id).unwrap_or_else(|| panic!("{} has no shape", asset.id));
            assert!(
                mesh.min.distance(Vec3::from(shape.bounds.min)) <= 1e-3 && mesh.max.distance(Vec3::from(shape.bounds.max)) <= 1e-3,
                "{}: bounds {:?}..{:?} vs shape {:?}",
                asset.id,
                mesh.min,
                mesh.max,
                shape.bounds
            );
            let texture = mesh.texture.as_ref().expect("variant texture");
            let shared = atlases.entry(v.base).or_insert_with(|| texture.rgba.clone());
            assert!(Arc::ptr_eq(shared, &texture.rgba), "{}: its own texture copy", asset.id);
            let base = load_mesh(v.base).unwrap();
            let atlas = damage::atlas(v.base, base.texture.as_ref().unwrap());
            let (mut snow, mut heavy, mut light) = (0.0_f32, 0.0_f32, 0.0_f32);
            for tri in mesh.indices.chunks_exact(3) {
                let vs: [&Vertex; 3] = std::array::from_fn(|i| &mesh.vertices[tri[i] as usize]);
                let regions: Vec<Option<Region>> = vs.iter().map(|x| atlas.region_of_v(x.uv[1])).collect();
                assert!(regions.iter().all(|r| r.is_some() && *r == regions[0]), "{}: a triangle straddles atlas regions", asset.id);
                let p: [Vec3; 3] = std::array::from_fn(|i| Vec3::from_array(vs[i].pos));
                let area = (p[1] - p[0]).cross(p[2] - p[0]).length() * 0.5;
                match regions[0].unwrap() {
                    Region::Snow => {
                        snow += area;
                        if base_of(asset.id).contains("window") && !matches!(v.damage, Damage::TopA | Damage::TopB) {
                            assert!(p.iter().all(|q| q.y < 2.95), "{}: snow above the sill zone", asset.id);
                        }
                    }
                    Region::SootHeavy => heavy += area,
                    Region::SootLight => light += area,
                    Region::Clean => {}
                }
            }
            match v.burn {
                Burn::Clean => assert!(heavy == 0.0 && light == 0.0, "{}: soot on a clean bay", asset.id),
                Burn::Sooted | Burn::Burnt => assert!(heavy > 1.0 && light > 1.0, "{}: {heavy} heavy, {light} light", asset.id),
                Burn::SootedBoth => assert!(heavy > 2.0 && light == 0.0, "{}: {heavy} heavy, {light} light", asset.id),
            }
            if base_of(asset.id).contains("window") {
                assert!(snow > 0.05, "{}: no snow on the sill ({snow} m² of snow faces)", asset.id);
            }
        }
    }

    /// A variant's collision sees its holes and breach edges and none of its pits, spalls and
    /// joist pockets, within the recorded bound: a ray through a hole's middle meets no proxy.
    #[test]
    fn variant_collision_sees_holes_and_edges_but_keeps_pits_within_its_bound() {
        if !assets_installed() {
            eprintln!("skipping: Pizza kit assets are not installed");
            return;
        }
        for &(base, damage, id) in &registry().collision {
            let p = proxies(id).unwrap();
            let base_proxies = proxies(base).unwrap();
            assert!(p.error_m <= p.bound_m + 1e-4, "{id}: error {} over bound {}", p.error_m, p.bound_m);
            assert!(p.bound_m <= base_proxies.bound_m + VARIANT_SURFACE_ERROR_M + 1e-4, "{id}: bound");
            let cuts = damage_cuts_for(base, damage, true).unwrap();
            if damage == Damage::Pitted {
                assert_eq!(p.boxes, base_proxies.boxes, "{id}: pits reached the collision");
            }
            for cut in cuts.iter().filter(|c| c.structural) {
                // The centroid of a structural cut is open to a ray along Z.
                let c = cut.polygon.iter().fold(glam::Vec2::ZERO, |a, q| a + *q) / cut.polygon.len() as f32;
                if c.x.abs() >= 1.95 || c.y <= 0.05 || c.y >= 2.95 {
                    continue;
                }
                let blocked = p.boxes.iter().any(|(min, max)| c.x > min.x && c.x < max.x && c.y > min.y && c.y < max.y);
                assert!(!blocked, "{id}: a proxy fills the cut at {c:?}");
            }
            assert!(p.volume_m3 < base_proxies.volume_m3 + 1e-4 || damage == Damage::Pitted, "{id}: volume grew");
        }
    }

    /// The snow on a top lies on the host's topmost surface, in the district's snow texel shared by
    /// every deposit.
    #[test]
    fn deposits_lie_on_their_hosts_tops() {
        if !assets_installed() {
            eprintln!("skipping: Pizza kit assets are not installed");
            return;
        }
        let mut texel: Option<Arc<Vec<u8>>> = None;
        for host in DEPOSIT_HOSTS {
            let id = deposit_asset(host).unwrap();
            let mesh = load_mesh(id).unwrap_or_else(|e| panic!("{id}: {e}"));
            let host_mesh = load_mesh(host).unwrap();
            let t = mesh.texture.as_ref().unwrap();
            assert_eq!((t.width, t.height), (1, 1), "{id}");
            let shared = texel.get_or_insert_with(|| t.rgba.clone());
            assert!(Arc::ptr_eq(shared, &t.rgba), "{id}: its own snow texel");
            let slope = host.contains("gable");
            for v in &mesh.vertices {
                if !slope {
                    assert!(v.pos[1] >= host_mesh.max.y - 0.13 && v.pos[1] <= host_mesh.max.y + 0.08, "{id}: snow at {:?} off the top {}", v.pos, host_mesh.max.y);
                }
                assert!(v.pos[0] >= host_mesh.min.x - 1e-3 && v.pos[0] <= host_mesh.max.x + 1e-3, "{id}: snow beside its host");
            }
            let shape = shape(id).unwrap();
            assert!(shape.solids.is_empty() && shape.floor.is_none(), "{id}: a deposit is not solid");
            assert!(mesh.max.cmple(Vec3::from(shape.bounds.max) + Vec3::splat(1e-3)).all(), "{id}: outside its shape");
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
