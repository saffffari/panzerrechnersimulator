#!/usr/bin/env python3
"""terrain_render_gap.py — the rendered terrain against the collision surface (verifier, 2026-09-29).

The engine keeps the manifest's full heightfield for collision, feet, tanks and building pads
(`Heightfield::height_at`). Until 2026-09-29 that surface was bilinear on the full grid while the
drawn terrain was a decimated mesh (`terrain.rs` `mesh_from_grid`): 512 x 512 vertices on the 4090
preset and 256 x 256 on the laptop preset for a 1024 x 1024 manifest, each render vertex at the
exact fraction i/(rr-1) of the world taking the height of source sample round(i/(rr-1)*(res-1)).
Since the solid-city change both presets draw the full grid, and `height_at` interpolates on the
drawn triangles (each quad split along its (i+1, j)-(i, j+1) diagonal), so the default run
(`--res` = the grid) must report zero. `--res 512 256` replicates the old decimated meshes (the
debug gate `KESSEL_DEBUG_TERRAIN_RENDER_RES`), and `--bilinear` measures against the old bilinear
collision surface instead.

This tool measures how far the snow a player sees lies from the surface everything stands on: at
the perimeters and inside the pads of a workshop's `building_instances`, along its streets (from
`features.json`, if present) and over any extra rectangles given on the command line.

    python3 tools/worldgen/terrain_render_gap.py [WORKSHOP_DIR] [--res N ...] [--bilinear] [--rect NAME:X0:X1:Z0:Z1 ...]

Read-only and deterministic. Positive numbers mean the rendered snow lies above the collision
surface (a wall base or a rail sunk into it), negative that it lies below (a gap under a wall).
"""
import argparse
import json
import math
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
DEFAULT = ROOT / "assets/worldgen/workshops/act1-urban-intro-20260928"


class Terrain:
    def __init__(self, workshop: Path):
        manifest = json.loads((workshop / "manifest.v1.json").read_text())
        self.spacing = float(manifest["terrain"]["spacing"])
        heights_path = Path(manifest["terrain"]["heights"])
        if not heights_path.is_absolute():
            heights_path = workshop / heights_path
        self.h = np.load(heights_path).astype(np.float64)
        self.res = self.h.shape[0]
        self.half = (self.res - 1) * 0.5 * self.spacing
        self.bilinear = False

    def collision(self, x, z):
        """The engine's `Heightfield::height_at`: the full grid's drawn triangles."""
        if self.bilinear:
            return self.collision_bilinear(x, z)
        return self.rendered(x, z, self.res)

    def collision_bilinear(self, x, z):
        """The collision surface before 2026-09-29: bilinear on the full grid."""
        res = self.res
        gx, gz = x / self.spacing + (res - 1) * 0.5, z / self.spacing + (res - 1) * 0.5
        x0 = int(min(max(math.floor(gx), 0), res - 1))
        z0 = int(min(max(math.floor(gz), 0), res - 1))
        x1, z1 = min(x0 + 1, res - 1), min(z0 + 1, res - 1)
        tx, tz = min(max(gx - x0, 0.0), 1.0), min(max(gz - z0, 0.0), 1.0)
        a = self.h[z0, x0] * (1 - tx) + self.h[z0, x1] * tx
        b = self.h[z1, x0] * (1 - tx) + self.h[z1, x1] * tx
        return a * (1 - tz) + b * tz

    def rendered(self, x, z, rr):
        res = self.res
        span = (res - 1) * self.spacing
        fx, fz = (x + self.half) / span * (rr - 1), (z + self.half) / span * (rr - 1)
        i = min(max(int(math.floor(fx)), 0), rr - 2)
        j = min(max(int(math.floor(fz)), 0), rr - 2)
        u, v = fx - i, fz - j

        def src(k):
            return min(int(round(k / (rr - 1) * (res - 1))), res - 1)

        def hv(ii, jj):
            return self.h[src(jj), src(ii)]

        h00, h10, h01, h11 = hv(i, j), hv(i + 1, j), hv(i, j + 1), hv(i + 1, j + 1)
        if u + v <= 1.0:
            return h00 + (h10 - h00) * u + (h01 - h00) * v
        return h11 + (h01 - h11) * (1 - u) + (h10 - h11) * (1 - v)


def footprint(b):
    w, d = b["width_m"], b["depth_m"]
    if int(round(b["yaw_deg"])) % 180 == 90:
        w, d = d, w
    return b["x"] - w / 2, b["x"] + w / 2, b["z"] - d / 2, b["z"] + d / 2


def report(t, name, points, rr):
    if not points:
        return
    d = np.array([t.rendered(x, z, rr) - t.collision(x, z) for x, z in points])
    print(f"  {name:<32} n={len(d):6d}  rendered - collision: {d.min():+.3f} .. {d.max():+.3f} m, "
          f"mean |d| {np.abs(d).mean():.3f} m, p95 |d| {np.percentile(np.abs(d), 95):.3f} m")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("workshop", nargs="?", default=str(DEFAULT))
    ap.add_argument("--res", nargs="*", type=int, default=None,
                    help="render grids to compare (default: the collision grid itself, what ships)")
    ap.add_argument("--bilinear", action="store_true",
                    help="compare against the bilinear collision surface used before 2026-09-29")
    ap.add_argument("--rect", nargs="*", default=[], help="NAME:X0:X1:Z0:Z1 extra regions")
    args = ap.parse_args()
    workshop = Path(args.workshop)
    t = Terrain(workshop)
    t.bilinear = args.bilinear
    edits = json.loads((workshop / "manifest_edits.json").read_text())
    buildings = edits.get("building_instances", [])
    features = {}
    if (workshop / "features.json").exists():
        features = json.loads((workshop / "features.json").read_text())
    streets = []
    for value in features.values():
        if isinstance(value, dict) and isinstance(value.get("streets"), list):
            streets += [s["rect"] for s in value["streets"]]
    for rr in (args.res or [t.res]):
        cell = (t.res - 1) / (rr - 1) * t.spacing
        print(f"{workshop.name}: render grid {rr}^2 ({cell:.2f} m) against collision {t.res}^2 ({t.spacing:.2f} m)")
        perimeter, pads, worst = [], [], []
        for b in buildings:
            x0, x1, z0, z1 = footprint(b)
            ring = []
            for s in np.linspace(0.0, 1.0, 25):
                ring += [(x0, z0 + (z1 - z0) * s), (x1, z0 + (z1 - z0) * s),
                         (x0 + (x1 - x0) * s, z0), (x0 + (x1 - x0) * s, z1)]
            d = [t.rendered(x, z, rr) - t.collision(x, z) for x, z in ring]
            worst.append((min(d), max(d), b["id"]))
            perimeter += ring
            pads += [(x0 + (x1 - x0) * u, z0 + (z1 - z0) * v)
                     for u in np.linspace(0, 1, 9) for v in np.linspace(0, 1, 9)]
        report(t, "building perimeters", perimeter, rr)
        report(t, "building pads", pads, rr)
        if worst:
            low = min(worst)
            high = max(worst, key=lambda w: w[1])
            print(f"    lowest: {low[0]:+.3f} m at {low[2]}; highest: {high[1]:+.3f} m at {high[2]}")
        report(t, "streets", [(x, z) for (x0, x1, z0, z1) in streets
                              for x in np.linspace(x0, x1, 40) for z in np.linspace(z0, z1, 8)], rr)
        for spec in args.rect:
            name, x0, x1, z0, z1 = spec.split(":")
            x0, x1, z0, z1 = map(float, (x0, x1, z0, z1))
            report(t, name, [(x, z) for x in np.linspace(x0, x1, 25) for z in np.linspace(z0, z1, 25)], rr)


if __name__ == "__main__":
    main()
