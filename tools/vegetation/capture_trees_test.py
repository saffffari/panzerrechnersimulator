#!/usr/bin/env python3
"""Engine stills of the spring 1943 vegetation on the spring-bridge-trees-test workshop: the
lineup rows (every asset) and views over the town, the river and the woods. Real renders through
the locked look (deltap-real, 1024x576, raymarch); one GPU process at a time, and none while the
owner's demo build or another agent's engine process is running.

    python3 tools/vegetation/capture_trees_test.py [lineup|map|all] [ROW,...]
"""
import json
import math
import os
import subprocess
import sys
import time
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
ASSET_ROOT = Path(os.environ.get('KESSEL_ASSET_ROOT', '/windows/shop/panzerrechnersystem'))
TEST = ASSET_ROOT / 'assets/worldgen/workshops/spring-bridge-trees-test'
BIN = ROOT / 'target/release/panzerrechnersystem'
STAMP = time.strftime('%Y%m%d')


def ground_fn(world):
    m = json.loads((world / 'manifest.v1.json').read_text())
    h = np.load(world / m['terrain']['heights'])
    sp = m['terrain']['spacing']
    half = (h.shape[0] - 1) * sp / 2

    def ground(x, z):
        gx = np.clip((x + half) / sp, 0, h.shape[1] - 1.001)
        gz = np.clip((z + half) / sp, 0, h.shape[0] - 1.001)
        ix, iz = int(gx), int(gz)
        tx, tz = gx - ix, gz - iz
        return float((h[iz, ix] * (1 - tx) + h[iz, ix + 1] * tx) * (1 - tz) + (h[iz + 1, ix] * (1 - tx) + h[iz + 1, ix + 1] * tx) * tz)
    return ground


def angles(pos, target):
    dx, dy, dz = np.array(target) - np.array(pos)
    return [math.degrees(math.atan2(dx, -dz)), math.degrees(math.atan2(dy, math.hypot(dx, dz)))]


def wait_for_gpu():
    # the owner's demo, or any other engine process on the GPU (one at a time)
    while subprocess.run(['pgrep', '-f', 'target/release/panzerrechnersystem( |$)'], capture_output=True).returncode == 0:
        print('another engine process is on the GPU; waiting', flush=True)
        time.sleep(30)


def shoot(world, name, pos, target, fog=0.0008):
    env = {k: v for k, v in os.environ.items() if not k.startswith('KESSEL_')}
    env.update(KESSEL_ASSET_ROOT=str(ASSET_ROOT), KESSEL_RECON_MANIFEST=str(world / 'manifest.v1.json'),
               KESSEL_RENDERER='raymarch', KESSEL_GRAPHICS_PRESET='high', KESSEL_INTERNAL_RES='1024x576',
               KESSEL_LOOK='deltap-real', KESSEL_REAL_STEPS='15,15,7', KESSEL_DITHER_PATTERN='screen_bayer8',
               KESSEL_GEOMETRY_AA='msaa4', KESSEL_SOURCE_VISIBILITY='maps', KESSEL_TRANSPORT_QUALITY='full',
               KESSEL_CANYON_V2='1', KESSEL_SKY_START='16:00', KESSEL_CAPTURES_DIR='/home/alex/panzer/captures',
               KESSEL_CAPTURE_4K_JPG='1')
    out = f'captures/veg1943_{name}_{STAMP}.png'
    cmd = ['nice', '-n', '10', str(BIN), 'screenshot', out] + [str(round(v, 3)) for v in list(pos) + angles(pos, target) + [fog]]
    wait_for_gpu()
    print('render', name, flush=True)
    log = ROOT / 'target' / 'veg1943_logs' / f'{name}.log'
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open('w') as f:
        subprocess.run(cmd, cwd=ROOT, env=env, stdout=f, stderr=subprocess.STDOUT)
    # The still is the receipt (the process can exit non-zero after writing it).
    wrote = [line for line in log.read_text().splitlines() if '] wrote ' in line]
    print(wrote[-1] if wrote else f'FAILED {name}: see {log}', flush=True)
    return 0 if wrote else 1


def densest(world, prefix, cell=120.0):
    """Centre of the cell holding the most placements of spring1943 assets starting with `prefix`."""
    m = json.loads((world / 'manifest.v1.json').read_text())
    ids = {i for i, a in enumerate(m['assets']) if '/spring1943/' in a and Path(a).stem.startswith(prefix)}
    pts = np.array([[p[1], p[3]] for p in m['placements'] if p[0] in ids and p[1] > -700])
    keys, counts = np.unique(np.floor(pts / cell).astype(int), axis=0, return_counts=True)
    k = keys[np.argmax(counts)]
    inside = pts[np.all(np.floor(pts / cell).astype(int) == k, axis=1)]
    return inside.mean(axis=0)


def map_views(world):
    g = ground_fn(world)
    v = []

    def at(name, x, z, up, tx, tz, tup):
        v.append((name, [x, g(x, z) + up, z], [tx, g(tx, tz) + tup, tz]))
    at('map_start_east', -395, -8, 6.0, -150, -10, 6.0)          # ahead of the Tiger at the start, toward the town
    at('map_westbank_bridge', -75, 60, 3.0, 2, -1, 4.0)          # down to the bridge and the river trees
    at('map_riverbank_north', 60, 260, 3.0, 0, 60, 4.0)          # along the east bank, willows and poplars
    at('map_town_aerial', -320, 180, 0.0, 0, -10, 0.0)
    v[-1][1][1] = 120.0
    def toward_centre(c, d):
        return c - c / max(np.linalg.norm(c), 1.0) * d   # the camera on the map's side of the subject
    w = densest(world, 'tree_oak')
    cam = toward_centre(w, 160.0)
    at('map_wood_edge', cam[0], cam[1], 2.5, w[0], w[1], 6.0)    # the densest oak wood, from 160 m off
    d = densest(world, 'tree_linden_shattered')
    cam = toward_centre(d, 100.0)
    at('map_damaged', cam[0], cam[1], 3.0, d[0], d[1], 5.0)      # where the shattered limes stand
    return v


def main(argv):
    what = argv[0] if argv else 'all'
    rows = set(int(r) for r in argv[1].split(',')) if len(argv) > 1 else None
    rec = json.loads((TEST / 'vegetation_swap.receipt.json').read_text())
    fails = 0
    if what in ('lineup', 'all'):
        for row in rec['lineup']:
            if rows and row['row'] not in rows:
                continue
            fails += bool(shoot(TEST, f"lineup_row{row['row']}", row['camera'], row['target'], fog=0.0003))
    if what in ('map', 'all'):
        for name, pos, target in map_views(TEST):
            fails += bool(shoot(TEST, name, pos, target))
    sys.exit(1 if fails else 0)


if __name__ == '__main__':
    main(sys.argv[1:])
