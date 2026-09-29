//! Building kit: the registry from a blueprint module's semantic asset id to its source mesh and
//! material, shared by every generated building.
//!
//! Owned by the building grammar of the urban-intro team (docs/URBAN_INTRO_TEAM_20260928.md). The
//! first nine entries are the Pizza modules of the featured house; `building_runtime` keeps a
//! private copy of them until the runtime reads this registry. A new entry needs a runtime-catalog
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building_blueprint::SemanticId;

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
