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
