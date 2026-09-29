//! Building kit: the registry from a blueprint module's semantic asset id to its source mesh and
//! material, shared by every generated building.
//!
//! Owned by the building grammar of the urban-intro team (docs/URBAN_INTRO_TEAM_20260928.md). The
//! first nine entries are the Pizza modules of the featured house. This is the only asset registry
//! of the building runtime: blueprints name semantic asset ids, and every source path and material
//! is resolved here. A new entry needs a runtime-catalog
//! review (docs/RUNTIME_ASSET_CATALOG.md), and its collision proxies are generated from its source
//! geometry with a measured error bound (CLAUDE.md, second principle, rule 3).
#![allow(dead_code)] // wired by the runtime builder of the urban-intro team

/// Material class of a kit module, consumed by destruction (crush tier, shell damage) and sound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KitMaterial {
    Brick,
    Stone,
    Timber,
    Concrete,
    Metal,
}

#[derive(Clone, Copy, Debug)]
pub struct KitAsset {
    /// Semantic asset id used by `ModuleDef::asset_id`.
    pub id: &'static str,
    /// Source mesh, relative to the repository root.
    pub path: &'static str,
    pub material: KitMaterial,
}

pub const KIT: &[KitAsset] = &[
    KitAsset {
        id: "pizza.structure.wall",
        path: concat!(
            "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/",
            "Modular Structures/wall_hr_1.glb"
        ),
        material: KitMaterial::Brick,
    },
    KitAsset {
        id: "pizza.structure.window_frame",
        path: concat!(
            "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/",
            "Modular Structures/window_1_single_frame_only.glb"
        ),
        material: KitMaterial::Timber,
    },
    KitAsset {
        id: "pizza.structure.doorway",
        path: concat!(
            "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/",
            "Modular Structures/doorway_hr_2_wide.glb"
        ),
        material: KitMaterial::Brick,
    },
    KitAsset {
        id: "pizza.structure.floor",
        path: concat!(
            "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/",
            "Modular Structures/floor_ceiling_hr_1.glb"
        ),
        material: KitMaterial::Concrete,
    },
    KitAsset {
        id: "pizza.structure.floor_hole",
        path: concat!(
            "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/",
            "Modular Structures/floor_ceiling_hr_2_hole.glb"
        ),
        material: KitMaterial::Concrete,
    },
    KitAsset {
        id: "pizza.structure.roof",
        path: concat!(
            "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/",
            "Modular Structures/roof_mh_4_a_1.glb"
        ),
        material: KitMaterial::Metal,
    },
    KitAsset {
        id: "pizza.structure.stairs",
        path: concat!(
            "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/",
            "Modular Structures/stairs_5_concrete.glb"
        ),
        material: KitMaterial::Concrete,
    },
    KitAsset {
        id: "pizza.props.shelf",
        path: concat!(
            "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/",
            "Props/shelf_mx_7.glb"
        ),
        material: KitMaterial::Timber,
    },
    KitAsset {
        id: "pizza.retro_fps.asset.ceiling_lamp_fps_1",
        path: concat!(
            "assets/pizza/_extracted/Modular Retro FPS Kit v1.5/Modular Retro FPS Kit/Models/other-formats/GLB/",
            "ceiling_lamp_fps_1.glb"
        ),
        material: KitMaterial::Metal,
    },
];

pub fn asset(id: &str) -> Option<&'static KitAsset> {
    KIT.iter().find(|asset| asset.id == id)
}

pub fn asset_path(id: &str) -> Option<&'static str> {
    asset(id).map(|asset| asset.path)
}

// ── Runtime shapes (urban-intro F1, 2026-09-28) ─────────────────────────────────────────────────
// What the building runtime needs to know about an asset's geometry to derive collision, walking,
// stairs and fixtures from a blueprint without loading the mesh: every value is in the asset's
// source frame (metres, +Y up) and is checked against the source GLB by
// `runtime_shapes_match_their_source_meshes_within_the_declared_error` below. The table is separate
// from `KIT` so that new `KitAsset` entries keep compiling; an asset without a shape has no proxy,
// walk surface, stair or light, and the runtime refuses to instantiate a structural module on it.

/// Axis-aligned box in an asset's source frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KitBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// A walkable top face (floor and ceiling slabs): its height above the source origin. The cell the
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
    /// Unit XZ direction of ascent in the source frame (one of ±X, ±Z).
    pub ascent: [f32; 2],
    /// Largest vertical gap between the analytic surface and a stepped tread top, measured.
    pub tread_error_m: f32,
}

/// A modelled light fixture: a compact source below the fixture mesh, in the source frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KitLight {
    pub offset: [f32; 3],
    pub radius_m: f32,
    pub color: [f32; 3],
    pub intensity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KitShape {
    pub id: &'static str,
    /// Axis-aligned bounds of the source mesh, measured.
    pub bounds: KitBox,
    /// Solid collision, damage and sight proxies. Empty for props and fixtures.
    pub solids: &'static [KitBox],
    /// Largest distance by which a solid departs from the visible mesh, measured and named: the
    /// proxies below are axis-aligned boxes, so a gable roof or a stair flight also encloses air.
    pub solid_error_m: f32,
    pub floor: Option<KitFloor>,
    pub stair: Option<KitStair>,
    pub light: Option<KitLight>,
}

const fn kbox(min: [f32; 3], max: [f32; 3]) -> KitBox {
    KitBox { min, max }
}

pub const SHAPES: &[KitShape] = &[
    KitShape {
        id: "pizza.structure.wall",
        bounds: kbox([-2.0, 0.0, -0.2], [2.0, 3.0, 0.2]),
        // One slab 0.42 m deep so that walls and the 0.42 m doorway frames share a face.
        solids: &[kbox([-2.0, 0.0, -0.21], [2.0, 3.0, 0.21])],
        solid_error_m: 0.01,
        floor: None,
        stair: None,
        light: None,
    },
    KitShape {
        id: "pizza.structure.window_frame",
        bounds: kbox([-2.03, 0.0, -0.1], [2.03, 3.0, 0.1]),
        // Two piers, a sill and a lintel around the clear aperture x=(-0.56, 0.56), y=(0.90, 2.50);
        // the frame carries no glass. The strips are 0.42 m deep like the walls they continue,
        // 0.11 m deeper than the 0.20 m source frame on each face.
        solids: &[
            kbox([-2.03, 0.0, -0.21], [-0.56, 3.0, 0.21]),
            kbox([0.56, 0.0, -0.21], [2.03, 3.0, 0.21]),
            kbox([-0.56, 0.0, -0.21], [0.56, 0.90, 0.21]),
            kbox([-0.56, 2.50, -0.21], [0.56, 3.0, 0.21]),
        ],
        solid_error_m: 0.11,
        floor: None,
        stair: None,
        light: None,
    },
    KitShape {
        id: "pizza.structure.doorway",
        bounds: kbox([-2.0, 0.0, -0.21], [2.0, 3.0, 0.21]),
        // Two jambs and a lintel around the clear 2.2 × 2.5 m aperture; the leaves are separate
        // physical world doors.
        solids: &[
            kbox([-2.0, 0.0, -0.23], [-1.1, 2.5, 0.23]),
            kbox([1.1, 0.0, -0.23], [2.0, 2.5, 0.23]),
            kbox([-2.0, 2.5, -0.23], [2.0, 3.0, 0.23]),
        ],
        solid_error_m: 0.02,
        floor: None,
        stair: None,
        light: None,
    },
    KitShape {
        id: "pizza.structure.floor",
        bounds: kbox([-2.0, 0.0, -2.0], [2.0, 0.4, 2.0]),
        solids: &[kbox([-2.0, 0.0, -2.0], [2.0, 0.4, 2.0])],
        solid_error_m: 0.0,
        floor: Some(KitFloor { top_y: 0.4 }),
        stair: None,
        light: None,
    },
    KitShape {
        id: "pizza.structure.floor_hole",
        bounds: kbox([-2.0, 0.0, -2.0], [2.0, 0.4, 2.0]),
        // The box spans the stair opening of the source slab (about 1.9 × 2.7 m), so for shells
        // and sight the opening is closed by this proxy; walking uses the stair corridor instead.
        solids: &[kbox([-2.0, 0.0, -2.0], [2.0, 0.4, 2.0])],
        solid_error_m: 2.0,
        floor: Some(KitFloor { top_y: 0.4 }),
        stair: None,
        light: None,
    },
    KitShape {
        id: "pizza.structure.roof",
        bounds: kbox([-4.0, 0.0, -2.0], [4.0, 2.0, 2.0]),
        // The bounding box of a gable: it encloses the air above both slopes up to the ridge.
        solids: &[kbox([-4.0, 0.0, -2.0], [4.0, 2.0, 2.0])],
        solid_error_m: 2.0,
        floor: None,
        stair: None,
        light: None,
    },
    KitShape {
        id: "pizza.structure.stairs",
        bounds: kbox([-1.0, -0.2, -2.0], [1.0, 3.0, 2.0766]),
        // The bounding box of the flight: it encloses the air above the treads.
        solids: &[kbox([-1.0, -0.2, -2.0], [1.0, 3.0, 2.0766])],
        solid_error_m: 3.0,
        floor: None,
        stair: Some(KitStair {
            // 0.15 m wider than the treads on each side; the corridor ends 0.2 m inside the flight
            // at the bottom and 0.2 m at the top landing (source z = +2.0 … -2.0). The linear
            // surface over ten 0.3 m risers lies up to 0.471 m (measured) below a tread top just
            // past a riser: feet on the stair sink into the visible step by up to that much.
            corridor_min: [-1.15, -1.8],
            corridor_max: [1.15, 1.8],
            ascent: [0.0, -1.0],
            tread_error_m: 0.48,
        }),
        light: None,
    },
    KitShape {
        id: "pizza.props.shelf",
        bounds: kbox([-0.31, 0.0, -0.21], [0.31, 1.81, 0.21]),
        solids: &[],
        solid_error_m: 0.0,
        floor: None,
        stair: None,
        light: None,
    },
    KitShape {
        id: "pizza.retro_fps.asset.ceiling_lamp_fps_1",
        bounds: kbox([-0.4, -0.09, -0.4], [0.4, 0.0, 0.4]),
        solids: &[],
        solid_error_m: 0.0,
        floor: None,
        stair: None,
        // The source sits 0.22 m below the ceiling fixture so the room, not the fixture's own
        // housing, receives its light.
        light: Some(KitLight {
            offset: [0.0, -0.22, 0.0],
            radius_m: 4.5,
            color: [1.0, 0.70, 0.44],
            intensity: 6.0,
        }),
    },
];

pub fn shape(id: &str) -> Option<&'static KitShape> {
    SHAPES.iter().find(|shape| shape.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building_blueprint::SemanticId;

    /// Every runtime shape belongs to a kit asset, its bounds are the measured source AABB, its
    /// solids stay within the declared error of the visible mesh, walkable tops lie on the mesh
    /// and stair corridors lie over the flight.
    #[test]
    fn runtime_shapes_match_their_source_meshes_within_the_declared_error() {
        let mut seen = std::collections::BTreeSet::new();
        for shape in SHAPES {
            assert!(seen.insert(shape.id), "duplicate shape {}", shape.id);
            let asset = asset(shape.id).unwrap_or_else(|| panic!("shape {} has no kit asset", shape.id));
            for solid in shape.solids {
                for axis in 0..3 {
                    assert!(solid.min[axis] < solid.max[axis], "{}: empty solid", shape.id);
                    assert!(
                        solid.min[axis] >= shape.bounds.min[axis] - shape.solid_error_m - 1e-6
                            && solid.max[axis] <= shape.bounds.max[axis] + shape.solid_error_m + 1e-6,
                        "{}: solid leaves the mesh bounds by more than the declared error",
                        shape.id
                    );
                }
            }
            if let Some(floor) = shape.floor {
                assert_eq!(floor.top_y, shape.bounds.max[1], "{}: floor top", shape.id);
            }
            if let Some(stair) = shape.stair {
                let length = (stair.ascent[0].powi(2) + stair.ascent[1].powi(2)).sqrt();
                assert!((length - 1.0).abs() < 1e-6 && (stair.ascent[0] == 0.0 || stair.ascent[1] == 0.0));
                assert!(stair.corridor_min[0] < stair.corridor_max[0] && stair.corridor_min[1] < stair.corridor_max[1]);
            }
            if !std::path::Path::new(asset.path).is_file() {
                eprintln!("skipping mesh check: {} is not installed", asset.path);
                continue;
            }
            let mesh = crate::gltf_load::load_glb(asset.path).unwrap();
            for axis in 0..3 {
                assert!(
                    (mesh.min[axis] - shape.bounds.min[axis]).abs() < 1.0e-3
                        && (mesh.max[axis] - shape.bounds.max[axis]).abs() < 1.0e-3,
                    "{}: declared bounds {:?} differ from the mesh {:?}..{:?}",
                    shape.id,
                    shape.bounds,
                    mesh.min,
                    mesh.max
                );
            }
            // The visible mesh reaches every face of the union of solids within the error bound.
            if !shape.solids.is_empty() {
                let mut union_min = [f32::INFINITY; 3];
                let mut union_max = [f32::NEG_INFINITY; 3];
                for solid in shape.solids {
                    for axis in 0..3 {
                        union_min[axis] = union_min[axis].min(solid.min[axis]);
                        union_max[axis] = union_max[axis].max(solid.max[axis]);
                    }
                }
                for axis in 0..3 {
                    assert!(
                        (union_min[axis] - mesh.min[axis]).abs() <= shape.solid_error_m + 1e-3
                            && (union_max[axis] - mesh.max[axis]).abs()
                                <= shape.solid_error_m + 1e-3,
                        "{}: solids depart from the mesh by more than {} m on axis {axis}",
                        shape.id,
                        shape.solid_error_m
                    );
                }
            }
            if let Some(stair) = shape.stair {
                // Measure the declared tread error along the flight's centre line: the exact top
                // surface of the mesh (highest triangle over the sample point) against the linear
                // corridor ramp from the bottom edge (height 0, the lower floor) to the top edge
                // (the mesh top, the upper floor).
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
                assert!((top - shape.bounds.max[1]).abs() < 1e-3, "{}: stair ascends the wrong way", shape.id);
                println!("{}: analytic tread surface departs at most {worst:.3} m from the treads", shape.id);
                assert!(
                    worst <= stair.tread_error_m,
                    "{}: analytic tread surface departs {worst} m from the treads",
                    shape.id
                );
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

    #[test]
    fn kit_ids_are_unique_semantic_ids_and_every_source_mesh_is_on_disk() {
        let mut seen = std::collections::BTreeSet::new();
        for asset in KIT {
            SemanticId::new(asset.id).expect("kit id is a semantic id");
            assert!(seen.insert(asset.id), "duplicate kit id {}", asset.id);
            assert!(
                std::path::Path::new(asset.path).is_file(),
                "kit mesh missing: {}",
                asset.path
            );
        }
        assert_eq!(asset_path("pizza.structure.wall"), Some(KIT[0].path));
        assert!(asset("pizza.structure.nonexistent").is_none());
    }
}
