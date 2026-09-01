#!/usr/bin/env python3
"""Offline regression tests for gen_world's authored tactical relief."""

from __future__ import annotations

import importlib.util
import math
from pathlib import Path
import unittest

import numpy as np


REPO_ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "kessel_gen_world", Path(__file__).with_name("gen_world.py")
)
GEN = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(GEN)


def blank_world(res: int = 401, spacing: float = 4.0):
    world = GEN.World.__new__(GEN.World)
    world.res = res
    world.spacing = spacing
    world.heights = np.zeros((res, res), np.float32)
    half = (res - 1) * 0.5
    axis = (np.arange(res, dtype=np.float32) - half) * spacing
    world._gx = axis[None, :]
    world._gz = axis[:, None]
    world.tactical = []
    return world


class TacticalTerrainTests(unittest.TestCase):
    def test_ridge_is_deterministic_and_saddle_reopens_the_pass(self):
        kwargs = dict(
            name="test ridge",
            center=(0.0, 0.0),
            angle=math.pi / 2,
            length=1200.0,
            width=80.0,
            amount=6.0,
            gaps=[(240.0, 55.0)],
        )
        first = blank_world()
        second = blank_world()
        first.elongated_stamp(**kwargs)
        second.elongated_stamp(**kwargs)
        np.testing.assert_array_equal(first.heights, second.heights)
        self.assertGreater(first.ground(0.0, 0.0), 5.8)
        self.assertLess(abs(first.ground(0.0, 240.0)), 0.02)
        self.assertLess(first.ground(300.0, 0.0), 0.01)

    def test_every_act_has_two_real_flanking_routes_and_segmented_ridges(self):
        for act_id, spec in GEN.TACTICAL_TERRAIN.items():
            with self.subTest(act=act_id):
                self.assertGreaterEqual(len(spec["routes"]), 2)
                self.assertGreaterEqual(len(spec["los_targets"]), 4)
                self.assertTrue(all(len(ridge["gaps"]) >= 2 for ridge in spec["ridges"]))
                self.assertGreaterEqual(spec["min_los_block_m"], 2.0)

    def test_act_two_roads_have_only_the_authored_bridge_crossing(self):
        world = GEN.World.__new__(GEN.World)
        world.roads = [
            [600.0, -700.0, -800.0, -300.0],
            [700.0, 900.0, 100.0, 200.0],
            [-200.0, 450.0, -1200.0, 520.0],
        ]
        world.funnel_roads_through_crossing(GEN.CANAL_X, GEN.CANAL_BRIDGE_Z)
        bridge = (GEN.CANAL_X, GEN.CANAL_BRIDGE_Z)
        crossing_segments = []
        for x0, z0, x1, z1 in world.roads:
            if (x0 - GEN.CANAL_X) * (x1 - GEN.CANAL_X) <= 0.0:
                crossing_segments.append(((x0, z0), (x1, z1)))
        self.assertEqual(len(crossing_segments), 2)
        self.assertTrue(all(bridge in segment for segment in crossing_segments))

    def test_act_two_channel_meander_is_phase_locked_to_bridge(self):
        world = blank_world()
        world.carve_channel(
            "x", GEN.CANAL_X, 18.0, 3.2,
            meander=GEN.CANAL_MEANDER,
            meander_origin=GEN.CANAL_BRIDGE_Z,
        )
        self.assertAlmostEqual(world.ground(GEN.CANAL_X, GEN.CANAL_BRIDGE_Z), -3.2, places=2)
        self.assertGreater(world.ground(GEN.CANAL_X + 48.0, GEN.CANAL_BRIDGE_Z), -0.01)

    def test_installed_campaign_grids_keep_all_spawn_lanes_blocked(self):
        missing = [
            REPO_ROOT / f"assets/worldgen/act{act_id}/heights.npy"
            for act_id in GEN.TACTICAL_TERRAIN
            if not (REPO_ROOT / f"assets/worldgen/act{act_id}/heights.npy").is_file()
        ]
        if missing:
            self.skipTest("untracked campaign height payload is not installed")
        for act_id, act in GEN.ACTS.items():
            with self.subTest(act=act_id):
                path = REPO_ROOT / f"assets/worldgen/act{act_id}/heights.npy"
                heights = np.load(path).astype(np.float32)
                world = GEN.World.__new__(GEN.World)
                world.act_id = act_id
                world.act = act
                world.res = heights.shape[0]
                world.spacing = act["half_extent"] * 2.0 / (world.res - 1)
                world.heights = heights
                half = (world.res - 1) * 0.5
                axis = (np.arange(world.res, dtype=np.float32) - half) * world.spacing
                world._gx = axis[None, :]
                world._gz = axis[:, None]
                world.tactical = []
                world.validate_tactical_terrain()


if __name__ == "__main__":
    unittest.main()
