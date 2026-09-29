#!/usr/bin/env python3
"""Headless review cameras for the urban-intro tableaux (story role S, 2026-09-29).

Prints (and with --run, makes) `screenshot` commands that frame each tableau of
`catalog/urban_intro_tableaux.v1.json` on the district: from the approach a player takes and from
inside or close by at eye height. Camera and target heights are metres above the workshop's
terrain (sampled from heights.npy exactly as the engine samples it: origin-centred grid,
bilinear), or absolute where a camera stands inside a building or the dugout.

    python3 tools/worldgen/urban_tableau_views.py [--lighting authored|inspect|worklamp] [--run] [NAME ...]
        [--prefix urban_s] [--exposure EV]
    python3 tools/worldgen/urban_tableau_views.py --write-worklamp

Cellars and scenes (29 Sep 2026) adds the cellar interiors of ranks 1 and 2 and the historical
scenes (the burned household with its courtyard trench, the cellar ward and the doctor's cellar,
the Gumrak stores beside the route west). Cellars are dark: `--exposure` sets a fixed camera
exposure (`KESSEL_CAMERA_EXPOSURE`, a camera setting, not light) and names the capture `_x<EV>`.
The unlit ones (rank 1's cellar, the household's, the ward and the doctor's) stay black even at the
exposure cap: daylight reaches them only through small windows and the renderer's single secondary
bounce. `--write-worklamp` writes a review-only copy of the inspect workshop with one work lamp
hung in each of those rooms (`WORK_LAMPS`: real, shadowed point emitters in `authored_lights`, like
a photographer's lamp, 2 m above the cellar floor); captures made with it are named `_worklamp`.
It judges composition only: the game never has these lamps.

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
    "worklamp": ROOT / "assets/worldgen/workshops/act1-urban-intro-20260928-worklamp",
}

# Review-only work lamps (world metres; 2 m above each unlit cellar room's floor, at its centre).
WORK_LAMPS = {
    "worklamp_embrasure_cellar": (1865.16, 6.43, 315.93),
    "worklamp_household_shelter": (1952.16, 9.79, 327.16),
    "worklamp_household_front": (1957.84, 9.79, 336.17),
    "worklamp_ward_west": (1901.16, 8.30, 433.00),
    "worklamp_ward_east": (1902.36, 8.30, 437.50),
    "worklamp_doctor": (1865.16, 8.10, 466.16),
    # The red-cross stair hall, for the cellar walk film (catalog/shader_reviews/urban2_*_worklamp).
    "worklamp_redcross_stair": (1834.84, 7.75, 187.34),
}
WORK_LAMP_INTENSITY = 20.0


def write_worklamp() -> Path:
    """The inspect workshop plus the review-only work lamps; nothing else differs."""
    src, dst = WORKSHOP["inspect"], WORKSHOP["worklamp"]
    if not (src / "manifest_edits.json").exists():
        raise SystemExit(f"{src} is missing: run tools/worldgen/urban_intro_views.py --write-inspect")
    dst.mkdir(parents=True, exist_ok=True)
    for name in ["manifest.v1.json", "heights.npy", "features.json", "level_design.v1.json"]:
        (dst / name).write_bytes((src / name).read_bytes())
    edits = json.loads((src / "manifest_edits.json").read_text())
    lights = [l for l in edits.get("authored_lights", []) if not l["id"].startswith("worklamp_")]
    for lamp_id, (x, y, z) in WORK_LAMPS.items():
        lights.append({"id": lamp_id, "kind": "point", "position": [x, y, z], "temperature_k": 3200.0,
                       "radiant_intensity": WORK_LAMP_INTENSITY, "source_radius_m": 0.05, "range_m": 12.0,
                       "flicker": 0.0, "enabled": True})
    edits["authored_lights"] = lights
    (dst / "manifest_edits.json").write_text(json.dumps(edits, indent=2) + "\n")
    return dst

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
    # Rank 1's cellar behind the slot: the firing step under it, and the September gun in the corner.
    "embrasure_cellar": ((1867.16, 6.03, 316.83, True), (1862.86, 5.63, 314.64, True), 0.002),
    "embrasure_cellar_gun": ((1864.16, 6.03, 314.03, True), (1866.36, 4.93, 317.63, True), 0.002),
    # Rank 2's cellar: the dressing room's table, the ward room, and the stair head looking down.
    "redcross_cellar_table": ((1836.14, 7.30, 185.06, True), (1834.59, 6.55, 181.21, True), 0.002),
    "redcross_cellar_ward": ((1831.46, 7.30, 181.50, True), (1827.16, 6.00, 181.66, True), 0.002),
    "redcross_stair_head": ((1837.30, 9.95, 187.20, True), (1833.00, 5.95, 187.60, True), 0.002),
    # The burned household (host urban_intro.c3r2.e05): the street door, the stair head, the cellar
    # shelter under the ground floor's vault, and the courtyard trench.
    "household_street": ((1971.0, 1.7, 335.0, False), (1960.68, 11.39, 332.33, True), 0.002),
    "household_stair_head": ((1960.2, 11.99, 332.8, True), (1955.8, 7.99, 331.79, True), 0.002),
    "household_cellar": ((1954.26, 9.34, 330.16, True), (1950.11, 8.19, 326.96, True), 0.002),
    "household_trench_far": ((1928.66, 1.7, 339.93, False), (1948.0, 1.2, 336.0, False), 0.002),
    "household_trench_inside": ((1935.16, 0.8, 339.98, False), (1947.0, 1.0, 339.0, False), 0.002),
    # The cellar ward (host urban_intro.c3r3.r00) and the doctor's cellar (urban_intro.c3r3.w02).
    "ward_entry": ((1899.96, 7.85, 430.70, True), (1902.96, 6.60, 433.50, True), 0.002),
    "ward_hearth": ((1899.96, 7.85, 436.60, True), (1900.71, 6.40, 439.60, True), 0.002),
    "ward_doctor": ((1863.96, 7.65, 469.36, True), (1863.96, 6.90, 464.36, True), 0.002),
    # The Gumrak stores and the house beside the route west, outside the district.
    "gumrak_approach": ((1230.00, 2.4, 815.10, False), (1212.50, 3.28, 797.50, True), 0.002),
    "gumrak_store": ((1214.95, 3.13, 803.70, True), (1208.65, 2.13, 793.30, True), 0.002),
    "gumrak_house": ((1238.0, 5.12, 817.3, True), (1234.64, 3.67, 820.8, True), 0.002),
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


def command(name: str, lighting: str, terrain: Terrain, prefix: str = "urban_s",
            exposure: float | None = None) -> tuple[Path, list[str]]:
    (x, h, z, ab), (tx, th, tz, tab), fog = SHOTS[name]
    y = h if ab else terrain.height_at(x, z) + h
    ty = th if tab else terrain.height_at(tx, tz) + th
    dx, dy, dz = tx - x, ty - y, tz - z
    yaw = math.degrees(math.atan2(dx, -dz)) % 360.0  # yaw 0 looks toward -Z
    pitch = math.degrees(math.atan2(dy, math.hypot(dx, dz)))
    suffix = f"_x{exposure:g}" if exposure else ""
    out = f"captures/{prefix}_{name}_{lighting}{suffix}.png"
    manifest = (WORKSHOP[lighting] / "manifest.v1.json").relative_to(ROOT)
    return manifest, ["./target/debug/panzerrechnersystem", "screenshot", out, f"{x:.2f}", f"{y:.2f}",
                      f"{z:.2f}", f"{yaw:.1f}", f"{pitch:.1f}", f"{fog}"]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--lighting", choices=sorted(WORKSHOP), default="inspect")
    parser.add_argument("--run", action="store_true", help="make the captures (debug binary)")
    parser.add_argument("--write-worklamp", action="store_true",
                        help="write the review-only work-lamp copy of the inspect workshop")
    parser.add_argument("--prefix", default="urban_s", help="capture name prefix")
    parser.add_argument("--exposure", type=float, default=None,
                        help="fixed camera exposure (KESSEL_CAMERA_EXPOSURE) for dark interiors")
    parser.add_argument("names", nargs="*", help=f"shots (default all): {', '.join(SHOTS)}")
    args = parser.parse_args()
    if args.write_worklamp:
        print(f"wrote {write_worklamp().relative_to(ROOT)}")
        return
    unknown = [n for n in args.names if n not in SHOTS]
    if unknown:
        parser.error(f"unknown shots: {unknown}")
    workshop = WORKSHOP[args.lighting]
    if not (workshop / "manifest.v1.json").exists():
        parser.error(f"{workshop} is missing (inspect: run tools/worldgen/urban_intro_views.py --write-inspect)")
    terrain = Terrain(WORKSHOP["authored"])
    for name in args.names or list(SHOTS):
        manifest, cmd = command(name, args.lighting, terrain, args.prefix, args.exposure)
        exposure = f"KESSEL_CAMERA_EXPOSURE={args.exposure:g} " if args.exposure else ""
        print(f"{exposure}KESSEL_RECON_MANIFEST={manifest} " + " ".join(cmd), flush=True)
        if args.run:
            env = dict(os.environ, KESSEL_RECON_MANIFEST=str(manifest))
            if args.exposure:
                env["KESSEL_CAMERA_EXPOSURE"] = f"{args.exposure:g}"
            result = subprocess.run(cmd, cwd=ROOT, env=env, capture_output=True, text=True)
            wrote = [l for l in (result.stdout + result.stderr).splitlines() if "wrote" in l]
            print(f"    exit {result.returncode}: {wrote[-1].strip() if wrote else 'no capture written'}", flush=True)


if __name__ == "__main__":
    main()
