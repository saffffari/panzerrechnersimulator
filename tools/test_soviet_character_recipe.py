#!/usr/bin/env python3
"""Offline contract tests for the Soviet infantry bake recipe (Blender not required)."""

from __future__ import annotations

import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import soviet_character_recipe as recipe  # noqa: E402


def write_glb(path: Path, document) -> None:
    encoded = json.dumps(document, separators=(",", ":")).encode("utf-8")
    encoded += b" " * ((4 - len(encoded) % 4) % 4)
    total = 12 + 8 + len(encoded)
    path.write_bytes(
        struct.pack("<4sII", b"glTF", 2, total)
        + struct.pack("<II", len(encoded), 0x4E4F534A)
        + encoded
    )


def valid_glb_document(
    *,
    rigged: bool,
    animation_durations=None,
) -> dict:
    accessors = [{"name": "positions"}]
    attributes = {"POSITION": 0}
    nodes = [{"mesh": 0}]
    document = {
        "asset": {"version": "2.0"},
        "meshes": [
            {
                "primitives": [
                    {
                        "attributes": attributes,
                        "material": 0,
                    }
                ]
            }
        ],
        "nodes": nodes,
        "materials": [
            {"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}}
        ],
        "textures": [{"source": 0}],
        "images": [{"bufferView": 0, "mimeType": "image/png"}],
        "bufferViews": [{}],
        "accessors": accessors,
    }
    if not rigged:
        return document
    attributes.update({"JOINTS_0": 1, "WEIGHTS_0": 2})
    accessors.extend([{"name": "joints"}, {"name": "weights"}])
    inverse_bind_index = len(accessors)
    accessors.append({"name": "inverse-bind"})
    nodes[0]["skin"] = 0
    nodes.append({"name": "Hips"})
    document["skins"] = [
        {"joints": [1], "inverseBindMatrices": inverse_bind_index}
    ]
    animations = []
    for name, duration_s in (animation_durations or {}).items():
        input_index = len(accessors)
        output_index = input_index + 1
        accessors.extend(
            [
                {"min": [0.0], "max": [duration_s]},
                {"name": f"{name}-rotation"},
            ]
        )
        animations.append(
            {
                "name": name,
                "samplers": [
                    {
                        "input": input_index,
                        "output": output_index,
                        "interpolation": "LINEAR",
                    }
                ],
                "channels": [
                    {
                        "sampler": 0,
                        "target": {"node": 1, "path": "rotation"},
                    }
                ],
            }
        )
    document["animations"] = animations
    return document


class SovietCharacterRecipeTests(unittest.TestCase):
    def test_recipe_covers_required_motion_states(self):
        recipe.validate_recipe()
        self.assertEqual(
            set(recipe.ANIMATION_SEQUENCES),
            {"idle", "walk", "run", "aim", "fire", "death"},
        )
        names = {frame.name for frame in recipe.POSE_FRAMES}
        self.assertTrue({"aim_00", "fire_01", "death_03"}.issubset(names))
        self.assertFalse(recipe.ANIMATION_SEQUENCES["fire"]["loop"])
        self.assertFalse(recipe.ANIMATION_SEQUENCES["death"]["loop"])
        self.assertGreaterEqual(len(names), 16)

    def test_animation_manifest_is_stable_and_references_real_pose_names(self):
        first = recipe.stable_json(recipe.animation_manifest())
        second = recipe.stable_json(recipe.animation_manifest())
        self.assertEqual(first, second)
        manifest = json.loads(first)
        poses = {pose["name"] for pose in manifest["poses"]}
        for sequence in manifest["sequences"].values():
            for frame in sequence["frames"]:
                self.assertIn(frame["pose"], poses)
                self.assertGreater(frame["duration_s"], 0.0)
        self.assertEqual(manifest["runtime_mode"], "static GLB pose-frame switching")

    def test_posture_classification_routes_t_a_and_compact_sources(self):
        self.assertEqual(
            recipe.classify_source_posture(1.58, 1.72)["recommended_rigging_mode"],
            "donor-transfer",
        )
        self.assertEqual(
            recipe.classify_source_posture(1.20, 1.72)["recommended_rigging_mode"],
            "a-pose",
        )
        self.assertEqual(
            recipe.classify_source_posture(0.72, 1.72)["recommended_rigging_mode"],
            "compact-standing",
        )
        with self.assertRaises(ValueError):
            recipe.classify_source_posture(1.0, 0.0)

    def test_attribution_hashes_the_supplied_bytes_and_names_exact_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "soldier.glb"
            source.write_bytes(b"downloaded source bytes")
            record = recipe.attribution_record(
                source,
                logical_source_path="source/ww2_soviet_soldier_original.glb",
                retrieved="2026-08-30",
            )
            recipe.validate_hash_records(record)
            self.assertEqual(record["title"], "WW2 Soviet Soldier")
            self.assertEqual(record["author"], "Michael Jones")
            self.assertEqual(record["author_handle"], "mikhailgolubkov09")
            self.assertEqual(record["license"]["spdx"], "CC-BY-4.0")
            self.assertEqual(record["source_page"], recipe.ASSET_SOURCE_PAGE)
            self.assertEqual(record["preserved_source"]["bytes"], len(b"downloaded source bytes"))
            markdown = recipe.attribution_markdown(record)
            self.assertIn(recipe.ASSET_SOURCE_PAGE, markdown)
            self.assertIn(record["preserved_source"]["sha256"], markdown)

    def test_preserved_source_is_byte_exact_and_never_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            supplied = root / "authenticated-download.glb"
            preserved = root / "asset/source/ww2_soviet_soldier_original.glb"
            original_bytes = b"glTF\x00operator-supplied\xffbytes"
            supplied.write_bytes(original_bytes)

            recipe.preserve_source_bytes(supplied, preserved)
            self.assertEqual(preserved.read_bytes(), original_bytes)
            self.assertEqual(recipe.sha256_file(preserved), recipe.sha256_file(supplied))

            recipe.preserve_source_bytes(supplied, preserved)
            self.assertEqual(preserved.read_bytes(), original_bytes)

            replacement = root / "different-download.glb"
            replacement.write_bytes(b"different authenticated bytes")
            with self.assertRaisesRegex(RuntimeError, "refusing to overwrite"):
                recipe.preserve_source_bytes(replacement, preserved)
            self.assertEqual(preserved.read_bytes(), original_bytes)

            recipe.preserve_source_bytes(preserved, preserved)
            self.assertEqual(preserved.read_bytes(), original_bytes)

    def test_hash_validation_rejects_placeholder_values(self):
        with self.assertRaises(ValueError):
            recipe.validate_hash_records({"sha256": "unknown"})
        with self.assertRaises(ValueError):
            recipe.validate_hash_records({"sha256": "0" * 63})
        with self.assertRaises(ValueError):
            recipe.validate_hash_records({"sha256": "0" * 64})
        recipe.validate_hash_records({"sha256": "a" * 64})

    def test_release_verifier_rehashes_outputs_and_rejects_stale_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source/ww2_soviet_soldier_original.glb"
            rigged = root / "authoring/soviet_ww2_soldier_rigged.glb"
            manifest = root / "baked/animation_manifest.v1.json"
            for path in (source, rigged, manifest):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(path.name.encode("utf-8"))
            metadata = []
            for name in ("asset.json", "SOURCE.md", "LICENSE.txt"):
                path = root / name
                path.write_bytes(name.encode("utf-8"))
                metadata.append(recipe.file_record(path, logical_path=name))
            poses = []
            for frame in recipe.POSE_FRAMES:
                relative = f"baked/soviet_ww2_soldier_{frame.name}.glb"
                path = root / relative
                path.write_bytes(frame.name.encode("utf-8"))
                poses.append(recipe.file_record(path, logical_path=relative))
            source_record = recipe.file_record(
                source,
                logical_path="source/ww2_soviet_soldier_original.glb",
            )
            report = {
                "asset": {"preserved_source": source_record},
                "sources": {"preserved_soviet_glb": source_record},
                "rigged_master": recipe.file_record(
                    rigged,
                    logical_path="authoring/soviet_ww2_soldier_rigged.glb",
                ),
                "animation_manifest": recipe.file_record(
                    manifest,
                    logical_path="baked/animation_manifest.v1.json",
                ),
                "metadata_files": metadata,
                "static_poses": poses,
            }
            verification = recipe.verify_bake_receipt(root, report)
            self.assertEqual(verification["static_pose_count"], len(recipe.POSE_FRAMES))

            rigged.write_bytes(b"changed after receipt")
            with self.assertRaisesRegex(ValueError, "stale"):
                recipe.verify_bake_receipt(root, report)

    def test_deformation_quality_accepts_local_joint_skinning(self):
        report = recipe.deformation_quality_report(
            rest_edge_lengths=[1.0] * 100,
            posed_edge_lengths=[0.78, 1.34] * 50,
            weld_separations_m=[0.0, 0.0015],
            height_m=1.72,
        )
        self.assertTrue(report["passed"], report["violations"])
        self.assertEqual(report["edge_sample_count"], 100)
        self.assertIsInstance(report["edge_sample_count"], int)

    def test_deformation_quality_rejects_spikes_and_opened_seams(self):
        stretched = recipe.deformation_quality_report(
            rest_edge_lengths=[1.0] * 100,
            posed_edge_lengths=[1.0] * 97 + [3.5, 6.0, 9.0],
            weld_separations_m=[0.0],
            height_m=1.72,
        )
        self.assertFalse(stretched["passed"])
        self.assertTrue(
            any("maximum_edge_stretch_ratio" in item for item in stretched["violations"])
        )
        opened_seam = recipe.deformation_quality_report(
            rest_edge_lengths=[1.0] * 20,
            posed_edge_lengths=[1.0] * 20,
            weld_separations_m=[0.0, 0.04],
            height_m=1.72,
        )
        self.assertFalse(opened_seam["passed"])
        self.assertTrue(
            any(
                "maximum_weld_separation_height_ratio" in item
                for item in opened_seam["violations"]
            )
        )
        filtered_spike = recipe.deformation_quality_report(
            rest_edge_lengths=[0.001, 1.0],
            posed_edge_lengths=[100.0, 1.0],
            weld_separations_m=[],
            height_m=1.72,
        )
        self.assertFalse(filtered_spike["passed"])
        self.assertEqual(filtered_spike["filtered_short_edge_count"], 1)
        self.assertTrue(
            any(
                "maximum_filtered_edge_posed_height_ratio" in item
                for item in filtered_spike["violations"]
            )
        )

    def test_glb_contract_accepts_rigged_master_and_static_frame(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            rigged = root / "rigged.glb"
            actions = list(recipe.expected_action_names())
            durations = recipe.expected_action_durations_s()
            write_glb(
                rigged,
                valid_glb_document(rigged=True, animation_durations=durations),
            )
            contract = recipe.validate_glb_contract(
                rigged,
                require_skin=True,
                require_animations=True,
                expected_animation_names=actions,
            )
            self.assertEqual(contract["skin_count"], 1)
            self.assertEqual(contract["animation_names"], sorted(actions))

            static = root / "static.glb"
            write_glb(
                static,
                valid_glb_document(rigged=False),
            )
            contract = recipe.validate_glb_contract(
                static,
                require_skin=False,
                require_animations=False,
            )
            self.assertEqual(contract["skin_count"], 0)

    def test_glb_contract_fails_closed_when_skin_or_action_is_missing(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "bad.glb"
            write_glb(
                path,
                valid_glb_document(rigged=False),
            )
            with self.assertRaisesRegex(ValueError, "skin contract"):
                recipe.validate_glb_contract(
                    path,
                    require_skin=True,
                    require_animations=True,
                    expected_animation_names=recipe.expected_action_names(),
                )

    def test_glb_contract_validates_zero_based_action_durations(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "timed.glb"
            durations = recipe.expected_action_durations_s()
            names = list(durations)
            document = valid_glb_document(
                rigged=True,
                animation_durations=durations,
            )
            write_glb(path, document)
            contract = recipe.validate_glb_contract(
                path,
                require_skin=True,
                require_animations=True,
                expected_animation_names=names,
                expected_animation_durations_s=durations,
            )
            self.assertAlmostEqual(
                contract["animation_spans_s"][names[0]]["duration_s"],
                durations[names[0]],
                places=7,
            )

            first_input = document["animations"][0]["samplers"][0]["input"]
            document["accessors"][first_input]["min"] = [1.0 / 30.0]
            write_glb(path, document)
            with self.assertRaisesRegex(ValueError, "starts at"):
                recipe.validate_glb_contract(
                    path,
                    require_skin=True,
                    require_animations=True,
                    expected_animation_names=names,
                    expected_animation_durations_s=durations,
                )

    def test_glb_contract_rejects_dangling_mesh_skin_action_and_atlas(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "dangling.glb"
            durations = recipe.expected_action_durations_s()
            names = list(durations)

            document = valid_glb_document(
                rigged=True,
                animation_durations=durations,
            )
            document["animations"][0]["channels"] = []
            write_glb(path, document)
            with self.assertRaisesRegex(ValueError, "no samplers or channels"):
                recipe.validate_glb_contract(
                    path,
                    require_skin=True,
                    require_animations=True,
                    expected_animation_names=names,
                )

            document = valid_glb_document(rigged=False)
            document["meshes"][0]["primitives"] = []
            write_glb(path, document)
            with self.assertRaisesRegex(ValueError, "no primitives"):
                recipe.validate_glb_contract(
                    path,
                    require_skin=False,
                    require_animations=False,
                )

            document = valid_glb_document(rigged=False)
            document["materials"][0] = {}
            write_glb(path, document)
            with self.assertRaisesRegex(ValueError, "atlas chain"):
                recipe.validate_glb_contract(
                    path,
                    require_skin=False,
                    require_animations=False,
                )


if __name__ == "__main__":
    unittest.main(verbosity=2)
