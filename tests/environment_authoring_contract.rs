//! Focused, renderer-free contract tests for editor environmental authoring.
//!
//! The shipping binary is intentionally not imported: this keeps deterministic field/scatter and
//! JSON tests runnable even while another worktree is changing unrelated gameplay test symbols.

mod assetlib {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum AssetKind {
        Model,
        SemanticModel,
    }

    impl AssetKind {
        pub const fn is_model(self) -> bool {
            matches!(self, Self::Model | Self::SemanticModel)
        }
    }

    #[derive(Clone, Debug)]
    pub struct AssetEntry {
        pub path: String,
        pub search_key: String,
        pub kind: AssetKind,
    }

    impl AssetEntry {
        pub fn model(path: &str, search_key: &str) -> Self {
            Self {
                path: path.to_owned(),
                search_key: search_key.to_ascii_lowercase(),
                kind: AssetKind::Model,
            }
        }

        pub fn story(path: &str, search_key: &str) -> Self {
            Self {
                path: path.to_owned(),
                search_key: search_key.to_ascii_lowercase(),
                kind: AssetKind::SemanticModel,
            }
        }
    }
}

#[path = "../src/environment_authoring.rs"]
mod environment_authoring;

fn reviewed_scene_library() -> Vec<assetlib::AssetEntry> {
    use assetlib::AssetEntry;

    [
        "pillar_hr_2",
        "wires_hr_1",
        "wires_holder_hr_small_1",
        "lamp_mx_1_a_on",
        "wall_hr_1",
        "doorway_hr_1",
        "fence_b_1",
        "garage_hl_1",
        "bed_1",
        "table_large_2",
        "chair_mp_1",
        "wardrobe_mp_1",
        "supply_crate_1",
        "ceiling_lamp_1_on",
        "handheld_fm_radio_etx_1",
        "first_aid_kit_hr_1",
        "jerrycan_mx_1",
        "debris_bricks_mx_1",
        "wooden_board_2",
        "tire_1",
    ]
    .into_iter()
    .map(|stem| AssetEntry::model(&format!("assets/test/{stem}.glb"), stem))
    .chain(
        ["deadman_prone", "t34_85", "pz3"]
            .into_iter()
            .map(|stem| AssetEntry::story(&format!("assets/test/{stem}.glb"), stem)),
    )
    .collect()
}

#[test]
fn built_in_ecology_and_aftermath_recipes_resolve_semantic_asset_mixes() {
    use assetlib::AssetEntry;

    let library = vec![
        AssetEntry::model("nature/pine_tree_01.glb", "pine tree nature"),
        AssetEntry::model("nature/tree_stump_01.glb", "tree stump"),
        AssetEntry::model("nature/rock_01.glb", "rock stone"),
        AssetEntry::model("nature/grass_01.glb", "grass"),
        AssetEntry::model("props/rubble_01.glb", "debris rubble brick"),
        AssetEntry::model("props/crate_01.glb", "crate"),
        AssetEntry::story("story/deadman_prone.glb", "fallen soldier bodies / remains"),
        AssetEntry::story("story/t34_wreck.glb", "burned t34 vehicle wrecks"),
    ];

    let forest = environment_authoring::recipe_from_browser_preset(
        "editor:brush/forest_mixed",
        &library,
        17,
    )
    .unwrap();
    assert_eq!(forest.role, "ecology");
    assert!(forest
        .assets
        .iter()
        .any(|asset| asset.path.contains("pine")));
    assert!(forest
        .assets
        .iter()
        .any(|asset| asset.path.contains("rock")));

    let aftermath =
        environment_authoring::recipe_from_browser_preset("editor:brush/aftermath", &library, 17)
            .unwrap();
    assert_eq!(aftermath.role, "destruction");
    assert!(aftermath
        .assets
        .iter()
        .any(|asset| asset.path.contains("deadman")));
    assert!(aftermath
        .assets
        .iter()
        .any(|asset| asset.path.contains("wreck")));
}

#[test]
fn surface_zones_roundtrip_and_reject_unsafe_or_excessive_layers() {
    use environment_authoring::{SurfaceLayer, SurfaceMaterialZone};
    use glam::Vec3;

    let zone = SurfaceMaterialZone::preset(
        "mud_001".into(),
        "material_mix",
        Vec3::new(2.0, 0.1, -4.0),
        Some("assets/terrain/act1_ground.png".into()),
    );
    zone.validate().unwrap();
    let edits = serde_json::json!({"surface_material_zones": [zone.to_json()]});
    assert_eq!(
        environment_authoring::surface_zones_from_edits(Some(&edits)).unwrap(),
        vec![zone.clone()]
    );

    let duplicate = serde_json::json!({"surface_material_zones": [zone.to_json(), zone.to_json()]});
    assert!(environment_authoring::surface_zones_from_edits(Some(&duplicate)).is_err());
    let overflow = vec![zone.clone(); environment_authoring::MAX_SURFACE_ZONES + 1];
    assert!(environment_authoring::validate_surface_zones(&overflow)
        .unwrap_err()
        .to_string()
        .contains("exceeds"));

    let mut unsafe_zone = zone.clone();
    unsafe_zone.layers[0].texture = Some("../outside.png".into());
    assert!(unsafe_zone.validate().is_err());

    let mut too_many = zone;
    too_many.layers = (0..5)
        .map(|slot| SurfaceLayer {
            texture: Some(format!("assets/terrain/layer_{slot}.png")),
            weight: 1.0,
        })
        .collect();
    assert!(too_many.validate().is_err());
}

#[test]
fn reviewed_scene_recipes_are_deterministic_and_complete() {
    let library = reviewed_scene_library();
    for preset in [
        "editor:scene/lit_utility_road",
        "editor:scene/forward_base",
        "editor:scene/destroyed_convoy",
        "editor:scene/artillery_position",
    ] {
        let plan =
            environment_authoring::scene_recipe_from_browser_preset(preset, &library, 71).unwrap();
        plan.validate().unwrap();
        assert!(!plan.placements.is_empty());
        assert!(!plan.resolved_assets().is_empty());
    }

    let mut reversed = library.clone();
    reversed.reverse();
    let left = environment_authoring::scene_recipe_from_browser_preset(
        "editor:scene/destroyed_convoy",
        &library,
        0xA11C_E001,
    )
    .unwrap();
    let right = environment_authoring::scene_recipe_from_browser_preset(
        "editor:scene/destroyed_convoy",
        &reversed,
        0xA11C_E001,
    )
    .unwrap();
    assert_eq!(left, right);

    let incomplete = library
        .into_iter()
        .filter(|entry| !entry.path.ends_with("wires_hr_1.glb"))
        .collect::<Vec<_>>();
    assert!(environment_authoring::scene_recipe_from_browser_preset(
        "editor:scene/lit_utility_road",
        &incomplete,
        71,
    )
    .is_err());

    let utility = environment_authoring::scene_recipe_from_browser_preset(
        "editor:scene/lit_utility_road",
        &reversed,
        71,
    )
    .unwrap();
    let wire_height = utility
        .placements
        .iter()
        .find(|placement| placement.child_id == "utility.wire.00")
        .unwrap()
        .offset
        .y;
    assert!(utility
        .infrastructure
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .all(|node| (node.height_m - wire_height).abs() < 1.0e-5));

    let base = environment_authoring::scene_recipe_from_browser_preset(
        "editor:scene/forward_base",
        &reversed,
        71,
    )
    .unwrap();
    let mut south_centers = base
        .placements
        .iter()
        .filter(|placement| {
            placement.child_id.starts_with("barracks.wall.s.")
                || placement.child_id == "barracks.door"
        })
        .map(|placement| placement.offset.x)
        .collect::<Vec<_>>();
    south_centers.sort_by(f32::total_cmp);
    assert_eq!(south_centers, vec![-18.0, -14.0, -10.0, -6.0]);
}

#[test]
fn scene_provenance_and_infrastructure_roundtrip_without_placement_indexes() {
    use environment_authoring::SceneRecipeInstance;
    use glam::Vec3;

    let plan = environment_authoring::scene_recipe_from_browser_preset(
        "editor:scene/lit_utility_road",
        &reviewed_scene_library(),
        19,
    )
    .unwrap();
    let instance = SceneRecipeInstance {
        id: "utility_001".into(),
        recipe_id: plan.recipe_id.clone(),
        recipe_revision: 1,
        seed: 19,
        anchor: Vec3::new(10.0, 2.0, -30.0),
        yaw_deg: 45.0,
        resolved_assets: plan.resolved_assets(),
        generated_count: plan.placements.len() + plan.lights.len() + plan.surfaces.len(),
    };
    instance.validate().unwrap();
    let instance_json = instance.to_json();
    assert!(!instance_json.to_string().contains("placement_index"));
    let edits = serde_json::json!({"scene_recipe_instances": [instance_json]});
    assert_eq!(
        environment_authoring::scene_recipes_from_edits(Some(&edits)).unwrap(),
        vec![instance.clone()]
    );
    let duplicate = serde_json::json!({
        "scene_recipe_instances": [instance.to_json(), instance.to_json()]
    });
    assert!(environment_authoring::scene_recipes_from_edits(Some(&duplicate)).is_err());
    let overflow = vec![instance.clone(); environment_authoring::MAX_SCENE_INSTANCES + 1];
    assert!(environment_authoring::validate_scene_recipes(&overflow)
        .unwrap_err()
        .to_string()
        .contains("exceeds"));

    let run = plan.infrastructure.unwrap();
    let run_json = run.to_json();
    let edits = serde_json::json!({"infrastructure_runs": [run_json]});
    assert_eq!(
        environment_authoring::infrastructure_runs_from_edits(Some(&edits)).unwrap(),
        vec![run.clone()]
    );
    let overflow = vec![run.clone(); environment_authoring::MAX_INFRASTRUCTURE_RUNS + 1];
    assert!(
        environment_authoring::validate_infrastructure_runs(&overflow)
            .unwrap_err()
            .to_string()
            .contains("exceeds")
    );

    let mut broken = run.to_json();
    broken["spans"][0]["to_node"] = serde_json::json!("missing-node");
    let edits = serde_json::json!({"infrastructure_runs": [broken]});
    assert!(environment_authoring::infrastructure_runs_from_edits(Some(&edits)).is_err());
}
