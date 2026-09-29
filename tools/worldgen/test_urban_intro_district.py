#!/usr/bin/env python3
"""Offline regression tests for tools/worldgen/urban_intro_district.py.

Runs the district tool twice into target/urban_intro_test/ and checks the outputs: byte-identical
reruns, an untouched source workshop, footprints that touch but never overlap, every instance
inside the typology limits (a Python mirror, cross-checked against src/building_grammar.rs), pads
flat within tolerance, streets clear of buildings and drivable, the courtyard, spawn and mission
points kept clear, R1's typology and condition shares, and dressing from files on disk.

    python3 tools/worldgen/test_urban_intro_district.py        (about a minute; needs assets/)
"""

from __future__ import annotations

import hashlib
import importlib.util
import json
import math
import re
import shutil
import subprocess
import sys
import unittest
from pathlib import Path

import numpy as np

REPO_ROOT = Path(__file__).resolve().parents[2]
TOOL = Path(__file__).with_name("urban_intro_district.py")
SPEC = importlib.util.spec_from_file_location("kessel_urban_intro_district", TOOL)
UID = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(UID)

OUT = REPO_ROOT / "target" / "urban_intro_test"
RUNS = {}

# The contract's limits() at commit 904ea9b, mirrored. test_limits_mirror_matches_building_grammar
# fails when src/building_grammar.rs changes, so the mirror and the tool are revisited together.
LIMITS_MIRROR = {
    "merchant_brick_house": dict(width=(8.0, 24.0), depth=(8.0, 16.0), storeys=(1, 2), bay=4.0, cellar=True),
    "apartment_block": dict(width=(16.0, 64.0), depth=(12.0, 16.0), storeys=(3, 5), bay=4.0, cellar=True),
    "wooden_house": dict(width=(8.0, 12.0), depth=(8.0, 8.0), storeys=(1, 1), bay=4.0, cellar=True),
    "workshop": dict(width=(8.0, 32.0), depth=(8.0, 16.0), storeys=(1, 1), bay=4.0, cellar=False),
}
SHARED_WALL_TOLERANCE_M = 0.01   # building_placement::SHARED_WALL_TOLERANCE_M
SEMANTIC_ID = re.compile(r"^[a-z][a-z0-9_-]*(\.[a-z][a-z0-9_-]*)*$")


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_tool(dest: Path):
    subprocess.run([sys.executable, str(TOOL), "--dest", str(dest), "--force"], cwd=REPO_ROOT, check=True,
                   stdout=subprocess.DEVNULL)


def output_hashes(dest: Path):
    return {name: sha(dest / name) for name in UID.OUTPUT_FILES + [UID.RECEIPT_NAME]}


def setUpModule():
    if not (REPO_ROOT / UID.SOURCE_REL / "manifest.v1.json").is_file():
        raise unittest.SkipTest("untracked assets/ payload is not installed")
    a = OUT / "run"
    if a.exists():
        shutil.rmtree(a)
    run_tool(a)
    RUNS["first"] = output_hashes(a)
    run_tool(a)   # the same command again, over its own output
    RUNS["second"] = output_hashes(a)
    RUNS["dir"] = a
    RUNS["edits"] = json.loads((a / "manifest_edits.json").read_text())
    RUNS["manifest"] = json.loads((a / "manifest.v1.json").read_text())
    RUNS["features"] = json.loads((a / "features.json").read_text())[UID.EDITS_KEY]
    RUNS["receipt"] = json.loads((a / UID.RECEIPT_NAME).read_text())
    RUNS["heights"] = np.load(a / "heights.npy").astype(np.float64)
    RUNS["source_heights"] = np.load(REPO_ROOT / UID.SOURCE_REL / "heights.npy").astype(np.float64)
    t = RUNS["manifest"]["terrain"]
    RUNS["grid"] = UID.Grid(int(t["res"]), float(t["spacing"]))


def footprint(inst):
    return UID.footprint_of_instance(inst)


def overlap(a, b, tol):
    return a[0] < b[1] - tol and b[0] < a[1] - tol and a[2] < b[3] - tol and b[2] < a[3] - tol


def within(inst, lim):
    errs = []
    for label, side, (lo, hi) in (("width", inst["width_m"], lim["width"]), ("depth", inst["depth_m"], lim["depth"])):
        if not (lo - 1e-3 <= side <= hi + 1e-3):
            errs.append(f"{label} {side} outside {lo}..{hi}")
        bays = side / lim["bay"]
        if abs(bays - round(bays)) >= 1e-3 or round(bays) < 1:
            errs.append(f"{label} {side} is not whole {lim['bay']} m bays")
    if not (lim["storeys"][0] <= inst["storeys"] <= lim["storeys"][1]):
        errs.append(f"storeys {inst['storeys']} outside {lim['storeys']}")
    if inst["cellar"] and not lim["cellar"]:
        errs.append("cellar not allowed")
    return errs


class UrbanIntroDistrictTests(unittest.TestCase):
    def test_rerun_is_byte_identical(self):
        for name, digest in RUNS["first"].items():
            with self.subTest(file=name):
                self.assertEqual(digest, RUNS["second"][name], f"{name} differs between runs")
        recorded = RUNS["receipt"]["outputs"]["sha256"]
        self.assertEqual(set(recorded), set(UID.OUTPUT_FILES))
        for name, digest in recorded.items():
            self.assertEqual(digest, sha(RUNS["dir"] / name), f"receipt hash of {name} is stale")

    def test_source_workshop_is_not_modified(self):
        src = REPO_ROOT / UID.SOURCE_REL
        for name, digest in RUNS["receipt"]["source"]["sha256"].items():
            self.assertEqual(sha(src / name), digest, f"source {name} changed")

    def test_instances_are_well_formed_and_footprints_never_overlap(self):
        insts = RUNS["edits"]["building_instances"]
        self.assertGreater(len(insts), 100)
        ids = [i["id"] for i in insts]
        self.assertEqual(len(ids), len(set(ids)))
        keys = {"id", "typology", "seed", "width_m", "depth_m", "storeys", "cellar", "condition", "x", "z", "yaw_deg"}
        for inst in insts:
            self.assertEqual(set(inst), keys)
            self.assertRegex(inst["id"], SEMANTIC_ID)
            self.assertLessEqual(len(inst["id"]), 128)
            self.assertIn(inst["typology"], UID.TYPOLOGIES)
            self.assertIn(inst["condition"], UID.CONDITIONS)
            self.assertAlmostEqual((inst["yaw_deg"] / 90.0) % 1.0, 0.0, places=6)
            self.assertIsInstance(inst["seed"], int)
        fps = [footprint(i) for i in insts]
        for i in range(len(fps)):
            for j in range(i + 1, len(fps)):
                self.assertFalse(overlap(fps[i], fps[j], SHARED_WALL_TOLERANCE_M),
                                 f"{ids[i]} overlaps {ids[j]}")

    def test_limits_mirror_matches_building_grammar(self):
        rust = UID.read_rust_limits()
        self.assertEqual(rust, LIMITS_MIRROR, "src/building_grammar.rs limits() changed: update the mirror "
                                              "and rerun the district tool")

    def test_every_instance_is_inside_the_limits(self):
        rust = UID.read_rust_limits()
        for inst in RUNS["edits"]["building_instances"]:
            with self.subTest(id=inst["id"]):
                self.assertEqual(within(inst, LIMITS_MIRROR[inst["typology"]]), [])
                self.assertEqual(within(inst, rust[inst["typology"]]), [])

    def test_pads_are_flat_within_tolerance(self):
        H = RUNS["heights"]
        grid = RUNS["grid"]
        plots = {p["id"]: p for p in RUNS["features"]["plots"]}
        worst = 0.0
        for inst in RUNS["edits"]["building_instances"]:
            x0, x1, z0, z1 = footprint(inst)
            pad = plots[inst["id"]]["pad_y"]
            for z in np.arange(z0, z1 + 1e-6, 0.5):
                for x in np.arange(x0, x1 + 1e-6, 0.5):
                    worst = max(worst, abs(grid.sample(H, x, z) - pad))
            self.assertAlmostEqual(grid.sample(H, inst["x"], inst["z"]), pad, delta=UID.PAD_TOLERANCE_M)
        self.assertLessEqual(worst, UID.PAD_TOLERANCE_M)

    def test_streets_and_square_are_clear_of_buildings(self):
        feats = RUNS["features"]
        rects = [s["rect"] for s in feats["streets"]] + [feats["square"]["rect"]]
        road = feats["old_road"]
        for inst in RUNS["edits"]["building_instances"]:
            fp = footprint(inst)
            for r in rects:
                self.assertFalse(overlap(fp, r, SHARED_WALL_TOLERANCE_M), f"{inst['id']} stands in the street {r}")
            self.assertGreaterEqual(UID.rect_seg_dist(fp, road["a"], road["b"]), road["width_m"] / 2,
                                    f"{inst['id']} stands in the old road")

    def test_streets_are_drivable(self):
        """Every street centreline climbs at most 20 deg and crosses at most 25 deg (the Tiger climbs 35,
        R1 §8.1); the one street closed by the fallen facade is the only blocked carriageway."""
        H = RUNS["heights"]
        grid = RUNS["grid"]
        feats = RUNS["features"]
        ridge = feats["fallen_facade_ridge"]["rect"]
        for st in feats["streets"]:
            r = st["rect"]
            ns = st["axis"] == "ns"
            lo, hi = (r[2], r[3]) if ns else (r[0], r[1])
            prev = None
            for t in np.arange(lo + 1.0, hi - 1.0, 2.0):
                x, z = (st["centre"], t) if ns else (t, st["centre"])
                if ridge[0] - 3 <= x <= ridge[1] + 3 and ridge[2] - 3 <= z <= ridge[3] + 3:
                    prev = None
                    continue
                h = grid.sample(H, x, z)
                if prev is not None:
                    self.assertLessEqual(math.degrees(math.atan2(abs(h - prev), 2.0)), 20.0, f"{st['id']} at {t}")
                prev = h
                half = st["width_m"] / 2 - 1.0
                a = grid.sample(H, x - half, z) if ns else grid.sample(H, x, z - half)
                b = grid.sample(H, x + half, z) if ns else grid.sample(H, x, z + half)
                self.assertLessEqual(math.degrees(math.atan2(abs(b - a), 2 * half)), 25.0, f"{st['id']} across at {t}")
        lanes = RUNS["receipt"]["measurements"]["clear_lanes"]
        for lane in lanes:
            if lane["cls"] == "old_road":
                self.assertEqual(lane["hard_objects_in_carriageway"], 0)
            else:
                self.assertEqual(lane["unintended_closed_stations"], 0, lane)
                self.assertGreaterEqual(lane["min_clear_m"], UID.pv("tiger.min_clear_lane_m"), lane)
        self.assertEqual(sum(1 for lane in lanes if lane.get("intended_closure")), 1)

    def test_courtyard_spawn_and_mission_points_stay_clear(self):
        feats = RUNS["features"]
        keep = feats["courtyard_keep"]
        frozen = feats["courtyard_frozen"]
        grid = RUNS["grid"]
        # every sample of every cell that touches the courtyard's pad and apron is the source's
        self.assertLessEqual(frozen[0], keep[0] - grid.sp + 1e-3)
        self.assertGreaterEqual(frozen[3], keep[3] + grid.sp - 1e-3)
        cols = np.where((grid.axis >= frozen[0]) & (grid.axis <= frozen[1]))[0]
        rows = np.where((grid.axis >= frozen[2]) & (grid.axis <= frozen[3]))[0]
        np.testing.assert_array_equal(RUNS["heights"][np.ix_(rows, cols)], RUNS["source_heights"][np.ix_(rows, cols)])
        for x in np.arange(keep[0], keep[1] + 1e-6, 0.5):
            for z in np.arange(keep[2], keep[3] + 1e-6, 0.5):
                self.assertEqual(grid.sample(RUNS["heights"], x, z), grid.sample(RUNS["source_heights"], x, z))
        mission = RUNS["receipt"]["mission"]
        cell = grid.sp
        for inst in RUNS["edits"]["building_instances"]:
            fp = footprint(inst)
            grown = (frozen[0] - cell, frozen[1] + cell, frozen[2] - cell, frozen[3] + cell)
            self.assertFalse(overlap(fp, grown, 0.0), f"{inst['id']} reaches the courtyard")
            for name, radius in (("spawn", UID.SPAWN_CLEAR_BUILDINGS_M), ("wreck", 16.0), ("withdraw", 18.0)):
                self.assertGreaterEqual(UID.rect_dist(fp, *mission[name]), radius - 1e-6, f"{inst['id']} near {name}")
        source_ops = json.loads((REPO_ROOT / UID.SOURCE_REL / "manifest_edits.json").read_text())["ops"]
        ops = RUNS["edits"]["ops"]
        for a, b in zip(source_ops, ops[:len(source_ops)]):
            self.assertEqual({k: v for k, v in a.items() if k != "y"}, {k: v for k, v in b.items() if k != "y"})
            if keep[0] <= a["x"] <= keep[1] and keep[2] <= a["z"] <= keep[3]:
                self.assertEqual(a, b, "a courtyard op moved")
        self.assertEqual(RUNS["edits"]["base_placements"], len(RUNS["manifest"]["placements"]))
        markers = [m for m in RUNS["manifest"]["markers"] if m["kind"] == "Spawn"]
        self.assertEqual(len(markers), 1)

    def test_district_matches_the_research_shares(self):
        m = RUNS["receipt"]["measurements"]
        for t in UID.TYPOLOGIES:
            with self.subTest(typology=t):
                self.assertTrue(m["typology_by_count"][t]["within"], m["typology_by_count"][t])
                self.assertTrue(m["typology_by_area"][t]["within"], m["typology_by_area"][t])
                group = [i for i in RUNS["edits"]["building_instances"] if i["typology"] == t]
                for c in UID.CONDITIONS:
                    n = sum(1 for i in group if i["condition"] == c)
                    self.assertLessEqual(abs(n - UID.CONDITION_SHARES[t][c] * len(group)), 1.0, (t, c, n))
        share = m["buildings"]["razed_share_of_masonry"]
        self.assertGreaterEqual(share, 0.05)
        self.assertLessEqual(share, 0.15)
        for key in ("core.long", "ordinary.long", "fringe.long"):
            lo, hi = UID.RECIPES[key.split(".")[0]]["occupancy"]
            occ = m["frontage_occupancy"][key]["share_of_clear"]
            self.assertTrue(lo - 0.02 <= occ <= hi + 0.02, (key, occ))

    def test_dressing_uses_files_on_disk_within_budget(self):
        source_ops = json.loads((REPO_ROOT / UID.SOURCE_REL / "manifest_edits.json").read_text())["ops"]
        new = [op for op in RUNS["edits"]["ops"][len(source_ops):] if op["op"] == "add"]
        self.assertGreater(len(new), 1000)
        self.assertLessEqual(len(new), UID.OP_BUDGET)
        for path in sorted(set(op["asset"] for op in new)):
            self.assertTrue((REPO_ROOT / path).is_file(), path)
            self.assertIn(path, UID.ASSET.values())
        for op in RUNS["edits"]["ops"]:
            if op["op"] == "set":
                self.assertLess(op["i"], RUNS["edits"]["base_placements"])
        tex = RUNS["manifest"]["terrain"]["texture"]
        self.assertTrue((REPO_ROOT / tex).is_file(), tex)

    def test_tracked_sample_is_current(self):
        """catalog/examples/urban_intro_district.sample.json (validated by the Rust test
        src/urban_district_sample_tests.rs) is exactly what the tool produces now."""
        src, rust, rust_limits, grid, L, H, D, instances = UID.build_all(None)
        text = UID.dumps(UID.sample_document(L, instances))
        self.assertEqual((REPO_ROOT / UID.SAMPLE_REL).read_text(), text,
                         "stale sample: run tools/worldgen/urban_intro_district.py --write-sample")
        sample_ids = {i["id"] for i in json.loads(text)["building_instances"]}
        self.assertTrue(sample_ids <= {i["id"] for i in RUNS["edits"]["building_instances"]})

    def test_encounter_conflicts_and_proposals_are_recorded(self):
        review = RUNS["receipt"]["encounter_review"]
        self.assertEqual(len(review["at_gun_sites"]), 2)
        self.assertEqual(len(review["routes"]), 2)
        proposals = RUNS["receipt"]["proposals"]
        self.assertEqual(sorted(g["side"] for g in proposals["at_guns"]), ["east", "west"])
        for g in proposals["at_guns"]:
            self.assertGreater(g["terrain_clearance_m"], 0.0)
        self.assertTrue(proposals["t34_beat"]["clear"])


if __name__ == "__main__":
    unittest.main()
