#!/usr/bin/env python3
"""Headless capture set for the strewn field (docs/SCATTER.md): the same viewpoints with the
field off (KESSEL_SCATTER=0) and on, a road, a crater field, a treeline, a wreck cluster and a
casualty field, each near and far, by day and by night, on the raymarch backend at 800x450.

    python tools/worldgen/scatter_shots.py [--root ASSET_ROOT] [--exe PATH] [--prefix scatter]
                                           [--sites road,crater,...] [--only before|after]
                                           [--times day,night] [--ranges near,far]

Output: captures/<prefix>_<site>_<range>_<time>_<before|after>.png and .log beside it.
"""
import argparse
import json
import math
import os
import subprocess
from pathlib import Path

import numpy as np

DEST = 'assets/worldgen/workshops/gaea-canyon-run-v2'
# site centres (x, z) chosen from the layer's own sources (tools/worldgen/scatter_layers.py):
# the road at the start, the densest crater cluster, the densest tree stand, the densest tank
# wreck column, the dissolved-unit casualty field. Heading: the direction the near camera looks.
SITES = {
    'road': dict(x=-791.0, z=600.0, heading=180.0),
    'crater': dict(x=625.5, z=567.1, heading=250.0),
    'treeline': dict(x=-586.5, z=-277.9, heading=90.0),
    'wrecks': dict(x=-776.6, z=484.9, heading=200.0),
    'casualties': dict(x=-610.2, z=-150.1, heading=120.0),
}
RANGES = {'near': (16.0, 2.4, -9.0), 'far': (95.0, 14.0, -9.5)}   # distance, eye height, pitch
TIMES = {'day': '1942-12-21T09:20:00Z', 'night': '1942-12-21T18:30:00Z'}


def ground(H, spacing, x, z):
    half = (H.shape[0] - 1) * 0.5
    gx, gz = x / spacing + half, z / spacing + half
    ix, iz = int(math.floor(gx)), int(math.floor(gz))
    u, v = gx - ix, gz - iz
    return float((1 - v) * ((1 - u) * H[iz, ix] + u * H[iz, ix + 1]) + v * ((1 - u) * H[iz + 1, ix] + u * H[iz + 1, ix + 1]))


def camera(H, spacing, site, rng):
    dist, height, pitch = RANGES[rng]
    h = math.radians(site['heading'])
    # the camera stands `dist` behind the site along its heading and looks at it
    look = (math.sin(h), math.cos(h))
    ex, ez = site['x'] - look[0] * dist, site['z'] - look[1] * dist
    ey = max(ground(H, spacing, ex, ez), ground(H, spacing, site['x'], site['z'])) + height
    yaw = math.degrees(math.atan2(look[0], -look[1]))
    return ex, ey, ez, yaw, pitch


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--exe', type=Path, default=None)
    parser.add_argument('--prefix', default='scatter')
    parser.add_argument('--sites', default=','.join(SITES))
    parser.add_argument('--only', choices=['before', 'after'], default=None)
    parser.add_argument('--times', default='day,night')
    parser.add_argument('--ranges', default='near,far')
    parser.add_argument('--weather', default='clear')
    args = parser.parse_args()
    root = args.root
    here = Path(__file__).resolve().parents[2]
    exe = args.exe or here / 'target' / 'release' / ('panzerrechnersystem.exe' if os.name == 'nt' else 'panzerrechnersystem')
    manifest = json.loads((root / DEST / 'manifest.v1.json').read_text())
    H = np.load(root / DEST / manifest['terrain']['heights']).astype(np.float64)
    spacing = float(manifest['terrain']['spacing'])
    runtime = here / 'artifacts' / 'runtime'
    env = {k: v for k, v in os.environ.items() if not k.startswith('KESSEL_')}
    env.setdefault('HOME', os.environ.get('USERPROFILE', str(Path.home())))
    for name in ('state', 'cache', 'config'):
        (runtime / name).mkdir(parents=True, exist_ok=True)
        env[f'XDG_{name.upper()}_HOME'] = str(runtime / name)
    env.update(KESSEL_ASSET_ROOT=str(root), KESSEL_RENDERER='raymarch', KESSEL_GRAPHICS_PRESET='high',
               KESSEL_INTERNAL_RES='800x450', KESSEL_DITHER_PATTERN='screen_bayer8', KESSEL_SOURCE_VISIBILITY='maps',
               KESSEL_TRANSPORT_QUALITY='full', KESSEL_RECON_MANIFEST=str(root / DEST / 'manifest.v1.json'),
               KESSEL_SHOT_CANYON='1', KESSEL_SHOT_WEATHER=args.weather, KESSEL_SCATTER_STATS='4')
    (here / 'captures').mkdir(exist_ok=True)
    for site_name in args.sites.split(','):
        site = SITES[site_name]
        for rng in args.ranges.split(','):
            ex, ey, ez, yaw, pitch = camera(H, spacing, site, rng)
            for time_name in args.times.split(','):
                for state in ('before', 'after'):
                    if args.only and state != args.only:
                        continue
                    out = f'{args.prefix}_{site_name}_{rng}_{time_name}_{state}.png'
                    e = dict(env, KESSEL_SKY_UTC=TIMES[time_name], KESSEL_SCATTER='0' if state == 'before' else '1')
                    cmd = [str(exe), 'screenshot', out, f'{ex:.1f}', f'{ey:.1f}', f'{ez:.1f}', f'{yaw:.1f}', f'{pitch:.1f}', '0.0']
                    log = here / 'captures' / out.replace('.png', '.log')
                    with open(log, 'w') as fh:
                        res = subprocess.run(cmd, cwd=here, env=e, stdout=fh, stderr=subprocess.STDOUT)
                    text = log.read_text(errors='replace')
                    vis = next((line.strip() for line in text.splitlines() if '[scatter] frame' in line), '')
                    print(json.dumps(dict(shot=out, exit=res.returncode, camera=[round(ex, 1), round(ey, 1), round(ez, 1), round(yaw, 1), pitch], visible=vis)), flush=True)


if __name__ == '__main__':
    main()
