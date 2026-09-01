#!/usr/bin/env python3
"""Unit tests for the reviewed Enigma/bridge recipes and GLB parser."""

from __future__ import annotations

import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import reviewed_static_asset_recipe as recipe
import verify_reviewed_static_glb as verifier


class RecipeTests(unittest.TestCase):
    def test_atlas_tile_convention_and_uv_remap(self) -> None:
        self.assertEqual(recipe.atlas_offset(0), (0.0, 0.0))
        self.assertEqual(recipe.atlas_offset(1), (0.5, 0.0))
        self.assertEqual(recipe.atlas_offset(2), (0.0, 0.5))
        self.assertEqual(recipe.atlas_offset(3), (0.5, 0.5))
        self.assertEqual(recipe.remap_uv((0.25, 0.75), 1), (0.625, 0.375))
        with self.assertRaises(ValueError):
            recipe.atlas_offset(4)

    def test_bridge_deck_is_upper_face_of_largest_equal_area_cohort(self) -> None:
        faces = [
            (-1.754179, 76.971513),
            (-1.443531, 76.971513),
            (1.754179, 5.239083),
            (-0.367013, 5.807010),
        ]
        self.assertAlmostEqual(recipe.select_bridge_deck_plane(faces), -1.443531)
        with self.assertRaises(ValueError):
            recipe.select_bridge_deck_plane([(0.0, 1.0)])

    def test_axis_dimension_mapping(self) -> None:
        self.assertEqual(
            recipe.blender_dimensions_to_gltf((19.978382, 4.065169, 3.508359)),
            (19.978382, 3.508359, 4.065169),
        )

    def test_reviewed_input_gate_rejects_wrong_source_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / recipe.BRIDGE.source_filename
            source.write_bytes(b"not the reviewed FBX")
            textures = root / "textures"
            textures.mkdir()
            with self.assertRaisesRegex(ValueError, "unreviewed bridge source bytes"):
                recipe.validate_reviewed_inputs(recipe.BRIDGE, source, textures)

    def test_reviewed_input_gate_accepts_declared_hash_set(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / recipe.BRIDGE.source_filename
            source.write_bytes(b"source")
            textures = root / "textures"
            textures.mkdir()
            for filename in recipe.BRIDGE.texture_sha256:
                (textures / filename).write_bytes(filename.encode("utf-8"))

            expected_by_path = {
                source: recipe.BRIDGE.source_sha256,
                **{
                    textures / filename: digest
                    for filename, digest in recipe.BRIDGE.texture_sha256.items()
                },
            }
            with mock.patch.object(
                recipe, "sha256", side_effect=lambda path: expected_by_path[path]
            ):
                observed = recipe.validate_reviewed_inputs(
                    recipe.BRIDGE, source, textures
                )
            self.assertEqual(
                observed[recipe.BRIDGE.source_filename], recipe.BRIDGE.source_sha256
            )
            self.assertEqual(len(observed), 1 + len(recipe.BRIDGE.texture_sha256))


class LiveReceiptTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.repository_root = Path(__file__).resolve().parent.parent
        cls.asset_files = {
            "enigma": (
                cls.repository_root
                / "assets/flavor/Props/enigma-machine-ashish0096-cc-by-4/baked"
                / recipe.ENIGMA.output_filename,
                cls.repository_root
                / "assets/flavor/Props/enigma-machine-ashish0096-cc-by-4/baked"
                / recipe.ENIGMA.report_filename,
            ),
            "bridge": (
                cls.repository_root
                / "assets/flavor/Structures/city-metal-bridge-remidoes3d-cc-by-4/baked"
                / recipe.BRIDGE.output_filename,
                cls.repository_root
                / "assets/flavor/Structures/city-metal-bridge-remidoes3d-cc-by-4/baked"
                / recipe.BRIDGE.report_filename,
            ),
        }

    def verify_modified_report(self, asset_key: str, report: dict) -> None:
        glb_path, _report_path = self.asset_files[asset_key]
        asset = recipe.ASSETS[asset_key]
        with tempfile.TemporaryDirectory() as temporary:
            report_path = Path(temporary) / asset.report_filename
            report_path.write_text(json.dumps(report), encoding="utf-8")
            verifier.verify(asset_key, glb_path, report_path)

    def test_live_reports_bind_all_source_and_tool_hashes(self) -> None:
        for asset_key, (glb_path, report_path) in self.asset_files.items():
            with self.subTest(asset=asset_key):
                summary = verifier.verify(asset_key, glb_path, report_path)
                self.assertEqual(summary["status"], "PASS")
                self.assertEqual(
                    summary["verified_tool_files"],
                    sorted(recipe.expected_tool_paths(recipe.ASSETS[asset_key])),
                )

    def test_live_reports_reject_stale_baker_hash(self) -> None:
        for asset_key, (_glb_path, report_path) in self.asset_files.items():
            with self.subTest(asset=asset_key):
                report = json.loads(report_path.read_text(encoding="utf-8"))
                report["toolchain"]["baker"]["sha256"] = "0" * 64
                with self.assertRaisesRegex(ValueError, "stale .* baker tool receipt"):
                    self.verify_modified_report(asset_key, report)

    def test_live_reports_reject_stale_extracted_source_hash(self) -> None:
        for asset_key, (_glb_path, report_path) in self.asset_files.items():
            with self.subTest(asset=asset_key):
                report = json.loads(report_path.read_text(encoding="utf-8"))
                source_filename = recipe.ASSETS[asset_key].source_filename
                report["inputs"]["sha256"][source_filename] = "0" * 64
                with self.assertRaisesRegex(ValueError, "extracted source hashes"):
                    self.verify_modified_report(asset_key, report)

    def test_live_reports_reject_stale_original_archive_hash(self) -> None:
        for asset_key, (_glb_path, report_path) in self.asset_files.items():
            with self.subTest(asset=asset_key):
                report = json.loads(report_path.read_text(encoding="utf-8"))
                report["source_archive"]["sha256"] = "0" * 64
                with self.assertRaisesRegex(ValueError, "original archive receipt"):
                    self.verify_modified_report(asset_key, report)


class GlbParserTests(unittest.TestCase):
    def make_glb(self, document: dict, binary: bytes = b"\0\0\0\0") -> bytes:
        encoded = json.dumps(document, separators=(",", ":")).encode("utf-8")
        encoded += b" " * ((-len(encoded)) % 4)
        binary += b"\0" * ((-len(binary)) % 4)
        total = 12 + 8 + len(encoded) + 8 + len(binary)
        return b"".join(
            (
                struct.pack("<4sII", verifier.GLB_MAGIC, verifier.GLB_VERSION, total),
                struct.pack("<II", len(encoded), verifier.JSON_CHUNK_TYPE),
                encoded,
                struct.pack("<II", len(binary), verifier.BIN_CHUNK_TYPE),
                binary,
            )
        )

    def test_parse_glb_accepts_json_and_bin_chunks(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "valid.glb"
            document = {"asset": {"version": "2.0"}, "buffers": [{"byteLength": 4}]}
            path.write_bytes(self.make_glb(document))
            parsed, binary = verifier.parse_glb(path)
            self.assertEqual(parsed, document)
            self.assertEqual(len(binary), 4)

    def test_parse_glb_rejects_declared_length_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "bad.glb"
            data = bytearray(self.make_glb({"asset": {"version": "2.0"}}))
            struct.pack_into("<I", data, 8, len(data) + 4)
            path.write_bytes(data)
            with self.assertRaisesRegex(ValueError, "length mismatch"):
                verifier.parse_glb(path)


if __name__ == "__main__":
    unittest.main()
