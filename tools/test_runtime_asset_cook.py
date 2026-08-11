#!/usr/bin/env python3
"""Offline regression suite for tools/runtime_asset_cook.py."""

from __future__ import annotations

import copy
import json
from pathlib import Path
import struct
import tempfile
import unittest

import runtime_asset_cook as cook


REPO_ROOT = Path(__file__).resolve().parents[1]
CATALOG_PATH = REPO_ROOT / cook.DEFAULT_CATALOG

ACT1_STORY_PIZZA_GLB_RELATIVES = {
    "mega1": [
        "Items & Weapons/canned_food_mp_5_rusty.glb",
        "Items & Weapons/hand_saw_1.glb",
        "Items & Weapons/meat_1.glb",
        "Large Props/supply_crate_1_empty.glb",
        "Small Props/tin_can_mp_1_rusty.glb",
    ],
    "mega2": [
        "Debris & Misc/gravel_pile_hr_1.glb",
        "Large Props & Machinery/chimney_a_2.glb",
        "Props/bucket_mx_1.glb",
        "Props/first_aid_kit_hr_1.glb",
        "Props/glass_bottle_mx_3.glb",
        "Props/handheld_fm_radio_etx_1.glb",
        "Props/jerrycan_mx_1.glb",
        "Props/luggage_mp_1.glb",
        "Props/metal_barrel_hr_4.glb",
        "Props/wooden_board_2.glb",
        "Props/wooden_crate_1.glb",
        "Props/wooden_plank_1.glb",
        "Weapons & Tools/axe_mx_1.glb",
        "Weapons & Tools/shovel_mx_1.glb",
    ],
}
ACT1_STORY_PIZZA_GLBS = {
    cook.PIZZA_PREFIXES[family] + relative
    for family, relatives in ACT1_STORY_PIZZA_GLB_RELATIVES.items()
    for relative in relatives
}

FEATURE_WINDOW_FRAME_GLB = (
    cook.PIZZA_PREFIXES["mega1"]
    + "Modular Structures/window_1_single_frame_only.glb"
)
BUILDING_CEILING_LAMP_GLB = cook.PIZZA_PREFIXES["retro_fps"] + "ceiling_lamp_fps_1.glb"
OLD_WINDOW_WALL_GLB = (
    cook.PIZZA_PREFIXES["mega2"] + "Modular Structures/wall_hr_1_m1a_1.glb"
)
PIZZA_SIDEARM_GLB = cook.PIZZA_PREFIXES["mega1"] + "Items & Weapons/pistol_mp_1.glb"
PIZZA_FIRE_BOTTLE_GLB = cook.PIZZA_PREFIXES["mega2"] + "Props/gas_cylinder_mx_1.glb"
REMOVED_NON_PIZZA_GAMEPLAY_GLBS = {
    "assets/88mm/baked/shell_AP.glb",
    "assets/88mm/baked/shell_HE.glb",
    "assets/88mm/baked/shell_case.glb",
    "assets/flavor/buildings/exports/BrickBuilding.glb",
    "assets/flavor/buildings/exports/IndustrialBuilding.glb",
    "assets/flavor/luger/luger.glb",
    "assets/flavor/psx-fire-extinguisher/extinguisher.glb",
}

BUILDING_RUNTIME_PIZZA_GLBS = {
    FEATURE_WINDOW_FRAME_GLB,
    BUILDING_CEILING_LAMP_GLB,
    cook.PIZZA_PREFIXES["mega2"] + "Modular Structures/wall_hr_1.glb",
    cook.PIZZA_PREFIXES["mega2"] + "Modular Structures/doorway_hr_2_wide.glb",
    cook.PIZZA_PREFIXES["mega2"] + "Modular Structures/floor_ceiling_hr_1.glb",
    cook.PIZZA_PREFIXES["mega2"] + "Modular Structures/floor_ceiling_hr_2_hole.glb",
    cook.PIZZA_PREFIXES["mega2"] + "Modular Structures/roof_mh_4_a_1.glb",
    cook.PIZZA_PREFIXES["mega2"] + "Props/shelf_mx_7.glb",
    cook.PIZZA_PREFIXES["mega1"] + "Modular Structures/stairs_5_concrete.glb",
}
PREVIOUS_BUILDING_RUNTIME_PIZZA_GLBS = {
    cook.PIZZA_PREFIXES["mega2"] + "Modular Structures/doorway_hr_1.glb",
    OLD_WINDOW_WALL_GLB,
    cook.PIZZA_PREFIXES["mega1"] + "Modular Structures/stairs_4.glb",
}
UNREFERENCED_BUILDING_RUNTIME_PIZZA_GLBS = {
    cook.PIZZA_PREFIXES["mega1"] + "Modular Structures/stairs_4.glb",
}


def fixture_glb(*, external_buffer: str | None = None) -> bytes:
    positions = struct.pack("<9f", -1.0, 0.0, -2.0, 1.0, 0.0, -2.0, 0.0, 3.0, 2.0)
    buffer: dict[str, object] = {"byteLength": len(positions)}
    if external_buffer is not None:
        buffer["uri"] = external_buffer
    document = {
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "max": [1.0, 3.0, 2.0],
                "min": [-1.0, 0.0, -2.0],
                "type": "VEC3",
            }
        ],
        "asset": {"version": "2.0"},
        "bufferViews": [{"buffer": 0, "byteLength": len(positions)}],
        "buffers": [buffer],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "nodes": [{"mesh": 0, "translation": [2.0, 1.0, -1.0]}],
        "scene": 0,
        "scenes": [{"nodes": [0]}],
    }
    json_chunk = json.dumps(document, separators=(",", ":"), sort_keys=True).encode("utf-8")
    json_chunk += b" " * ((-len(json_chunk)) % 4)
    chunks = [struct.pack("<II", len(json_chunk), 0x4E4F534A) + json_chunk]
    if external_buffer is None:
        binary = positions + b"\x00" * ((-len(positions)) % 4)
        chunks.append(struct.pack("<II", len(binary), 0x004E4942) + binary)
    body = b"".join(chunks)
    return struct.pack("<4sII", b"glTF", 2, 12 + len(body)) + body


def tree_contents(root: Path) -> dict[str, bytes]:
    return {
        path.relative_to(root).as_posix(): path.read_bytes()
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


class RuntimeAssetCookTests(unittest.TestCase):
    def test_live_catalog_locks_the_complete_audited_model_closure(self) -> None:
        catalog = cook.load_catalog(CATALOG_PATH)
        sources = [REPO_ROOT / entry["source"] for entry in catalog["assets"]]
        installed = [path for path in sources if path.is_file()]
        if not installed:
            self.skipTest("untracked runtime model payload is not installed")
        self.assertEqual(
            len(installed),
            len(sources),
            f"partial runtime model payload: {len(installed)}/{len(sources)} cataloged GLBs installed",
        )
        records = cook.validate_catalog(catalog, REPO_ROOT)
        self.assertEqual(len(records), 134)
        self.assertEqual(
            catalog["scope"],
            {
                "campaign_manifest_pizza_glbs": 59,
                "campaign_manifest_unique_glbs": 69,
                "non_pizza_glbs": 16,
                "pizza_glbs": 118,
                "total_glbs": 134,
            },
        )
        self.assertTrue(all(record["dependencies"] == [] for record in records))
        blockers = [
            entry
            for entry in catalog["assets"]
            if entry["license"] == "kessel-provenance-unrecorded"
        ]
        self.assertEqual(len(blockers), 16)

    def test_retired_manifest_models_are_semantic_ids_not_cooked_models(self) -> None:
        catalog = cook.load_catalog(CATALOG_PATH)
        catalog_sources = {entry["source"] for entry in catalog["assets"]}
        selected, counts = cook.candidate_sources(REPO_ROOT)
        expected_denied = (
            cook.RETIRED_SEMANTIC_GLB_SOURCES
            | REMOVED_NON_PIZZA_GAMEPLAY_GLBS
            | {"assets/tanks/tiger/baked/tiger_part_round.glb"}
        )
        self.assertEqual(cook.DENIED_RETIRED_GLB_SOURCES, expected_denied)
        self.assertEqual(cook.DENIED_RETIRED_GLB_SOURCES & catalog_sources, set())
        self.assertEqual(cook.DENIED_RETIRED_GLB_SOURCES & set(selected), set())
        self.assertEqual(counts["campaign_manifest_unique_glbs"], 69)

        authored = set()
        for act_id in (1, 2, 3):
            manifest = json.loads(
                (REPO_ROOT / f"assets/worldgen/act{act_id}/manifest.v1.json").read_text()
            )
            authored.update(
                source
                for source in manifest["assets"]
                if source in cook.RETIRED_SEMANTIC_GLB_SOURCES
            )
        self.assertEqual(authored, cook.RETIRED_SEMANTIC_GLB_SOURCES)

        self.assertEqual(
            cook.NON_PIZZA_RUNTIME_GLOBS,
            [
                "assets/tanks/T34-76/baked/*.glb",
                "assets/tanks/tiger/baked/*.glb",
            ],
        )

        editor = (REPO_ROOT / "src/editor.rs").read_text(encoding="utf-8")
        self.assertEqual(editor.count("crate::gltf_load::load_glb"), 1)
        self.assertEqual(editor.count("crate::worldgen::load_module"), 1)
        self.assertGreaterEqual(editor.count("load_editor_asset("), 3)

    def test_all_code_authored_pizza_paths_are_cataloged(self) -> None:
        catalog = cook.load_catalog(CATALOG_PATH)
        by_source = {entry["source"]: entry for entry in catalog["assets"]}
        discovered = cook.discover_code_authored_pizza(REPO_ROOT)
        self.assertEqual(len(discovered), 97)
        self.assertEqual(set(discovered) - set(by_source), set())
        for source, consumers in discovered.items():
            with self.subTest(source=source):
                entry = by_source[source]
                self.assertTrue(consumers <= set(entry["required_by"]))
                self.assertEqual(entry["license"], "pizzadoggy-local-purchase")
                self.assertTrue(entry["source_family"].startswith("pizzadoggy-"))
        selected, _counts = cook.candidate_sources(REPO_ROOT)
        self.assertEqual(set(selected), set(by_source))
        self.assertEqual(
            {source: set(entry["required_by"]) for source, entry in by_source.items()},
            selected,
        )

    def test_gameplay_props_are_pizza_and_ordnance_is_procedural(self) -> None:
        catalog = cook.load_catalog(CATALOG_PATH)
        by_source = {entry["source"]: entry for entry in catalog["assets"]}
        selected, _counts = cook.candidate_sources(REPO_ROOT)
        self.assertEqual(cook.DENIED_RETIRED_GLB_SOURCES & set(by_source), set())
        self.assertEqual(cook.DENIED_RETIRED_GLB_SOURCES & set(selected), set())

        for source, expected_tags, collision, destruction in (
            (PIZZA_SIDEARM_GLB, {"weapon", "sidearm"}, "held_item", "none"),
            (
                PIZZA_FIRE_BOTTLE_GLB,
                {"equipment", "fire-suppression"},
                "held_item",
                "prop",
            ),
        ):
            with self.subTest(source=source):
                entry = by_source[source]
                self.assertIn("code.game", entry["required_by"])
                self.assertEqual(entry["license"], "pizzadoggy-local-purchase")
                self.assertTrue(expected_tags <= set(entry["tags"]))
                self.assertEqual(entry["collision_class"], collision)
                self.assertEqual(entry["destruction_class"], destruction)
                self.assertEqual(
                    entry["canonical"]["runtime_path"],
                    f"models/{cook.stable_id(source)}.glb",
                )

        game = (REPO_ROOT / "src/game.rs").read_text(encoding="utf-8")
        gltf = (REPO_ROOT / "src/gltf_load.rs").read_text(encoding="utf-8")
        generator = (REPO_ROOT / "tools/worldgen/gen_world.py").read_text(encoding="utf-8")
        main = (REPO_ROOT / "src/main.rs").read_text(encoding="utf-8")
        for obsolete in REMOVED_NON_PIZZA_GAMEPLAY_GLBS:
            self.assertNotIn(obsolete, game)
            self.assertNotIn(obsolete, gltf)
        self.assertNotIn("BrickBuilding.glb", generator)
        self.assertNotIn("IndustrialBuilding.glb", generator)
        self.assertNotIn("load_glb_capped", gltf)
        self.assertIn("procedural_ordnance::ap_round_mesh", game)
        self.assertIn("procedural_ordnance::he_round_mesh", game)
        self.assertIn("procedural_ordnance::spent_case_mesh", game)
        self.assertIn("mod procedural_ordnance;", main)

    def test_denied_retired_models_cannot_reenter_through_manifests_or_edits(self) -> None:
        with tempfile.TemporaryDirectory(prefix="kessel-retired-injection-") as temp:
            root = Path(temp)
            unknown = "assets/unreviewed/future_model.glb"
            denied = sorted(cook.DENIED_RETIRED_GLB_SOURCES)
            for act_id in (1, 2, 3):
                act_dir = root / f"assets/worldgen/act{act_id}"
                act_dir.mkdir(parents=True)
                assets = denied + [unknown] if act_id == 1 else []
                (act_dir / "manifest.v1.json").write_text(
                    json.dumps({"assets": assets}), encoding="utf-8"
                )
            (root / "assets/worldgen/act1/manifest_edits.json").write_text(
                json.dumps(
                    {
                        "ops": [
                            {"op": "add", "asset": source}
                            for source in denied
                        ]
                        + [{"op": "add", "asset": unknown}]
                    }
                ),
                encoding="utf-8",
            )

            selected, counts = cook.candidate_sources(root)
            self.assertEqual(
                selected,
                {unknown: {"act1.manifest", "act1.manifest-edits"}},
            )
            self.assertEqual(cook.DENIED_RETIRED_GLB_SOURCES & set(selected), set())
            self.assertEqual(
                counts["campaign_manifest_unique_glbs"],
                len(cook.DENIED_RETIRED_GLB_SOURCES) + 1,
            )
            with self.assertRaisesRegex(
                cook.CatalogError,
                r"required runtime source is missing: assets/unreviewed/future_model\.glb",
            ):
                cook.build_catalog(root)

    def test_featured_building_modules_are_in_the_runtime_closure(self) -> None:
        catalog = cook.load_catalog(CATALOG_PATH)
        by_source = {entry["source"]: entry for entry in catalog["assets"]}
        discovered = cook.discover_code_authored_pizza(REPO_ROOT)
        building_sources = {
            source
            for source, consumers in discovered.items()
            if "code.building-runtime" in consumers
        }
        self.assertEqual(building_sources, BUILDING_RUNTIME_PIZZA_GLBS)
        self.assertEqual(BUILDING_RUNTIME_PIZZA_GLBS - set(by_source), set())
        self.assertEqual(
            discovered[FEATURE_WINDOW_FRAME_GLB],
            {"code.building-runtime", "code.city"},
        )
        self.assertEqual(
            discovered[BUILDING_CEILING_LAMP_GLB],
            {"code.building-runtime"},
        )
        self.assertEqual(discovered[OLD_WINDOW_WALL_GLB], {"code.worldgen"})
        for source in PREVIOUS_BUILDING_RUNTIME_PIZZA_GLBS:
            self.assertNotIn("code.building-runtime", discovered.get(source, set()))
        self.assertEqual(
            UNREFERENCED_BUILDING_RUNTIME_PIZZA_GLBS & set(discovered),
            set(),
        )
        self.assertEqual(
            UNREFERENCED_BUILDING_RUNTIME_PIZZA_GLBS & set(by_source),
            set(),
        )
        for source in sorted(BUILDING_RUNTIME_PIZZA_GLBS):
            with self.subTest(source=source):
                entry = by_source[source]
                self.assertIn("code.building-runtime", entry["required_by"])
                self.assertEqual(entry["license"], "pizzadoggy-local-purchase")
                self.assertEqual(
                    entry["canonical"]["runtime_path"],
                    f"models/{cook.stable_id(source)}.glb",
                )

    def test_act1_story_props_have_canonical_packaged_mappings(self) -> None:
        catalog = cook.load_catalog(CATALOG_PATH)
        by_source = {entry["source"]: entry for entry in catalog["assets"]}
        self.assertEqual(len(ACT1_STORY_PIZZA_GLBS), 19)
        self.assertEqual(ACT1_STORY_PIZZA_GLBS - set(by_source), set())
        for source in sorted(ACT1_STORY_PIZZA_GLBS):
            with self.subTest(source=source):
                entry = by_source[source]
                self.assertIn("code.worldgen", entry["required_by"])
                self.assertEqual(
                    entry["canonical"]["runtime_path"],
                    f"models/{cook.stable_id(source)}.glb",
                )

    def test_new_code_authored_alias_literal_joins_the_closure_without_installed_glb(self) -> None:
        with tempfile.TemporaryDirectory(prefix="kessel-code-assets-") as temp:
            root = Path(temp)
            source = root / "src/worldgen.rs"
            source.parent.mkdir(parents=True)
            source.write_text(
                '// format!("{MP2}Props/commented_out.glb")\n'
                'const NOTE: &str = r#"not-a-model"#;\n'
                'fn is_model(path: &str) -> bool { path.ends_with(".glb") }\n'
                'fn path() -> String { format!("{TECH}future_console_etx_1.glb") }\n',
                encoding="utf-8",
            )
            expected = cook.PIZZA_PREFIXES["tech"] + "future_console_etx_1.glb"
            self.assertEqual(
                cook.discover_code_authored_pizza(root),
                {expected: {"code.worldgen"}},
            )

    def test_psx_tech_uses_the_pizza_license_and_source_family(self) -> None:
        source = cook.PIZZA_PREFIXES["tech"] + "capacitor_etx_6_1.glb"
        self.assertEqual(
            cook.pizza_family(source),
            ("tech", "capacitor_etx_6_1.glb"),
        )
        policy = cook.semantic_policy(source)
        self.assertEqual(policy["license"], "pizzadoggy-local-purchase")
        self.assertEqual(policy["source_family"], "pizzadoggy-tech")
        self.assertIn("tech", policy["tags"])

    def test_world_editor_palette_is_pizza_only_with_prefabs_retained(self) -> None:
        source = (REPO_ROOT / "src/assetlib.rs").read_text(encoding="utf-8")
        roots = source.split("const LIB_ROOTS", 1)[1].split("];", 1)[0]
        self.assertIn('"assets/pizza/_extracted"', roots)
        for forbidden in ("assets/tanks", "assets/flavor", "assets/88mm"):
            self.assertNotIn(forbidden, roots)
        self.assertIn('pub const PREFAB_DIR: &str = "assets/worldgen/prefabs";', source)

    def test_worldbuilder_additions_join_the_model_closure(self) -> None:
        with tempfile.TemporaryDirectory(prefix="kessel-manifest-edits-") as temp:
            root = Path(temp)
            for act_id in (1, 2, 3):
                act_dir = root / f"assets/worldgen/act{act_id}"
                act_dir.mkdir(parents=True)
                (act_dir / "manifest.v1.json").write_text(
                    json.dumps({"assets": []}), encoding="utf-8"
                )
            added = "assets/pizza/fixture/Models/GLB/editor_added.glb"
            (root / "assets/worldgen/act2/manifest_edits.json").write_text(
                json.dumps({"ops": [{"op": "add", "asset": added}]}),
                encoding="utf-8",
            )
            sources, counts = cook.candidate_sources(root)
            self.assertEqual(sources, {added: {"act2.manifest-edits"}})
            self.assertEqual(counts["campaign_manifest_unique_glbs"], 1)
            self.assertEqual(counts["campaign_manifest_pizza_glbs"], 1)

    def test_fixture_cooks_are_byte_identical_and_emit_required_manifests(self) -> None:
        template = cook.load_catalog(CATALOG_PATH)
        with tempfile.TemporaryDirectory(prefix="kessel-asset-cook-") as temp:
            root = Path(temp)
            source = root / "assets/pizza/fixture/Models/GLB/triangle.glb"
            source.parent.mkdir(parents=True)
            source.write_bytes(fixture_glb())
            entry = cook.catalog_entry(
                root,
                source.relative_to(root).as_posix(),
                ["offline.fixture"],
            )
            catalog = copy.deepcopy(template)
            catalog["assets"] = [entry]
            catalog["scope"] = {
                "campaign_manifest_pizza_glbs": 0,
                "campaign_manifest_unique_glbs": 0,
                "non_pizza_glbs": 0,
                "pizza_glbs": 1,
                "total_glbs": 1,
            }
            first = root / "cook-a"
            second = root / "cook-b"
            manifest_a = cook.cook_catalog(catalog, root, first, enforce_repo_scope=False)
            manifest_b = cook.cook_catalog(catalog, root, second, enforce_repo_scope=False)
            self.assertEqual(cook.canonical_json(manifest_a), cook.canonical_json(manifest_b))
            self.assertEqual(tree_contents(first), tree_contents(second))
            self.assertEqual(manifest_a["asset_count"], 1)
            self.assertEqual(manifest_a["missing_dependencies"], [])
            self.assertEqual(
                json.loads((first / "missing-dependencies.json").read_text())["dependencies"],
                [],
            )
            self.assertTrue((first / "MANIFEST.bytes.tsv").is_file())
            self.assertTrue((first / "MANIFEST.sha256").is_file())

    def test_raw_and_preview_inputs_are_rejected(self) -> None:
        for raw in (
            "assets/pizza/raw/tank.zip",
            "assets/pizza/raw/tank.fbx",
            "assets/pizza/raw/tank.obj",
            "assets/pizza/raw/tank.dae",
            "assets/pizza/raw/tank.mtl",
            "assets/pizza/raw/tank.tif",
            "assets/pizza/raw/tank.blend1",
            "assets/pizza/raw/tank.blend2",
            "assets/pizza/_previews/tank.glb",
        ):
            with self.subTest(raw=raw), self.assertRaises(cook.CatalogError):
                cook.reject_noncanonical_source(cook.safe_repo_path(raw))

    def test_external_glb_dependency_is_reported_and_not_silently_cooked(self) -> None:
        parsed = cook.parse_glb(fixture_glb(external_buffer="mesh.bin"), "fixture.glb")
        self.assertEqual(parsed["dependencies"], ["mesh.bin"])
        self.assertIsNone(parsed["bounds"])

    def test_output_scanner_rejects_raw_or_preview_material(self) -> None:
        with tempfile.TemporaryDirectory(prefix="kessel-output-reject-") as temp:
            output = Path(temp)
            bad = output / "models/raw_source.fbx"
            bad.parent.mkdir()
            bad.write_bytes(b"not runtime material")
            with self.assertRaises(cook.CatalogError):
                cook.validate_output_tree(output)

    def test_cook_refuses_to_write_into_source_assets(self) -> None:
        catalog = cook.load_catalog(CATALOG_PATH)
        with self.assertRaises(cook.CatalogError):
            cook.cook_catalog(
                catalog,
                REPO_ROOT,
                REPO_ROOT / "assets/forbidden-runtime-cook",
            )


if __name__ == "__main__":
    unittest.main(verbosity=2)
