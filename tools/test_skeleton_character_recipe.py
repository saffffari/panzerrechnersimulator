#!/usr/bin/env python3
"""Offline contract tests for the reviewed Vinrax skeleton bake."""

from __future__ import annotations

import copy
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import skeleton_character_recipe as recipe  # noqa: E402


def write_glb(path: Path, *, rigged: bool, triangles: int = recipe.SOURCE_TRIANGLES) -> None:
    attributes = {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2}
    accessors = [
        {"count": 3, "min": [-0.2, 0.0, -0.1], "max": [0.2, 1.53, 0.1]},
        {"count": 3},
        {"count": 3},
        {"count": triangles * 3},
    ]
    nodes = [{"name": "SkeletonCharacter", "mesh": 0}]
    document = {
        "asset": {"version": "2.0"},
        "accessors": accessors,
        "buffers": [{"byteLength": 4}],
        "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 4}],
        "images": [{"bufferView": 0, "mimeType": "image/png"}],
        "textures": [{"source": 0}],
        "materials": [{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}}],
        "meshes": [{"primitives": [{"attributes": attributes, "indices": 3, "material": 0}]}],
        "nodes": nodes,
    }
    if rigged:
        attributes.update({"JOINTS_0": 4, "WEIGHTS_0": 5})
        accessors.extend([{"count": 3}, {"count": 3}, {"count": 1}])
        nodes[0]["skin"] = 0
        nodes.extend({"name": name} for name in recipe.ARMATURE_BONES)
        document["skins"] = [
            {"joints": list(range(1, 1 + len(recipe.ARMATURE_BONES))), "inverseBindMatrices": 6}
        ]
    encoded = json.dumps(document, separators=(",", ":")).encode("utf-8")
    encoded += b" " * ((-len(encoded)) % 4)
    binary = b"\0\0\0\0"
    total = 12 + 8 + len(encoded) + 8 + len(binary)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(
        struct.pack("<4sII", b"glTF", 2, total)
        + struct.pack("<II", len(encoded), 0x4E4F534A)
        + encoded
        + struct.pack("<II", len(binary), 0x004E4942)
        + binary
    )


class SkeletonCharacterRecipeTests(unittest.TestCase):
    def test_reviewed_contract(self) -> None:
        self.assertEqual(len(recipe.ARMATURE_BONES), 27)
        self.assertEqual(len(recipe.BODY_PARTS), 11)
        self.assertEqual(recipe.SOURCE_TRIANGLES, 1596)
        self.assertEqual(recipe.TARGET_HEIGHT_M, 1.53)

    def test_runtime_classifier_covers_boundaries(self) -> None:
        self.assertEqual(recipe.classify_relaxed_point((0.0, 1.30, 0.0)), "Head")
        self.assertEqual(recipe.classify_relaxed_point((-0.20, 0.90, 0.0)), "LeftUpperArm")
        self.assertEqual(recipe.classify_relaxed_point((0.20, 0.60, 0.0)), "RightForearm")
        self.assertEqual(recipe.classify_relaxed_point((-0.10, 0.50, 0.0)), "LeftThigh")
        self.assertEqual(recipe.classify_relaxed_point((0.10, 0.20, 0.0)), "RightShin")
        self.assertEqual(recipe.classify_relaxed_point((0.0, 0.75, 0.0)), "Pelvis")
        self.assertEqual(recipe.classify_relaxed_point((0.0, 1.00, 0.0)), "Torso")

    def test_safe_paths_reject_escape(self) -> None:
        for value in ("../escape", "/absolute", "a/../../b"):
            with self.assertRaises(ValueError):
                recipe.safe_relative_path(value)

    def test_static_and_rigged_glb_contracts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            static = root / "static.glb"
            rigged = root / "rigged.glb"
            write_glb(static, rigged=False)
            write_glb(rigged, rigged=True)
            self.assertEqual(recipe.validate_glb(static, rigged=False)["skin_count"], 0)
            self.assertEqual(recipe.validate_glb(rigged, rigged=True)["joint_count"], 27)
            with self.assertRaisesRegex(ValueError, "unexpectedly contains skinning"):
                recipe.validate_glb(rigged, rigged=False)
            with self.assertRaisesRegex(ValueError, "27-joint"):
                recipe.validate_glb(static, rigged=True)

    def test_wrong_triangle_count_and_external_image_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "skeleton.glb"
            write_glb(path, rigged=False, triangles=12)
            with self.assertRaisesRegex(ValueError, "1,596 triangles"):
                recipe.validate_glb(path, rigged=False)

            write_glb(path, rigged=False)
            data = path.read_bytes()
            json_length = struct.unpack_from("<I", data, 12)[0]
            document = json.loads(data[20 : 20 + json_length].rstrip(b" ").decode("utf-8"))
            document["images"] = [{"uri": "external.png"}]
            encoded = json.dumps(document, separators=(",", ":")).encode("utf-8")
            encoded += b" " * ((-len(encoded)) % 4)
            binary = b"\0\0\0\0"
            total = 12 + 8 + len(encoded) + 8 + len(binary)
            path.write_bytes(
                struct.pack("<4sII", b"glTF", 2, total)
                + struct.pack("<II", len(encoded), 0x4E4F534A)
                + encoded
                + struct.pack("<II", len(binary), 0x004E4942)
                + binary
            )
            with self.assertRaisesRegex(ValueError, "not embedded"):
                recipe.validate_glb(path, rigged=False)

    def test_live_receipt_binds_all_source_and_tool_hashes(self) -> None:
        asset_root = (
            Path(__file__).resolve().parent.parent
            / "assets/flavor/Characters/skeleton-character-psx-vinrax-cc-by-4"
        )
        report = json.loads(
            (asset_root / recipe.REPORT_RELATIVE).read_text(encoding="utf-8")
        )
        summary = recipe.verify_bake_receipt(asset_root, report)
        self.assertEqual(summary["status"], "PASS")
        self.assertEqual(
            summary["verified_tool_files"], sorted(recipe.expected_tool_paths())
        )

        stale_tool = copy.deepcopy(report)
        stale_tool["toolchain"]["files"]["baker"]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "stale receipt"):
            recipe.verify_bake_receipt(asset_root, stale_tool)

        stale_source = copy.deepcopy(report)
        stale_source["source"]["private_source_receipt"]["archive"]["sha256"] = (
            "0" * 64
        )
        with self.assertRaisesRegex(ValueError, "private source file hashes"):
            recipe.verify_bake_receipt(asset_root, stale_source)


if __name__ == "__main__":
    unittest.main()
