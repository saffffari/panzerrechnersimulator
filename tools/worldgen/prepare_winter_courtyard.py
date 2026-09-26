#!/usr/bin/env python3
"""Fork Map 1 without changing its source; prepare a blended pad for the live assembly."""
from pathlib import Path
import hashlib
import json
import shutil
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/worldgen/revisions/act1-winter-approach/20260913-v1'
DEST = ROOT / 'assets/worldgen/workshops/act1-winter-courtyard-20260925'
ANCHOR = [1880.0, 5.4, 165.0]


def main():
    if DEST.exists():
        raise SystemExit('Working map already exists; preserving current edits.')
    DEST.mkdir(parents=True)
    for name in ['manifest.v1.json', 'manifest_edits.json', 'features.json', 'level_design.v1.json']:
        if (SOURCE / name).exists():
            shutil.copy2(SOURCE / name, DEST / name)
    manifest = json.loads((SOURCE / 'manifest.v1.json').read_text())
    original = np.load(SOURCE / 'heights.npy')
    heights = original.copy()
    z, x = np.indices(heights.shape, dtype=np.float32)
    half = (len(heights) - 1) / 2
    spacing = manifest['terrain']['spacing']
    x = (x - half) * spacing
    z = (z - half) * spacing
    # Full foundation envelope includes gatehouse; the 10 m smooth apron meets unchanged snow.
    outside = np.maximum.reduce([1866-x, x-1904, 154-z, z-197, np.zeros_like(x)])
    t = np.clip(outside / 10, 0, 1)
    weight = 1 - t*t*(3-2*t)
    heights = (heights*(1-weight) + ANCHOR[1]*weight).astype(np.float32)
    np.save(DEST / 'heights.npy', heights)
    edits = json.loads((DEST / 'manifest_edits.json').read_text())
    edits['environment_lighting'].update(enabled=True, camera_exposure=1.35,
        sun_azimuth_deg=140, sun_elevation_deg=26, fog_density=.0048,
        overcast=.88, storm=.58, twilight=0)
    (DEST / 'manifest_edits.json').write_text(json.dumps(edits, indent=2)+'\n')
    receipt = {'source':str(SOURCE), 'anchor':ANCHOR,
        'base_placements_preserved':len(manifest['placements']),
        'existing_edit_ops_preserved':len(edits['ops']),
        'changed_height_samples':int(np.count_nonzero(heights != original)),
        'pad_bounds_xz':[1866,154,1904,197], 'apron_metres':10,
        'source_height_sha256':hashlib.sha256((SOURCE/'heights.npy').read_bytes()).hexdigest(),
        'working_height_sha256':hashlib.sha256((DEST/'heights.npy').read_bytes()).hexdigest(),
        'assembly':'assets/worldgen/prefabs/ruined-house-courtyard-v1.json'}
    (DEST/'winter-courtyard.receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps(receipt,indent=2))


if __name__ == '__main__':
    main()
