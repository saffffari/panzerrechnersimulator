#!/usr/bin/env python3
"""Offline contract tests for the reviewed AdrielCZ Panzer crew bake."""

from __future__ import annotations

import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import tiger_crew_recipe as recipe  # noqa: E402


def write_glb(path: Path, document: dict) -> None:
    encoded = json.dumps(document, separators=(",", ":")).encode("utf-8")
    encoded += b" " * ((4 - len(encoded) % 4) % 4)
    total = 12 + 8 + len(encoded)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(
        struct.pack("<4sII", b"glTF", 2, total)
        + struct.pack("<II", len(encoded), 0x4E4F534A)
        + encoded
    )


def glb_document(*, triangles: int, rigged: bool = False, node_name: str = "Crew") -> dict:
    attributes = {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2}
    accessors = [
        {"count": 3, "min": [-0.2, 0.0, -0.1], "max": [0.2, 1.53, 0.1]},
        {"count": 3},
        {"count": 3},
        {"count": triangles * 3},
    ]
    nodes = [{"name": node_name, "mesh": 0}]
    document = {
        "asset": {"version": "2.0"},
        "accessors": accessors,
        "bufferViews": [{}],
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
        nodes.append({"name": "ROOT_N1"})
        document["skins"] = [{"joints": [1], "inverseBindMatrices": 6}]
    return document


class TigerCrewRecipeTests(unittest.TestCase):
    def test_reviewed_topology_and_required_identity_pose_matrix(self):
        self.assertEqual(recipe.CHARACTER_VERTICES, 306)
        self.assertEqual(recipe.CHARACTER_TRIANGLES, 564)
        self.assertEqual(len(recipe.ARMATURE_BONES), 39)
        self.assertEqual(recipe.IDENTITIES, ("face01", "face02", "face03", "face04", "face05"))
        self.assertEqual(set(recipe.POSES), {"standing", "seated", "relaxed"})

    def test_static_glb_requires_one_embedded_base_color_material(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "crew.glb"
            document = glb_document(triangles=recipe.CHARACTER_TRIANGLES)
            write_glb(path, document)
            summary = recipe.validate_static_runtime_glb(
                path,
                expected_triangles=recipe.CHARACTER_TRIANGLES,
            )
            self.assertEqual(summary["material_count"], 1)
            self.assertEqual(summary["bounds_min_m"][1], 0.0)

            document["materials"].append({})
            write_glb(path, document)
            with self.assertRaisesRegex(ValueError, "exactly one material"):
                recipe.validate_static_runtime_glb(path, expected_triangles=recipe.CHARACTER_TRIANGLES)

    def test_runtime_rejects_external_image_skin_and_bundled_tiger(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "crew.glb"
            external = glb_document(triangles=recipe.CHARACTER_TRIANGLES)
            external["images"] = [{"uri": "atlas.png"}]
            write_glb(path, external)
            with self.assertRaisesRegex(ValueError, "embedded"):
                recipe.validate_static_runtime_glb(path, expected_triangles=recipe.CHARACTER_TRIANGLES)

            skinned = glb_document(triangles=recipe.CHARACTER_TRIANGLES, rigged=True)
            write_glb(path, skinned)
            with self.assertRaisesRegex(ValueError, "skins"):
                recipe.validate_static_runtime_glb(path, expected_triangles=recipe.CHARACTER_TRIANGLES)

            tiger = glb_document(
                triangles=recipe.CHARACTER_TRIANGLES,
                node_name="TIGER_H1_Hull",
            )
            write_glb(path, tiger)
            with self.assertRaisesRegex(ValueError, "leaked bundled Tiger"):
                recipe.validate_static_runtime_glb(path, expected_triangles=recipe.CHARACTER_TRIANGLES)

    def test_rigged_master_requires_skin_weights(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "rigged.glb"
            write_glb(path, glb_document(triangles=recipe.CHARACTER_TRIANGLES, rigged=True))
            summary = recipe.validate_rigged_master_glb(path)
            self.assertEqual(summary["skin_count"], 1)

            document = glb_document(triangles=recipe.CHARACTER_TRIANGLES, rigged=True)
            del document["meshes"][0]["primitives"][0]["attributes"]["WEIGHTS_0"]
            write_glb(path, document)
            with self.assertRaisesRegex(ValueError, "skin weights"):
                recipe.validate_rigged_master_glb(path)

    def test_receipt_rehashes_complete_matrix_and_rejects_stale_or_extra_glb(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            rigged = root / "authoring/german_panzertruppe_rigged.glb"
            write_glb(rigged, glb_document(triangles=recipe.CHARACTER_TRIANGLES, rigged=True))
            outputs = []
            identities = {}
            for identity in recipe.IDENTITIES:
                poses = {}
                for pose in recipe.POSES:
                    relative = f"baked/german_panzertruppe_{identity}_{pose}_static.glb"
                    path = root / relative
                    write_glb(path, glb_document(triangles=recipe.CHARACTER_TRIANGLES))
                    summary = recipe.validate_static_runtime_glb(
                        path,
                        expected_triangles=recipe.CHARACTER_TRIANGLES,
                    )
                    record = recipe.file_record(path, logical_path=relative)
                    record.update(summary)
                    record.update({"role": "static_character_pose", "identity": identity, "pose": pose})
                    outputs.append(record)
                    poses[pose] = relative
                identities[identity] = {"poses": poses}
            prop_relative = "baked/german_field_binoculars_physical.glb"
            prop = root / prop_relative
            write_glb(prop, glb_document(triangles=recipe.BINOCULAR_TRIANGLES))
            prop_summary = recipe.validate_static_runtime_glb(
                prop,
                expected_triangles=recipe.BINOCULAR_TRIANGLES,
            )
            prop_record = recipe.file_record(prop, logical_path=prop_relative)
            prop_record.update(prop_summary)
            prop_record["role"] = "physical_binoculars"
            outputs.append(prop_record)
            manifest = {
                "schema": "kessel.tiger-crew-runtime-manifest",
                "version": 1,
                "identities": identities,
                "props": {"binoculars": prop_relative},
                "first_person_binocular_frames": {},
            }
            manifest_path = root / "baked/pose_manifest.v1.json"
            manifest_path.write_text(recipe.stable_json(manifest), encoding="utf-8")
            report = {
                "schema": "kessel.tiger-crew-bake",
                "version": 1,
                "rigged_master": recipe.file_record(
                    rigged,
                    logical_path="authoring/german_panzertruppe_rigged.glb",
                ),
                "runtime_manifest": recipe.file_record(
                    manifest_path,
                    logical_path="baked/pose_manifest.v1.json",
                ),
                "runtime_outputs": outputs,
            }
            verification = recipe.verify_bake_receipt(root, report)
            self.assertEqual(verification["runtime_output_count"], 16)

            outputs[0]["path"] = "../escape.glb"
            with self.assertRaisesRegex(ValueError, "manifest"):
                recipe.verify_bake_receipt(root, report)
            outputs[0]["path"] = next(iter(identities[recipe.IDENTITIES[0]]["poses"].values()))

            extra = root / "baked/stale.glb"
            write_glb(extra, glb_document(triangles=recipe.CHARACTER_TRIANGLES))
            with self.assertRaisesRegex(ValueError, "unexpected or missing GLBs"):
                recipe.verify_bake_receipt(root, report)

    def test_safe_relative_path_rejects_escape(self):
        with self.assertRaises(ValueError):
            recipe.safe_relative_path("../outside.glb")
        with self.assertRaises(ValueError):
            recipe.safe_relative_path("/absolute.glb")


if __name__ == "__main__":
    unittest.main()
