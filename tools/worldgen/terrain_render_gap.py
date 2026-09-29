#!/usr/bin/env python3
"""terrain_render_gap.py — the rendered terrain against the collision heightfield (verifier, 2026-09-29).

The engine keeps the manifest's full heightfield for collision, feet, tanks and building pads
(`Heightfield::height_at`, bilinear on the full grid) but renders a decimated mesh
(`terrain.rs` `mesh_from_grid`, `authored_render_res`): 512 x 512 vertices on the 4090 preset and
256 x 256 on the laptop preset for a 1024 x 1024 manifest. Since cellars and scenes (29 Sep 2026,
`terrain_opening::build_mesh`) each render vertex lies on the source sample round(i/(rr-1)*(res-1))
it takes its height from (before, it sat at the exact fraction i/(rr-1) of the world); quads split
along the (i+1, j)-(i, j+1) diagonal. Round every opened cellar and dug trench the mesh is drawn
from the source grid itself in every coarse cell meeting a refinement rectangle, and a coarse cell
beside such cells is fanned from its centre to the fine vertices on their shared edges; inside an
opening no terrain is drawn. This tool replicates that mesh and measures how far the snow a player
sees lies from the surface everything stands on: at the perimeters and inside the pads of a
workshop's `building_instances`, along its streets (from `features.json`, if present) and over any
extra rectangles given on the command line. Points inside an opening are not terrain and are left
out. The openings and refinement rectangles come from the build itself (`--refine FILE`, written by
any headless run with `KESSEL_TERRAIN_OPENINGS_DUMP=FILE`); without it the tool measures the
decimated mesh alone.

    python3 tools/worldgen/terrain_render_gap.py [WORKSHOP_DIR] [--res 512 256] [--refine FILE]
        [--rect NAME:X0:X1:Z0:Z1 ...]

Read-only and deterministic. Positive numbers mean the rendered snow lies above the collision
surface (a wall base or a rail sunk into it), negative that it lies below (a gap under a wall).
"""
import argparse
import bisect
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

    def collision(self, x, z):
        res = self.res
        gx, gz = x / self.spacing + (res - 1) * 0.5, z / self.spacing + (res - 1) * 0.5
        x0 = int(min(max(math.floor(gx), 0), res - 1))
        z0 = int(min(max(math.floor(gz), 0), res - 1))
        x1, z1 = min(x0 + 1, res - 1), min(z0 + 1, res - 1)
        tx, tz = min(max(gx - x0, 0.0), 1.0), min(max(gz - z0, 0.0), 1.0)
        a = self.h[z0, x0] * (1 - tx) + self.h[z0, x1] * tx
        b = self.h[z1, x0] * (1 - tx) + self.h[z1, x1] * tx
        return a * (1 - tz) + b * tz

    def pos(self, s):
        """World coordinate of source index s along either axis."""
        return s * self.spacing - self.half

    def mesh(self, rr, rects):
        """The coarse vertices' source indices and the refined coarse cells for render grid rr."""
        coarse = [min(int(round(k / (rr - 1) * (self.res - 1))), self.res - 1) for k in range(rr)]
        xs = [self.pos(s) for s in coarse]
        refined = set()
        for (lx, lz, hx, hz) in rects:
            i0 = max(bisect.bisect_left(xs, lx) - 1, 0)
            i1 = max(bisect.bisect_left(xs, hx) - 1, i0)
            j0 = max(bisect.bisect_left(xs, lz) - 1, 0)
            j1 = max(bisect.bisect_left(xs, hz) - 1, j0)
            for j in range(j0, min(j1, rr - 2) + 1):
                for i in range(i0, min(i1, rr - 2) + 1):
                    if xs[i + 1] >= lx and xs[i] <= hx and xs[j + 1] >= lz and xs[j] <= hz:
                        refined.add((i, j))
        return coarse, xs, refined

    @staticmethod
    def split(h00, h10, h01, h11, u, v):
        if u + v <= 1.0:
            return h00 + (h10 - h00) * u + (h01 - h00) * v
        return h11 + (h01 - h11) * (1 - u) + (h10 - h11) * (1 - v)

    def rendered(self, x, z, rr, mesh=None):
        coarse, xs, refined = mesh or self.mesh(rr, [])
        i = min(max(bisect.bisect_right(xs, x) - 1, 0), rr - 2)
        j = min(max(bisect.bisect_right(xs, z) - 1, 0), rr - 2)
        if (i, j) in refined:
            gx, gz = (x + self.half) / self.spacing, (z + self.half) / self.spacing
            si = min(max(int(math.floor(gx)), 0), self.res - 2)
            sj = min(max(int(math.floor(gz)), 0), self.res - 2)
            h = self.h
            return self.split(h[sj, si], h[sj, si + 1], h[sj + 1, si], h[sj + 1, si + 1], gx - si, gz - sj)
        i0, i1, j0, j1 = coarse[i], coarse[i + 1], coarse[j], coarse[j + 1]
        near = [(i, j - 1) in refined, (i + 1, j) in refined, (i, j + 1) in refined, (i - 1, j) in refined]
        if not any(near):
            u = (x - xs[i]) / (xs[i + 1] - xs[i])
            v = (z - xs[j]) / (xs[j + 1] - xs[j])
            h = self.h
            return self.split(h[j0, i0], h[j0, i1], h[j1, i0], h[j1, i1], u, v)
        # A coarse cell beside refined ones: fanned from its centre to its boundary ring.
        ring = [(i0, j0)]
        if near[0]:
            ring += [(k, j0) for k in range(i0 + 1, i1)]
        ring.append((i1, j0))
        if near[1]:
            ring += [(i1, k) for k in range(j0 + 1, j1)]
        ring.append((i1, j1))
        if near[2]:
            ring += [(k, j1) for k in range(i1 - 1, i0, -1)]
        ring.append((i0, j1))
        if near[3]:
            ring += [(i0, k) for k in range(j1 - 1, j0, -1)]
        pts = [(self.pos(a), self.pos(b), self.h[b, a]) for a, b in ring]
        corners = [(i0, j0), (i1, j0), (i0, j1), (i1, j1)]
        cx = sum(self.pos(a) for a, _ in corners) / 4
        cz = sum(self.pos(b) for _, b in corners) / 4
        ch = sum(self.h[b, a] for a, b in corners) / 4
        for k in range(len(pts)):
            (ax, az, ah), (bx, bz, bh) = pts[k], pts[(k + 1) % len(pts)]
            det = (ax - cx) * (bz - cz) - (bx - cx) * (az - cz)
            if abs(det) < 1e-12:
                continue
            wa = ((x - cx) * (bz - cz) - (bx - cx) * (z - cz)) / det
            wb = ((ax - cx) * (z - cz) - (x - cx) * (az - cz)) / det
            if wa >= -1e-9 and wb >= -1e-9 and wa + wb <= 1 + 1e-9:
                return ch + (ah - ch) * wa + (bh - ch) * wb
        raise ValueError(f"({x}, {z}) in no fan triangle of cell {i}, {j}")


def footprint(b):
    w, d = b["width_m"], b["depth_m"]
    if int(round(b["yaw_deg"])) % 180 == 90:
        w, d = d, w
    return b["x"] - w / 2, b["x"] + w / 2, b["z"] - d / 2, b["z"] + d / 2


def report(t, name, points, rr, mesh=None, openings=()):
    points = [(x, z) for x, z in points
              if not any(lx < x < hx and lz < z < hz for (lx, lz, hx, hz) in openings)]
    if not points:
        return
    d = np.array([t.rendered(x, z, rr, mesh) - t.collision(x, z) for x, z in points])
    print(f"  {name:<32} n={len(d):6d}  rendered - collision: {d.min():+.3f} .. {d.max():+.3f} m, "
          f"mean |d| {np.abs(d).mean():.3f} m, p95 |d| {np.percentile(np.abs(d), 95):.3f} m")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("workshop", nargs="?", default=str(DEFAULT))
    ap.add_argument("--res", nargs="*", type=int, default=[512, 256])
    ap.add_argument("--rect", nargs="*", default=[], help="NAME:X0:X1:Z0:Z1 extra regions")
    ap.add_argument("--refine", default=None,
                    help="openings and refinement written by a build (KESSEL_TERRAIN_OPENINGS_DUMP)")
    args = ap.parse_args()
    rects, openings = [], []
    if args.refine:
        dump = json.loads(Path(args.refine).read_text())
        openings = [tuple(o["rect"]) for o in dump["openings"]]
        rects = [tuple(r) for r in dump["refine"]]
    # Every opening widened by one and a half source cells is refined too (`build_mesh`).
    workshop = Path(args.workshop)
    t = Terrain(workshop)
    edits = json.loads((workshop / "manifest_edits.json").read_text())
    buildings = edits.get("building_instances", [])
    features = {}
    if (workshop / "features.json").exists():
        features = json.loads((workshop / "features.json").read_text())
    streets = []
    for value in features.values():
        if isinstance(value, dict) and isinstance(value.get("streets"), list):
            streets += [s["rect"] for s in value["streets"]]
    widen = 1.5 * t.spacing
    rects += [(lx - widen, lz - widen, hx + widen, hz + widen) for (lx, lz, hx, hz) in openings]
    for rr in args.res:
        cell = (t.res - 1) / (rr - 1) * t.spacing
        mesh = t.mesh(rr, rects)
        print(f"{workshop.name}: render grid {rr}^2 ({cell:.2f} m) against collision {t.res}^2 ({t.spacing:.2f} m); "
              f"{len(mesh[2])} coarse cells drawn from the collision grid, {len(openings)} openings")
        perimeter, pads, worst = [], [], []
        for b in buildings:
            x0, x1, z0, z1 = footprint(b)
            ring = []
            for s in np.linspace(0.0, 1.0, 25):
                ring += [(x0, z0 + (z1 - z0) * s), (x1, z0 + (z1 - z0) * s),
                         (x0 + (x1 - x0) * s, z0), (x0 + (x1 - x0) * s, z1)]
            ring = [(x, z) for x, z in ring
                    if not any(lx < x < hx and lz < z < hz for (lx, lz, hx, hz) in openings)]
            d = [t.rendered(x, z, rr, mesh) - t.collision(x, z) for x, z in ring]
            if d:
                worst.append((min(d), max(d), b["id"]))
            perimeter += ring
            pads += [(x0 + (x1 - x0) * u, z0 + (z1 - z0) * v)
                     for u in np.linspace(0, 1, 9) for v in np.linspace(0, 1, 9)]
        report(t, "building perimeters", perimeter, rr, mesh, openings)
        report(t, "building pads", pads, rr, mesh, openings)
        if worst:
            low = min(worst)
            high = max(worst, key=lambda w: w[1])
            print(f"    lowest: {low[0]:+.3f} m at {low[2]}; highest: {high[1]:+.3f} m at {high[2]}")
        report(t, "streets", [(x, z) for (x0, x1, z0, z1) in streets
                              for x in np.linspace(x0, x1, 40) for z in np.linspace(z0, z1, 8)], rr, mesh, openings)
        for spec in args.rect:
            name, x0, x1, z0, z1 = spec.split(":")
            x0, x1, z0, z1 = map(float, (x0, x1, z0, z1))
            report(t, name, [(x, z) for x in np.linspace(x0, x1, 25) for z in np.linspace(z0, z1, 25)], rr,
                   mesh, openings)


if __name__ == "__main__":
    main()
