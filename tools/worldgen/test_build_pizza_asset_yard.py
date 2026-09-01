#!/usr/bin/env python3
"""Pure-Python tests for the Pizza Doggy yard inventory and layout rules."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import struct
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("build_pizza_asset_yard.py")
SPEC = importlib.util.spec_from_file_location("build_pizza_asset_yard", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
yard = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = yard
SPEC.loader.exec_module(yard)


class CategoryTests(unittest.TestCase):
    def test_vendor_folder_category_is_preserved(self) -> None:
        relative = (
            "PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/"
            "Furniture/bed_1.glb"
        )
        self.assertEqual(yard.category_for_relative(relative), "Furniture")

    def test_nature_direct_models_get_useful_categories(self) -> None:
        prefix = (
            "PSX Nature v1.7.3/PSX Nature (Tree branches separated)/Models/"
            "GLB (recommended)/"
        )
        self.assertEqual(
            yard.category_for_relative(prefix + "pine_tree_n_1.glb"), "Pine Trees"
        )
        self.assertEqual(yard.category_for_relative(prefix + "stone_1.glb"), "Stones")

    def test_tech_and_retro_direct_models_get_useful_categories(self) -> None:
        tech = "PSX Tech v1.2.3/PSX Tech/Models/GLB (recommended)/lever_etx_1.glb"
        retro = (
            "Modular Retro FPS Kit v1.5/Modular Retro FPS Kit/Models/other-formats/"
            "GLB/tunnel_1_end.glb"
        )
        self.assertEqual(yard.category_for_relative(tech), "Levers")
        self.assertEqual(yard.category_for_relative(retro), "Tunnels")


class LayoutTests(unittest.TestCase):
    def test_every_source_receives_one_unique_cell(self) -> None:
        root = Path("/read-only")
        sources = [
            yard.AssetSource(
                path=root / f"asset_{index}.glb",
                relative_path=(
                    "PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/"
                    f"Furniture/asset_{index}.glb"
                ),
                pack="PSX Mega Pack 3.1.3",
                category="Furniture",
                stem=f"asset_{index}",
                byte_count=12,
                mtime_ns=0,
            )
            for index in range(33)
        ]
        placements, metadata = yard.create_layout(sources)
        self.assertEqual(len(placements), len(sources))
        self.assertEqual(len({(item.x, item.y) for item in placements}), len(sources))
        self.assertEqual(metadata["packs"][0]["count"], len(sources))
        self.assertEqual(metadata["categories"][0]["rows"], 3)


class InventoryTests(unittest.TestCase):
    def test_generated_orig_backups_are_not_counted_as_vendor_assets(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            model_dir = root / "PSX Nature v1.7.3" / "Models" / "GLB"
            model_dir.mkdir(parents=True)
            header = b"glTF" + struct.pack("<II", 2, 12)
            (model_dir / "tree_1.glb").write_bytes(header)
            (model_dir / "tree_1_orig.glb").write_bytes(header)
            sources = yard.discover_sources(root)
            self.assertEqual([source.stem for source in sources], ["tree_1"])
            self.assertEqual(
                yard.generated_backup_paths(root),
                ["PSX Nature v1.7.3/Models/GLB/tree_1_orig.glb"],
            )


if __name__ == "__main__":
    unittest.main()
