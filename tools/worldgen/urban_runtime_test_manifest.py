#!/usr/bin/env python3
"""Test revision for the multi-instance building runtime (urban intro, role F1, 28 Sep 2026).

Forks the winter-courtyard workshop without changing it into
assets/worldgen/workshops/urban-runtime-test-20260928/, flattens a pad with a smooth apron under
each test building, and adds a top-level `building_instances` array to manifest_edits.json: six
12 x 12 m two-storey merchant houses of the building grammar at the four quarter-turn yaws on open
ground west of the courtyard (row b with cellars: their ground floors stand 1.1 m above the pad).
It is test content for the runtime, not authored district content. Since the integration
(2026-09-29) the grammar builds them; the 8 x 8 m featured-house stand-in of the first revision is
outside the research limits the grammar enforces.

    KESSEL_RECON_MANIFEST=assets/worldgen/workshops/urban-runtime-test-20260928/manifest.v1.json \
    cargo run -- screenshot captures/urban_runtime_<view>.png X Y Z YAW PITCH FOG

Deterministic: rerunning rewrites byte-identical files (see the receipt's hashes).
"""
from pathlib import Path
import hashlib
import json
import shutil

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "assets/worldgen/workshops/act1-winter-courtyard-20260925"
DEST = ROOT / "assets/worldgen/workshops/urban-runtime-test-20260928"
COPIED = ["manifest.v1.json", "features.json", "level_design.v1.json"]
APRON_M = 6.0
HALF_FOOTPRINT_M = 6.0
# The grammar's external steps reach up to about 2 m beyond the street and yard facades.
STEPS_M = 2.5

# (id, x, z, yaw_deg, cellar): a row of the four yaws, and two more behind it. 26 m apart, so no two
# pads (footprint + steps + one grid spacing) overlap.
PLACEMENTS = [
    ("urban_runtime_test.row_a.h1", 1640.0, 250.0, 0.0, False),
    ("urban_runtime_test.row_a.h2", 1666.0, 250.0, 90.0, False),
    ("urban_runtime_test.row_a.h3", 1692.0, 250.0, 180.0, False),
    ("urban_runtime_test.row_a.h4", 1718.0, 250.0, 270.0, False),
    ("urban_runtime_test.row_b.h1", 1653.0, 280.0, 270.0, True),
    ("urban_runtime_test.row_b.h2", 1705.0, 280.0, 90.0, True),
]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def instance(index: int, ident: str, x: float, z: float, yaw: float, cellar: bool) -> dict:
    return {
        "id": ident,
        "typology": "merchant_brick_house",
        "seed": 20260928 + index,
        "width_m": 12.0,
        "depth_m": 12.0,
        "storeys": 2,
        "cellar": cellar,
        "condition": "intact",
        "x": x,
        "z": z,
        "yaw_deg": yaw,
    }


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

    pads = []
    for ident, px, pz, _, _ in PLACEMENTS:
        pad_y = round(sample(px, pz), 3)
        # The pad covers the footprint and its external steps plus one grid spacing, so bilinear
        # sampling is exactly flat under the building; a smoothstep apron meets the unchanged snow.
        reach = HALF_FOOTPRINT_M + STEPS_M + spacing
        outside = np.maximum.reduce(
            [np.abs(x - px) - reach, np.abs(z - pz) - reach, np.zeros_like(x)]
        )
        t = np.clip(outside / APRON_M, 0.0, 1.0)
        weight = 1.0 - t * t * (3.0 - 2.0 * t)
        heights = (heights * (1.0 - weight) + pad_y * weight).astype(np.float32)
        pads.append({"id": ident, "pad_y": pad_y, "reach_m": round(reach, 3), "apron_m": APRON_M})
    np.save(DEST / "heights.npy", heights)

    edits = json.loads((SOURCE / "manifest_edits.json").read_text())
    edits["building_instances"] = [
        instance(index, *placement) for index, placement in enumerate(PLACEMENTS)
    ]
    # Inspection daylight: the workshop is authored as a stormy dusk with a fixed camera exposure
    # (sun 7.5 degrees up, overcast 0.96). Switching that authored override off returns the act's
    # own campaign atmosphere and auto-exposure, the daylight of the default Act I renders.
    edits["environment_lighting"]["enabled"] = False
    (DEST / "manifest_edits.json").write_text(json.dumps(edits, indent=2) + "\n")

    receipt = {
        "purpose": "multi-instance building runtime test revision (urban intro F1); not district content",
        "source": str(SOURCE.relative_to(ROOT)),
        "source_manifest_sha256": sha256(SOURCE / "manifest.v1.json"),
        "source_edits_sha256": sha256(SOURCE / "manifest_edits.json"),
        "source_heights_sha256": sha256(SOURCE / manifest["terrain"]["heights"]),
        "heights_sha256": sha256(DEST / "heights.npy"),
        "edits_sha256": sha256(DEST / "manifest_edits.json"),
        "changed_height_samples": int(np.count_nonzero(heights != original)),
        "pads": pads,
        "building_instances": len(PLACEMENTS),
        "environment_lighting": "authored override off: the act's campaign atmosphere and auto-exposure",
    }
    (DEST / "urban-runtime-test.receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    main()
