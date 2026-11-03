#!/usr/bin/env python3
"""Look at the spring bridge map from the ground: fixed viewpoints rendered by the game's own
`screenshot` path in the owner's locked look (deltap-real, 2,048 colours, 1024x576), for judging
changes to the map by eye. Viewpoints are derived from the map (bridge, route, battlefield).

    KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem python3 tools/worldgen/spring_bridge_lookbook.py TAG [--only a,b] [--workshop NAME]
Outputs: /home/alex/panzer/captures/<checkout>-lookbook_<TAG>_<view>.jpg (and a contact sheet).
"""
import argparse
import json
import math
import os
import subprocess
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
ASSET_ROOT = Path(os.environ.get('KESSEL_ASSET_ROOT', '/windows/shop/panzerrechnersystem'))
CAPTURES = Path('/home/alex/panzer/captures')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('tag')
    ap.add_argument('--only')
    ap.add_argument('--workshop', default='spring-bridge-v2')
    a = ap.parse_args()
    ws = ASSET_ROOT/'assets/worldgen/workshops'/a.workshop
    m = json.loads((ws/'manifest.v1.json').read_text())
    e = json.loads((ws/'manifest_edits.json').read_text())
    H = np.load(ws/'heights.npy')
    res, sp = m['terrain']['res'], m['terrain']['spacing']
    half = (res-1)*sp/2

    def ground(x, z):
        gx, gz = (x+half)/sp, (z+half)/sp
        ix, iz = int(np.clip(gx, 0, res-2)), int(np.clip(gz, 0, res-2))
        tx, tz = gx-ix, gz-iz
        return float(H[iz, ix]*(1-tx)*(1-tz)+H[iz, ix+1]*tx*(1-tz)+H[iz+1, ix]*(1-tx)*tz+H[iz+1, ix+1]*tx*tz)

    c = m['crossings'][0]
    cx, cz = c['center']
    deck = c['deck_y']
    views = {
        'field_west': ((cx-420, 1.7, cz-60), (cx, 4.0, cz)),
        'commander_approach': ((cx-300, 2.9, cz-2), (cx, deck+2.0, cz)),
        'bridge_deck': ((cx-30, None, cz), (cx+200, deck+1.5, cz)),
        'riverbank': ((cx+44, 1.6, cz-120), (cx, deck-2.0, cz)),
        'elevated_town': ((cx-380, 70.0, cz-240), (cx, 0.0, cz)),
        'column_east': ((cx+420, 1.7, cz-30), (cx+700, 1.0, cz)),
        'grass_close': ((cx-560, 1.2, cz+180), (cx-540, 0.2, cz+190)),
    }
    only = set(a.only.split(',')) if a.only else None
    env = dict(os.environ, KESSEL_RENDERER='raymarch', KESSEL_INTERNAL_RES='1024x576', KESSEL_LOOK='deltap-real',
               KESSEL_REAL_STEPS='15,15,7', KESSEL_GRAPHICS_PRESET='high', KESSEL_RECON_MANIFEST=str(ws/'manifest.v1.json'),
               KESSEL_SHOT_CANYON='1', KESSEL_CANYON_SEED='29', KESSEL_CAPTURES_DIR=str(CAPTURES), KESSEL_CAPTURE_4K_JPG='1',
               KESSEL_ASSET_ROOT=str(ASSET_ROOT))
    binary = ROOT/'target/release/panzerrechnersystem'
    made = []
    for name, (eye, target) in views.items():
        if only and name not in only:
            continue
        ex, ey, ez = eye
        ey = deck+1.7 if ey is None else ground(ex, ez)+ey
        tx, ty, tz = target
        ty = ground(tx, tz)+ty if name not in ('commander_approach', 'bridge_deck', 'riverbank') else ty
        dx, dy, dz = tx-ex, ty-ey, tz-ez
        yaw = math.degrees(math.atan2(dx, -dz))
        pitch = math.degrees(math.atan2(dy, math.hypot(dx, dz)))
        out = f'lookbook_{a.tag}_{name}.png'
        r = subprocess.run([str(binary), 'screenshot', out, f'{ex:.2f}', f'{ey:.2f}', f'{ez:.2f}', f'{yaw:.2f}', f'{pitch:.2f}', '0.0'],
                           env=env, cwd=ROOT, capture_output=True, text=True, timeout=900)
        ok = r.returncode == 0
        print(f'{name}: {"ok" if ok else "FAILED"}', '' if ok else r.stderr[-400:])
        if ok:
            made.append(name)
    print(json.dumps(made))


if __name__ == '__main__':
    main()
