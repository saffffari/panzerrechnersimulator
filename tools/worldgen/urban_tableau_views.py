#!/usr/bin/env python3
"""Headless review cameras for the urban-intro tableaux (story role S, 2026-09-29).

Prints (and with --run, makes) `screenshot` commands that frame each tableau of
`catalog/urban_intro_tableaux.v1.json` on the district: from the approach a player takes and from
inside or close by at eye height. Camera and target heights are metres above the workshop's
terrain (sampled from heights.npy exactly as the engine samples it: origin-centred grid,
bilinear), or absolute where a camera stands inside a building or the dugout.

    python3 tools/worldgen/urban_tableau_views.py [--lighting authored|inspect] [--run] [NAME ...]

Lighting: `authored` is the workshop as generated (the courtyard's stormy dusk: at night only the
tableaux' own lamp and stove show); `inspect` is the copy that
`tools/worldgen/urban_intro_views.py --write-inspect` writes (sun at 32 degrees, exposure at its
cap) for judging form. Captures go to captures/urban_s_<name>_<lighting>.png. Uses the debug
binary (`cargo build` first). Deterministic: the same workshop gives the same commands.
"""
from __future__ import annotations

import argparse
import json
import math
import os
import subprocess
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
WORKSHOP = {
    "authored": ROOT / "assets/worldgen/workshops/act1-urban-intro-20260928",
    "inspect": ROOT / "assets/worldgen/workshops/act1-urban-intro-20260928-inspect",
}

# name: ((x, height, z, absolute?), (target x, height, z, absolute?), fog density)
SHOTS = {
    # Rank 1, the low window (host urban_intro.c3r2.w01).
    "embrasure_street": ((1855.0, 2.8, 301.0, False), (1862.2, 0.4, 314.6, False), 0.002),
    "embrasure_slot": ((1858.6, 0.9, 313.2, False), (1862.2, 0.35, 314.6, False), 0.002),
    "embrasure_from_wreck": ((1817.0, 2.4, 303.5, False), (1862.0, 1.2, 314.6, False), 0.002),
    "embrasure_interior": ((1864.6, 8.85, 315.4, True), (1827.0, 1.5, 306.0, False), 0.002),
    # Rank 2, the red cross over the cellar stair (host urban_intro.c2r1.e02).
    "redcross_gate": ((1846.0, 2.4, 191.2, False), (1825.5, 1.2, 187.5, False), 0.002),
    "redcross_dead": ((1819.6, 1.7, 186.2, False), (1825.6, 0.6, 186.4, False), 0.002),
    "redcross_hall": ((1826.9, 9.9, 187.55, True), (1831.3, 8.5, 187.4, True), 0.002),
    # Rank 3, smoke from the ground (host urban_intro.c0r3.e02).
    "dugout_gate": ((1611.0, 1.7, 424.0, False), (1590.0, 1.2, 421.0, False), 0.002),
    "dugout_near": ((1595.5, 1.7, 427.0, False), (1590.0, 0.9, 421.0, False), 0.002),
    "dugout_door": ((1594.8, 1.3, 421.3, False), (1591.2, 0.6, 421.08, False), 0.002),
    "dugout_room": ((1591.23, 11.95, 421.08, True), (1589.23, 11.4, 421.08, True), 0.002),
    # Rank 4, the tram (host urban_intro.c2r3.e00).
    "tram_approach": ((1853.5, 2.4, 364.0, False), (1848.25, 1.6, 381.0, False), 0.002),
    "tram_horse": ((1840.2, 1.7, 372.5, False), (1845.2, 0.6, 379.5, False), 0.002),
    "tram_interior": ((1848.25, 10.25, 377.4, True), (1848.25, 9.3, 386.0, True), 0.002),
    # Rank 5, white canopies (host urban_intro.c2r1.e00).
    "canopy_street": ((1851.0, 2.4, 128.0, False), (1838.0, 25.0, 108.5, True), 0.002),
    "canister_yard": ((1821.3, 1.7, 110.8, False), (1795.0, 0.2, 96.0, False), 0.002),
    "canister_from_open": ((1781.0, 1.7, 89.0, False), (1822.0, 3.0, 106.0, False), 0.002),
    "canister_interior": ((1825.6, 12.8, 108.3, True), (1786.0, 10.9, 92.0, True), 0.002),
}


class Terrain:
    def __init__(self, workshop: Path):
        manifest = json.loads((workshop / "manifest.v1.json").read_text())
        self.spacing = float(manifest["terrain"]["spacing"])
        self.heights = np.load(workshop / manifest["terrain"]["heights"]).astype(np.float64)

    def height_at(self, x: float, z: float) -> float:
        res = self.heights.shape[0]
        half = (res - 1) * 0.5
        gx, gz = x / self.spacing + half, z / self.spacing + half
        x0 = int(min(max(math.floor(gx), 0), res - 1))
        z0 = int(min(max(math.floor(gz), 0), res - 1))
        x1, z1 = min(x0 + 1, res - 1), min(z0 + 1, res - 1)
        tx = min(max(gx - x0, 0.0), 1.0)
        tz = min(max(gz - z0, 0.0), 1.0)
        a = self.heights[z0, x0] * (1 - tx) + self.heights[z0, x1] * tx
        b = self.heights[z1, x0] * (1 - tx) + self.heights[z1, x1] * tx
        return float(a * (1 - tz) + b * tz)


def command(name: str, lighting: str, terrain: Terrain) -> tuple[Path, list[str]]:
    (x, h, z, ab), (tx, th, tz, tab), fog = SHOTS[name]
    y = h if ab else terrain.height_at(x, z) + h
    ty = th if tab else terrain.height_at(tx, tz) + th
    dx, dy, dz = tx - x, ty - y, tz - z
    yaw = math.degrees(math.atan2(dx, -dz)) % 360.0  # yaw 0 looks toward -Z
    pitch = math.degrees(math.atan2(dy, math.hypot(dx, dz)))
    out = f"captures/urban_s_{name}_{lighting}.png"
    manifest = (WORKSHOP[lighting] / "manifest.v1.json").relative_to(ROOT)
    return manifest, ["./target/debug/panzerrechnersystem", "screenshot", out, f"{x:.2f}", f"{y:.2f}",
                      f"{z:.2f}", f"{yaw:.1f}", f"{pitch:.1f}", f"{fog}"]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--lighting", choices=sorted(WORKSHOP), default="inspect")
    parser.add_argument("--run", action="store_true", help="make the captures (debug binary)")
    parser.add_argument("names", nargs="*", help=f"shots (default all): {', '.join(SHOTS)}")
    args = parser.parse_args()
    unknown = [n for n in args.names if n not in SHOTS]
    if unknown:
        parser.error(f"unknown shots: {unknown}")
    workshop = WORKSHOP[args.lighting]
    if not (workshop / "manifest.v1.json").exists():
        parser.error(f"{workshop} is missing (inspect: run tools/worldgen/urban_intro_views.py --write-inspect)")
    terrain = Terrain(WORKSHOP["authored"])
    for name in args.names or list(SHOTS):
        manifest, cmd = command(name, args.lighting, terrain)
        print(f"KESSEL_RECON_MANIFEST={manifest} " + " ".join(cmd), flush=True)
        if args.run:
            env = dict(os.environ, KESSEL_RECON_MANIFEST=str(manifest))
            result = subprocess.run(cmd, cwd=ROOT, env=env, capture_output=True, text=True)
            wrote = [l for l in (result.stdout + result.stderr).splitlines() if "wrote" in l]
            print(f"    exit {result.returncode}: {wrote[-1].strip() if wrote else 'no capture written'}", flush=True)


if __name__ == "__main__":
    main()
