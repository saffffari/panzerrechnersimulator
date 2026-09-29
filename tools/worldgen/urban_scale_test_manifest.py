#!/usr/bin/env python3
"""Many-building test revision for the building runtime at scale (urban intro, role F2, 28 Sep 2026).

Forks the winter-courtyard workshop without changing it into
assets/worldgen/workshops/urban-scale-test-20260928/. It adds a top-level `building_instances`
array to manifest_edits.json: 162 two-storey merchant houses of the building grammar (12 x 8 m;
before the integration of 2026-09-29, the 8 x 8 m featured house stood in) in 27 street
blocks of six houses on a 6 x 6 grid of 83 m block cells over the 500 m square x 1450..1950,
z -100..400 around the courtyard. Each block stands on one flattened pad with a smooth apron. Blocks
that would touch the courtyard house and its dressing, the WITHDRAW mission point or an anti-tank gun
site are left out, so the winter slice keeps working.

It is measurement content for the runtime (instanced rendering, spatial grids, winter-bench), not
authored district content:

    KESSEL_PLAYTEST_MANIFEST=assets/worldgen/workshops/urban-scale-test-20260928/manifest.v1.json \
    cargo run -- winter-bench dense 60 captures/urban_scale_bench.json

With `--daylight` it writes the same district to urban-scale-test-daylight-20260928/ with the
workshop's authored dusk-lighting override switched off (the act's own daylight and auto-exposure),
for inspection renders.

Deterministic: rerunning rewrites byte-identical files (see the receipt's hashes).
"""
from pathlib import Path
import hashlib
import json
import shutil
import sys

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "assets/worldgen/workshops/act1-winter-courtyard-20260925"
DAYLIGHT = "--daylight" in sys.argv[1:]
DEST = ROOT / ("assets/worldgen/workshops/urban-scale-test-daylight-20260928" if DAYLIGHT
               else "assets/worldgen/workshops/urban-scale-test-20260928")
COPIED = ["manifest.v1.json", "features.json", "level_design.v1.json"]

SQUARE_MIN = (1450.0, -100.0)
SQUARE_M = 500.0
BLOCK_CELLS = 6
HOUSE_PITCH_M = 14.0  # 12 m frontages with a 2 m alley: footprints never touch
HALF_FRONTAGE_M = 6.0
HALF_DEPTH_M = 4.0
# The grammar's external steps reach up to about 2 m beyond the street and yard facades.
STEPS_M = 2.5
APRON_M = 6.0

# Keep-out rectangles (x0, z0, x1, z1) and circles (x, z, r): the courtyard house with its
# dressing, spawn and wreck (manifest_edits ops lie in x 1825..1925, z 125..250), the WITHDRAW
# mission point, and the two 45 mm gun sites.
KEEP_OUT_RECTS = [(1810.0, 110.0, 1940.0, 265.0)]
KEEP_OUT_CIRCLES = [(1870.0, 340.0, 20.0), (1555.0, -80.0, 25.0), (1930.0, -95.0, 25.0)]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def block_houses(cx: float, cz: float, turned: bool):
    """Six houses: three along the block's long axis in two rows facing opposite streets."""
    houses = []
    for row, facing in ((-1, 180.0), (1, 0.0)):
        for column in (-1, 0, 1):
            along, across = column * HOUSE_PITCH_M, row * HOUSE_PITCH_M * 0.5
            if turned:
                houses.append((cx + across, cz - along, (facing + 90.0) % 360.0))
            else:
                houses.append((cx + along, cz + across, facing))
    return houses


def rect_clear(x0: float, z0: float, x1: float, z1: float) -> bool:
    for (a0, b0, a1, b1) in KEEP_OUT_RECTS:
        if x0 < a1 and x1 > a0 and z0 < b1 and z1 > b0:
            return False
    for (px, pz, r) in KEEP_OUT_CIRCLES:
        dx = max(x0 - px, 0.0, px - x1)
        dz = max(z0 - pz, 0.0, pz - z1)
        if dx * dx + dz * dz < r * r:
            return False
    return True


def main() -> None:
    DEST.mkdir(parents=True, exist_ok=True)
    for name in COPIED:
        if (SOURCE / name).exists():
            shutil.copyfile(SOURCE / name, DEST / name)
    manifest = json.loads((SOURCE / "manifest.v1.json").read_text())
    original = np.load(SOURCE / manifest["terrain"]["heights"])
    heights = np.array(original, dtype=np.float32, order="C")
    spacing = float(manifest["terrain"]["spacing"])
    half = (heights.shape[0] - 1) / 2
    z, x = np.indices(heights.shape, dtype=np.float64)
    x = (x - half) * spacing
    z = (z - half) * spacing

    def sample(px: float, pz: float) -> float:
        # Bilinear, exactly as terrain::Heightfield::height_at samples the grid.
        gx, gz = px / spacing + half, pz / spacing + half
        x0, z0 = int(np.floor(gx)), int(np.floor(gz))
        tx, tz = gx - x0, gz - z0
        a = original[z0, x0] * (1 - tx) + original[z0, x0 + 1] * tx
        b = original[z0 + 1, x0] * (1 - tx) + original[z0 + 1, x0 + 1] * tx
        return float(a * (1 - tz) + b * tz)

    pitch = SQUARE_M / BLOCK_CELLS
    instances = []
    blocks = []
    skipped = []
    for bz in range(BLOCK_CELLS):
        for bx in range(BLOCK_CELLS):
            cx = round(SQUARE_MIN[0] + (bx + 0.5) * pitch, 3)
            cz = round(SQUARE_MIN[1] + (bz + 0.5) * pitch, 3)
            turned = (bx + bz) % 2 == 1
            houses = block_houses(cx, cz, turned)
            # Frontage along the block's long axis, depth (plus the steps) across it.
            half_x = HALF_DEPTH_M + STEPS_M if turned else HALF_FRONTAGE_M
            half_z = HALF_FRONTAGE_M if turned else HALF_DEPTH_M + STEPS_M
            hx0 = min(h[0] for h in houses) - half_x
            hx1 = max(h[0] for h in houses) + half_x
            hz0 = min(h[1] for h in houses) - half_z
            hz1 = max(h[1] for h in houses) + half_z
            reach = spacing + APRON_M
            if not rect_clear(hx0 - reach, hz0 - reach, hx1 + reach, hz1 + reach):
                skipped.append({"block": f"b{bz}{bx}", "center": [cx, cz]})
                continue
            pad_y = round(sample(cx, cz), 3)
            # The pad covers the block's footprints plus one grid spacing, so bilinear sampling is
            # exactly flat under every house; a smoothstep apron meets the unchanged snow.
            outside = np.maximum.reduce([
                (hx0 - spacing) - x, x - (hx1 + spacing),
                (hz0 - spacing) - z, z - (hz1 + spacing),
                np.zeros_like(x),
            ])
            t = np.clip(outside / APRON_M, 0.0, 1.0)
            weight = 1.0 - t * t * (3.0 - 2.0 * t)
            heights = (heights * (1.0 - weight) + pad_y * weight).astype(np.float32)
            block_id = f"b{bz}{bx}"
            blocks.append({"id": block_id, "center": [cx, cz], "pad_y": pad_y, "turned": turned,
                           "footprint": [round(hx0, 3), round(hz0, 3), round(hx1, 3), round(hz1, 3)]})
            for index, (hx, hz, yaw) in enumerate(houses):
                instances.append({
                    "id": f"urban_scale_test.{block_id}.h{index}",
                    "typology": "merchant_brick_house",
                    "seed": 20260928 + len(instances),
                    "width_m": 12.0,
                    "depth_m": 8.0,
                    "storeys": 2,
                    "cellar": False,
                    "condition": "intact",
                    "x": round(hx, 3),
                    "z": round(hz, 3),
                    "yaw_deg": yaw,
                })
    np.save(DEST / "heights.npy", heights)

    edits = json.loads((SOURCE / "manifest_edits.json").read_text())
    edits["building_instances"] = instances
    if DAYLIGHT:
        edits["environment_lighting"]["enabled"] = False
    (DEST / "manifest_edits.json").write_text(json.dumps(edits, indent=2) + "\n")

    receipt = {
        "purpose": "many-building runtime scale test revision (urban intro F2); not district content",
        "source": str(SOURCE.relative_to(ROOT)),
        "source_manifest_sha256": sha256(SOURCE / "manifest.v1.json"),
        "source_edits_sha256": sha256(SOURCE / "manifest_edits.json"),
        "source_heights_sha256": sha256(SOURCE / manifest["terrain"]["heights"]),
        "heights_sha256": sha256(DEST / "heights.npy"),
        "edits_sha256": sha256(DEST / "manifest_edits.json"),
        "changed_height_samples": int(np.count_nonzero(heights != original)),
        "square": {"x": [SQUARE_MIN[0], SQUARE_MIN[0] + SQUARE_M],
                   "z": [SQUARE_MIN[1], SQUARE_MIN[1] + SQUARE_M]},
        "blocks": blocks,
        "skipped_blocks": skipped,
        "building_instances": len(instances),
        "environment_lighting": ("authored override off: the act's campaign atmosphere and auto-exposure"
                                 if DAYLIGHT else "unchanged: the workshop's authored stormy dusk"),
    }
    (DEST / "urban-scale-test.receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({k: receipt[k] for k in (
        "building_instances", "changed_height_samples", "heights_sha256", "edits_sha256")}, indent=2))
    print(f"blocks: {len(blocks)}, skipped: {[s['block'] for s in skipped]}")


if __name__ == "__main__":
    main()
