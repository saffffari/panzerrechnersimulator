#!/usr/bin/env python3
"""Offline tests for the crew anatomy tools: the KVOX format, the editor's codec and editing model
(run in Node, the same code the page runs), reading hand edits back into the build, and the
tissue table's invariants.

    python3 -I tools/anatomy/test_crew_anatomy.py

With KESSEL_ASSET_ROOT set and the volume built, it also checks the built volume against the
tracked receipt.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import build_anatomy  # noqa: E402
import kvox  # noqa: E402

REPO = HERE.parents[1]
TABLE = REPO / "catalog/crew_anatomy_tissues.v1.json"
EDITOR = HERE / "voxel_editor.html"


def synthetic(dims=(9, 9, 7)):
    nx, ny, nz = dims
    labels = np.zeros((ny, nz, nx), dtype=np.uint8)
    labels[1:-1, 1:-1, 1:-1] = 3
    labels[2:-2, 2:-2, 2:-2] = 4
    bone = np.where(labels > 0, 7, 255).astype(np.uint8)
    header = {"format": kvox.FORMAT, "version": 1, "dims": list(dims), "voxel_size_m": 0.01,
              "origin_m": [-0.035, 0.0, -0.025],
              "channels": [{"name": "label"}, {"name": "bone"}],
              "frame": {"up": "+Y"}, "labels": [{"id": 3, "key": "muscle"}], "edits": []}
    return header, {"label": labels, "bone": bone}


NODE_DRIVER = r"""
const fs = require("fs");
const html = fs.readFileSync(process.argv[2], "utf8");
const m = html.match(/<script id="kvox-codec">([\s\S]*?)<\/script>/);
if (!m) throw new Error("no codec script in the editor");
const KVOX = new Function(m[1] + "\nreturn KVOX;")();
const buf = fs.readFileSync(process.argv[3]);
const parsed = KVOX.parse(buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength));
const model = new KVOX.Model(parsed);
const [x, y, z] = JSON.parse(process.argv[5]);
const before = model.get(x, y, z);
// a stroke; then a second stroke limited to label 4, undone, redone and undone again
model.beginStroke(); const a = model.paint(x, y, z, 1, 9, "ball", null); model.endStroke();
model.beginStroke(); const b = model.paint(x, y, z, 2, 11, "y", 4); model.endStroke();
const centre = model.get(x, y, z), stamped = model.get(x + 2, y, z);
model.undo();
const afterUndo = model.get(x + 2, y, z);
model.redo(); model.undo();
model.save("test_crew_anatomy.py", "2026-11-02T00:00:00Z").then(bytes => {
  fs.writeFileSync(process.argv[4], bytes);
  console.log(JSON.stringify({before, a, b, centre, stamped, afterUndo, total: model.diff().total, undo: model.undoStack.length,
                              redo: model.redoStack.length}));
});
"""


class FormatTests(unittest.TestCase):
    def test_kvox_round_trip_keeps_channels_and_header(self):
        header, channels = synthetic()
        data = kvox.encode(header, channels)
        self.assertEqual(data[:8], b"KVOXANAT")
        self.assertEqual((16 + int.from_bytes(data[12:16], "little")) % 8, 0, "payloads start 8-byte aligned")
        back, ch = kvox.decode(data)
        self.assertEqual(back["dims"], [9, 9, 7])
        self.assertTrue(np.array_equal(ch["label"], channels["label"]))
        self.assertTrue(np.array_equal(ch["bone"], channels["bone"]))
        self.assertEqual(back["channels"][0]["payload_sha256"], kvox.payload_sha256(channels["label"]))
        # index order: x fastest, then z, then y
        n = 9 * 9 * 7
        flat = np.frombuffer(data[-2 * n:-n], dtype=np.uint8)
        y, z, x = 4, 2, 3
        self.assertEqual(flat[(y * 7 + z) * 9 + x], channels["label"][y, z, x])

    def test_a_corrupted_payload_is_refused(self):
        header, channels = synthetic()
        data = bytearray(kvox.encode(header, channels))
        data[-1] ^= 0xFF
        with self.assertRaises(ValueError):
            kvox.decode(bytes(data))


@unittest.skipUnless(shutil.which("node"), "node is not installed")
class EditorTests(unittest.TestCase):
    def test_the_editor_codec_paints_undoes_and_saves_a_volume_python_reads(self):
        header, channels = synthetic()
        with tempfile.TemporaryDirectory() as tmp:
            src, out, driver = Path(tmp, "in.kvox"), Path(tmp, "out.kvox"), Path(tmp, "driver.js")
            kvox.write(src, header, channels)
            driver.write_text(NODE_DRIVER, encoding="utf-8")
            centre = [4, 4, 3]
            run = subprocess.run(["node", str(driver), str(EDITOR), str(src), str(out), json.dumps(centre)],
                                 capture_output=True, text=True, timeout=60)
            self.assertEqual(run.returncode, 0, run.stderr)
            report = json.loads(run.stdout)
            self.assertEqual(report["before"], 4)
            self.assertEqual(report["centre"], 9, "the second stroke leaves the first stroke's label 9 alone")
            self.assertEqual(report["stamped"], 11, "and paints over label 4")
            self.assertEqual(report["afterUndo"], 4, "undo restores it")
            self.assertEqual((report["undo"], report["redo"]), (1, 1))
            saved_header, saved = kvox.read(out)
            # the first stroke (a radius-1 ball of label 9) survives, the undone one does not
            expected = channels["label"].copy()
            x, y, z = centre
            for dy in (-1, 0, 1):
                for dz in (-1, 0, 1):
                    for dx in (-1, 0, 1):
                        if dx * dx + dy * dy + dz * dz <= 2:
                            expected[y + dy, z + dz, x + dx] = 9
            self.assertTrue(np.array_equal(saved["label"], expected))
            self.assertEqual(report["total"], int((expected != channels["label"]).sum()))
            self.assertTrue(np.array_equal(saved["bone"], channels["bone"]), "other channels are kept")
            self.assertEqual(saved_header["frame"], header["frame"], "the header is carried through")
            self.assertEqual(saved_header["edits"][-1]["voxels_changed"], report["total"])
            # and the build reads the edits back as an edit layer that reproduces the edited volume
            layer = Path(tmp, "edits.json")
            doc = build_anatomy.capture_edits(channels["label"], out, kvox.payload_sha256(channels["label"]), layer, json.loads(TABLE.read_text()))
            self.assertEqual(len(doc["edits"]), report["total"])
            rebuilt = channels["label"].copy()
            info = build_anatomy.apply_edits(rebuilt, layer, kvox.payload_sha256(channels["label"]))
            self.assertTrue(info["base_matches"])
            self.assertTrue(np.array_equal(rebuilt, saved["label"]))


class TableTests(unittest.TestCase):
    def setUp(self):
        self.table = json.loads(TABLE.read_text(encoding="utf-8"))

    def test_every_label_has_physical_properties_and_a_known_class(self):
        ids = [r["id"] for r in self.table["labels"]]
        self.assertEqual(ids, sorted(set(ids)), "ids unique and ordered")
        self.assertTrue(all(0 <= i <= 48 for i in ids))
        keys = {r["id"]: r["key"] for r in self.table["labels"]}
        # the game reads these three by id
        self.assertEqual((keys[1], keys[3], keys[4]), ("skin", "muscle", "cortical_bone"))
        for r in self.table["labels"]:
            self.assertGreater(r["density_kg_m3"], 0.0, r["key"])
            self.assertTrue(r["density_source"], r["key"])
            self.assertIn(r["strength_class"], self.table["strength_classes"], r["key"])
            self.assertTrue(0.0 <= r["blood_volume_fraction"] <= 1.0, r["key"])
            self.assertRegex(r["colour"], r"^#[0-9a-f]{6}$")

    def test_strength_classes_are_crew_tissues_authored_values(self):
        c = self.table["strength_classes"]
        got = {k: (c[k]["tear_energy_density_j_m3"], c[k]["critical_stretch"]) for k in ("skin", "muscle", "bone", "fluid")}
        self.assertEqual(got, {"skin": (50_000, 0.6), "muscle": (125_000, 0.35), "bone": (400_000, 0.05), "fluid": (12_500, 1.0)})

    def test_blood_fractions_follow_from_their_recorded_inputs(self):
        blood_l = self.table["blood"]["reference_blood_volume_l"]
        for r in self.table["labels"]:
            d = r["blood_derivation"]
            if "icrp89_share_percent" not in d:
                continue
            vol = d.get("reference_volume_l")
            self.assertAlmostEqual(r["blood_volume_fraction"], d["icrp89_share_percent"] / 100.0 * blood_l / vol, places=3, msg=r["key"])

    def test_the_classifier_port_matches_crew_physics(self):
        # points straight from crew_physics::classify_relaxed_mesh_point's thresholds
        cases = [((0.0, 1.40), "Head"), ((-0.2, 1.0), "LeftUpperArm"), ((-0.2, 0.5), "LeftForearm"), ((0.2, 0.9), "RightUpperArm"),
                 ((0.05, 0.5), "RightThigh"), ((-0.05, 0.2), "LeftShin"), ((0.0, 0.75), "Pelvis"), ((0.0, 1.0), "Torso")]
        for (x, y), want in cases:
            self.assertEqual(build_anatomy.CREW_PARTS[int(build_anatomy.classify_relaxed(x, y))], want, (x, y))


class BuiltVolumeTests(unittest.TestCase):
    """With the assets present: the built volume matches the tracked receipt."""

    def test_the_built_volume_matches_its_receipt(self):
        root = os.environ.get("KESSEL_ASSET_ROOT")
        receipt_path = REPO / "catalog/crew_anatomy_volume.v1.json"
        if not root or not receipt_path.exists():
            self.skipTest("KESSEL_ASSET_ROOT not set or no receipt")
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        path = Path(root) / receipt["volume"]["path"]
        if not path.exists():
            self.skipTest(f"{path} not built")
        header, channels = kvox.read(path)
        self.assertEqual(kvox.payload_sha256(channels["label"]), receipt["volume"]["label_sha256"])
        self.assertEqual(kvox.payload_sha256(channels["bone"]), receipt["volume"]["bone_sha256"])
        self.assertEqual(header["dims"], receipt["volume"]["dims"])
        table = json.loads(TABLE.read_text(encoding="utf-8"))
        chk = build_anatomy.checks(channels["label"], table, channels["bone"])
        self.assertEqual(chk["non_skin_voxels_touching_exterior"], 0, "the skin shell is closed")
        self.assertFalse(any(chk["organ_voxels_touching_skin"].values()), chk["organ_voxels_touching_skin"])
        self.assertEqual(chk["brain_voxels_beside_non_cranial_tissue"], 0)
        self.assertEqual(chk["voxels_without_bone_binding"], 0)
        for organ, count in (("brain", 1), ("kidney", 2), ("eye", 2), ("heart (wall + blood)", 1), ("spinal_cord", 1),
                             ("stomach (wall + contents)", 1), ("colon (wall + contents)", 1), ("bladder (wall + urine)", 1)):
            self.assertEqual(chk["components"][organ]["count"], count, organ)


if __name__ == "__main__":
    unittest.main()
