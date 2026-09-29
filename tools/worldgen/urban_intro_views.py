#!/usr/bin/env python3
"""Headless review cameras for the urban-intro district (integrator, 2026-09-29).

Prints `cargo run -- screenshot` commands that frame the district's buildings from the ground: each
camera stands in front of a building's street facade (local +Z of its placement), at eye height
above the terrain of the generated workshop, looking back at the facade. The heights come from the
workshop's heights.npy exactly as the engine samples them (origin-centred grid, bilinear), so the
cameras sit on the ground whatever the pads' heights.

    python3 tools/worldgen/urban_intro_views.py [--lighting authored|review|inspect] [--pick TYPOLOGY:CONDITION ...]
    python3 tools/worldgen/urban_intro_views.py --write-inspect

Lighting: `authored` is the workshop as generated (the courtyard's stormy dusk, nearly black away
from lamps); `review` is the district tool's daylight variant (`urban_intro_district.py --review`);
`inspect` is a copy written by `--write-inspect` with the sun at 32 degrees, less overcast and the
camera exposure at its cap, for judging geometry. Review and inspect lighting change a real emitter
(the sun, the sky) and the camera, nothing else; captures made with them say so in their names.

Without --pick it chooses one building of every typology in every condition that the district
holds (nearest the spawn first). Deterministic: the same workshop gives the same commands and the
same inspection copy.
"""
from __future__ import annotations

import argparse
import json
import math
import shutil
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
WORKSHOP = ROOT / "assets/worldgen/workshops/act1-urban-intro-20260928"
LIGHTING = {
    "authored": WORKSHOP,
    "review": ROOT / "assets/worldgen/workshops/act1-urban-intro-20260928-review-daylight",
    "inspect": ROOT / "assets/worldgen/workshops/act1-urban-intro-20260928-inspect",
}
INSPECT_LIGHTING = {
    "enabled": True,
    "camera_exposure": 8.0,
    "fog_density": 0.0015,
    "overcast": 0.45,
    "storm": 0.0,
    "sun_azimuth_deg": 150.0,
    "sun_elevation_deg": 32.0,
    "twilight": 0.0,
    "schema": "kessel.environment-authoring",
    "version": 1,
}
SPAWN = (1850.0, 250.0)


def write_inspect() -> Path:
    """The workshop with inspection lighting: only `environment_lighting` differs."""
    dst = LIGHTING["inspect"]
    dst.mkdir(parents=True, exist_ok=True)
    for name in ["manifest.v1.json", "heights.npy", "features.json", "level_design.v1.json"]:
        shutil.copyfile(WORKSHOP / name, dst / name)
    edits = json.loads((WORKSHOP / "manifest_edits.json").read_text())
    edits["environment_lighting"] = INSPECT_LIGHTING
    (dst / "manifest_edits.json").write_text(json.dumps(edits, indent=2) + "\n")
    return dst


def load_heights(workshop: Path):
    manifest = json.loads((workshop / "manifest.v1.json").read_text())
    spacing = float(manifest["terrain"]["spacing"])
    heights = np.load(workshop / manifest["terrain"]["heights"]).astype(np.float64)
    return heights, spacing


def height_at(heights, spacing, x, z):
    res = heights.shape[0]
    half = (res - 1) * 0.5
    gx, gz = x / spacing + half, z / spacing + half
    x0 = int(min(max(math.floor(gx), 0), res - 1))
    z0 = int(min(max(math.floor(gz), 0), res - 1))
    x1, z1 = min(x0 + 1, res - 1), min(z0 + 1, res - 1)
    tx = min(max(gx - x0, 0.0), 1.0)
    tz = min(max(gz - z0, 0.0), 1.0)
    a = heights[z0, x0] * (1 - tx) + heights[z0, x1] * tx
    b = heights[z1, x0] * (1 - tx) + heights[z1, x1] * tx
    return a * (1 - tz) + b * tz


def facade_camera(inst, heights, spacing, distance=13.0, eye=1.7):
    """Camera in front of the street facade (local +Z), looking at the facade's centre."""
    yaw = math.radians(inst["yaw_deg"])
    # Local +Z turned by R_y(yaw): (sin yaw, cos yaw) in world XZ.
    nx, nz = math.sin(yaw), math.cos(yaw)
    half_depth = inst["depth_m"] * 0.5
    cx = inst["x"] + nx * (half_depth + distance)
    cz = inst["z"] + nz * (half_depth + distance)
    ground = height_at(heights, spacing, cx, cz)
    pad = height_at(heights, spacing, inst["x"], inst["z"])
    # Look back along -normal: camera forward = (sin cyaw, ·, -cos cyaw) = (-nx, ·, -nz).
    cyaw = math.degrees(math.atan2(-nx, nz))
    target_height = pad + 3.0 * inst["storeys"] * 0.5
    pitch = math.degrees(math.atan2(target_height - (ground + eye), half_depth + distance))
    return cx, ground + eye, cz, cyaw % 360.0, pitch, pad


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--lighting", choices=sorted(LIGHTING), default="inspect")
    parser.add_argument("--pick", nargs="*", default=None, help="TYPOLOGY:CONDITION pairs")
    parser.add_argument("--prefix", default="urban_i_facade")
    parser.add_argument("--write-inspect", action="store_true", help="write the inspection-lighting copy")
    args = parser.parse_args()
    if args.write_inspect:
        print(f"wrote {write_inspect().relative_to(ROOT)}")
        return
    heights, spacing = load_heights(WORKSHOP)
    edits = json.loads((WORKSHOP / "manifest_edits.json").read_text())
    instances = edits["building_instances"]
    manifest = LIGHTING[args.lighting] / "manifest.v1.json"
    by_kind = {}
    for inst in sorted(instances, key=lambda i: (math.hypot(i["x"] - SPAWN[0], i["z"] - SPAWN[1]), i["id"])):
        by_kind.setdefault((inst["typology"], inst["condition"]), inst)
    wanted = sorted(by_kind) if args.pick is None else [tuple(p.split(":")) for p in args.pick]
    for key in wanted:
        inst = by_kind.get(key)
        if inst is None:
            print(f"# no {key[0]} in condition {key[1]} in the district")
            continue
        cx, cy, cz, cyaw, pitch, pad = facade_camera(inst, heights, spacing)
        name = f"{args.prefix}_{key[0]}_{key[1]}_{args.lighting}.png"
        print(f"# {inst['id']}: {inst['typology']} {inst['width_m']:.0f}x{inst['depth_m']:.0f} m, "
              f"{inst['storeys']} storeys, cellar {inst['cellar']}, {inst['condition']}, yaw {inst['yaw_deg']:.0f}, pad {pad:.2f}")
        print(f"KESSEL_RECON_MANIFEST={manifest.relative_to(ROOT)} cargo run -- screenshot captures/{name} "
              f"{cx:.2f} {cy:.2f} {cz:.2f} {cyaw:.1f} {pitch:.1f} 0.002")


if __name__ == "__main__":
    main()
