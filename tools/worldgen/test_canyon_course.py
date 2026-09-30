#!/usr/bin/env python3
"""Offline regression tests for tools/worldgen/canyon_course.py (the CANYON playtest course).

Writes the course twice into target/canyon_course_test/ and checks: byte-identical reruns; a start
and a goal the game can read (the Spawn and Goal markers and the canyon_course block); a ravine of
2-3 km with a floor 150-400 m wide and three chokes; walls the Tiger cannot climb (no path from the
start to the plateau avoids a face steeper than its combined pitch and tipping limits, except through
the goal) while a route under 35 deg reaches the goal; buildings inside the grammar's limits, on flat
pads, apart and off the route; a defence in depth (candidate gun sites at every choke, beats clear of
buildings, defenders rising toward the goal); and every referenced asset on disk.

    python3 tools/worldgen/test_canyon_course.py        (about 30 s; needs assets/)
"""

from __future__ import annotations

import hashlib
import importlib.util
import json
import math
import shutil
import sys
import unittest
from pathlib import Path

import numpy as np

REPO_ROOT = Path(__file__).resolve().parents[2]
TOOL = Path(__file__).with_name("canyon_course.py")
sys.path.insert(0, str(TOOL.parent))
SPEC = importlib.util.spec_from_file_location("kessel_canyon_course", TOOL)
CC = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(CC)

OUT = REPO_ROOT / "target" / "canyon_course_test"
STATE = {}


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def setUpModule():
    if OUT.exists():
        shutil.rmtree(OUT)
    for run in ("a", "b"):
        dest, meas = CC.write(OUT / run)
        STATE[run] = (dest, meas)
    STATE["built"] = CC.build()


class CanyonCourseTests(unittest.TestCase):
    def outputs(self, run):
        return STATE[run][0]

    def test_the_workshop_is_deterministic(self):
        for name in CC.OUTPUT_FILES + [CC.RECEIPT_NAME]:
            self.assertEqual(sha(self.outputs("a") / name), sha(self.outputs("b") / name), name)

    def test_the_start_and_the_goal_exist_and_the_game_can_read_them(self):
        manifest = json.loads((self.outputs("a") / "manifest.v1.json").read_text())
        edits = json.loads((self.outputs("a") / "manifest_edits.json").read_text())
        self.assertEqual((manifest["schema"], manifest["version"], manifest["act_id"]), ("kessel.act", 1, 1))
        self.assertEqual(manifest["authoring"]["mode"], "course")
        kinds = [m["kind"] for m in manifest["markers"]]
        self.assertEqual(kinds.count("Spawn"), 1)
        self.assertEqual(kinds.count("Goal"), 1)
        self.assertEqual(edits["base_placements"], len(manifest["placements"]))
        self.assertEqual(edits["base_markers"], len(manifest["markers"]))
        course = edits["canyon_course"]
        self.assertEqual((course["schema"], course["version"]), ("kessel.canyon-course", 1))
        start, goal = course["start"], course["goal"]
        spawn = [m for m in manifest["markers"] if m["kind"] == "Spawn"][0]
        self.assertAlmostEqual(spawn["x"], start["x"], places=2)
        self.assertGreater(math.hypot(goal["x"] - start["x"], goal["z"] - start["z"]), 2000.0)
        heights = np.load(self.outputs("a") / "heights.npy")
        self.assertEqual(heights.shape, (CC.RES, CC.RES))
        self.assertEqual(manifest["terrain"]["res"], CC.RES)
        # the start lies on the ravine's floor, well inside its walls
        rv, grid, site, H, fields = STATE["built"][:5]
        self.assertLess(grid.sample(fields["E"], start["x"], start["z"]), -40.0)

    def test_the_ravine_is_two_to_three_km_long_and_150_to_400_m_wide(self):
        meas = STATE["a"][1]
        self.assertTrue(2000.0 <= meas["length_m"] <= 3000.0, meas["length_m"])
        rv = STATE["built"][0]
        # the floor proper; the ramp up to the head narrows to 70-120 m (the exit gully)
        widths = [float(rv.width(f)) for f in np.linspace(0.0, 0.92, 400)]
        self.assertGreaterEqual(min(widths), 150.0 - 1e-6)
        self.assertLessEqual(max(widths), 400.0)
        self.assertEqual([c["width_m"] for c in meas["chokes"]], [150.0, 150.0, 150.0])
        # the walls are high: the plateau stands 15-45 m over the floor along the playable length
        self.assertGreaterEqual(meas["depth_m"]["min"], 15.0)
        self.assertLessEqual(meas["depth_m"]["max"], 50.0)

    def test_the_walls_cannot_be_climbed_but_the_goal_can_be_reached(self):
        meas = STATE["a"][1]
        self.assertAlmostEqual(meas["unclimbable_deg"], 58.65, places=1)
        self.assertEqual(meas["plateau_reached_without_the_goal"], 0)
        self.assertTrue(meas["goal_reached_at_35_deg"])
        # every cross-section of the walls holds a face steeper than the combined limit
        rv, grid, site, H, fields = STATE["built"][:5]
        steep = CC.face_gradient(H, grid) > math.tan(math.radians(CC.UNCLIMBABLE_DEG))
        for f in np.linspace(0.02, CC.RAMP_START - 0.02, 60):
            for side in (-1, 1):
                hw = float(rv.half_width(np.array(f), side))
                cells = [grid.sample(steep.astype(float), *rv.world(f, side * (hw + e))) for e in np.arange(0.0, 60.0, 1.5)]
                self.assertTrue(any(c > 0.5 for c in cells), f"no unclimbable face at {f:.3f} side {side}")

    def test_buildings_are_valid_flat_apart_and_off_the_route(self):
        rv, grid, site, H, fields, Dr, street, course, instances = STATE["built"][:9]
        limits = CC.uid.read_rust_limits()
        self.assertGreaterEqual(len(instances), 30)
        groups = {b["group"] for b in site.buildings}
        self.assertTrue({"settlement", "factory", "strongpoint"} <= groups)
        typologies = {i["typology"] for i in instances}
        self.assertEqual(typologies, {"wooden_house", "merchant_brick_house", "apartment_block", "workshop"})
        for inst in instances:
            self.assertEqual(CC.uid.within_limits(inst, limits[inst["typology"]]), [], inst["id"])
        rects = [b["rect"] for b in site.buildings]
        for i, a in enumerate(rects):
            for b in rects[i + 1:]:
                self.assertGreaterEqual(CC.rect_gap(a, b), CC.BUILDING_GAP_M - 1e-6)
        for b in site.buildings:
            if b["razed"]:
                continue
            x0, x1, z0, z1 = b["rect"]
            hs = [grid.sample(H, x, z) for x in np.linspace(x0, x1, 6) for z in np.linspace(z0, z1, 6)]
            self.assertLess(max(hs) - min(hs), 0.12, b["id"])
            self.assertLess(abs(np.mean(hs) - b["pad_m"]), 0.1, b["id"])
            for x in (x0, x1):
                for z in (z0, z1):
                    self.assertGreaterEqual(abs(grid.sample(fields["D"], x, z)), CC.ROUTE_LANE_HALF_M - 0.5, b["id"])

    def test_the_defence_is_in_depth(self):
        rv, grid, site, H, fields, Dr, street, course, instances = STATE["built"][:9]
        meas = STATE["a"][1]
        self.assertEqual([z["id"] for z in course["gun_zones"]], ["choke1", "choke2", "choke3"])
        ids = {line["id"] for line in street["kill_zone"]}
        for zone in course["gun_zones"]:
            self.assertIn(zone["kill_zone"], ids)
            self.assertGreaterEqual(len(zone["candidates"]), 20, zone["id"])
            for i in zone["candidates"]:
                self.assertLess(i, len(street["gun_sites"]))
        self.assertIn("route", {line["role"] for line in street["kill_zone"]})
        self.assertEqual([b["platoon"] for b in street["beats"]], ["A", "B"])
        for beat in street["beats"]:
            for (x, z) in beat["points"]:
                self.assertTrue(all(CC.uid.rect_dist(r, x, z) > 8.0 for r in site.rects), (beat["platoon"], x, z))
                self.assertLess(grid.sample(fields["E"], x, z), -10.0)
        self.assertEqual(len(course["garrison"]), 6)
        # tanks plus guns by thirds of the length, rising toward the goal
        thirds = meas["defenders_by_third_tanks_plus_guns"]
        self.assertTrue(thirds[0] < thirds[1] < thirds[2], thirds)

    def test_every_referenced_asset_is_on_disk(self):
        manifest = json.loads((self.outputs("a") / "manifest.v1.json").read_text())
        for path in manifest["assets"] + [manifest["terrain"]["texture"]]:
            self.assertTrue((REPO_ROOT / path).is_file(), path)
        self.assertGreater(len(manifest["placements"]), 300)


if __name__ == "__main__":
    unittest.main(verbosity=2)
