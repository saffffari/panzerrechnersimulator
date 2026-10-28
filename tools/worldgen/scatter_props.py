#!/usr/bin/env python3
"""Engine-native procedural props for the strewn-field scatter layer (docs/SCATTER.md). Metres; sRGB.

The purchased PSX kit has crates, barrels, bricks, tools and tins but none of the specifically
1942 battlefield litter the roads of the pocket carried. These are inferred composite shapes of
attested objects (fired cartridge cases of the calibres present on the map, shell splinters,
frozen earth thrown from craters, masonry, blood frozen into snow, steel helmets, mess tins,
canteens, gas-mask canisters, track links, armour spall, snow lumps and drift tails). None is a
replica of a named pattern beyond its gross dimensions; nothing readable is on any of them.

    python tools/worldgen/scatter_props.py [--root PATH] [--force]

Writes assets/authoring/scatter/*.glb and scatter-props.receipt.json. Existing different output
files are preserved unless --force is passed.
"""
import argparse
import hashlib
import json
import math
import random
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from canyon_v2_assets import Mesh, SNOW  # noqa: E402

PREFIX = 'assets/authoring/scatter/'
RECEIPT = 'scatter-props.receipt.json'
BRASS = (158, 126, 62)
BRASS_DARK = (118, 92, 44)
STEEL = (58, 57, 54)
STEEL_DARK = (38, 37, 35)
RUST = (82, 56, 40)
FROZEN_EARTH = (84, 72, 60)
EARTH_DARK = (56, 46, 38)
MASONRY = (142, 112, 92)
MASONRY_PALE = (170, 150, 132)
BLOOD = (74, 16, 18)
BLOOD_DARK = (52, 12, 14)
FIELD_GREY = (92, 95, 88)
OLIVE = (96, 98, 70)
ALUMINIUM = (150, 152, 148)
CANVAS = (118, 112, 96)
SCORCH = (34, 29, 25)
TRACK = (66, 64, 60)


def rotated_about_y(m, degrees):
    a = math.radians(degrees)
    c, s = math.cos(a), math.sin(a)
    R = np.array([[c, 0, s], [0, 1, 0], [-s, 0, c]])
    pts = np.asarray(m.points) @ R.T
    nrm = np.asarray(m.normals) @ R.T
    m.points, m.normals = pts.tolist(), nrm.tolist()
    return m


def rest(m):
    """Rest the mesh on y = 0 and centre it on x/z."""
    pts = np.asarray(m.points)
    pts[:, 1] -= pts[:, 1].min()
    pts[:, 0] -= (pts[:, 0].min() + pts[:, 0].max()) * 0.5
    pts[:, 2] -= (pts[:, 2].min() + pts[:, 2].max()) * 0.5
    m.points = pts.tolist()
    return m


def cartridge(length, diameter, seed):
    """A fired case lying on its side along +Z, mouth toward +Z, slight taper and a rim."""
    m = Mesh()
    r = diameter * 0.5
    m.limb((0, r, -length * 0.5), (0, r * 0.96, length * 0.5), r, r * 0.74, BRASS, sides=8)
    m.limb((0, r, -length * 0.5), (0, r, -length * 0.5 + length * 0.04), r * 1.08, r * 1.0, BRASS_DARK, sides=8)
    return rest(rotated_about_y(m, random.Random(seed).uniform(-8, 8)))


def shard(seed, size):
    """A jagged steel splinter: a thin, twisted fan of triangles resting on the ground."""
    rnd = random.Random(seed)
    m = Mesh()
    n = rnd.randint(4, 7)
    pts = []
    for i in range(n):
        a = i / n * math.tau + rnd.uniform(-0.3, 0.3)
        rr = size * rnd.uniform(0.35, 1.0) * (0.5 if i % 2 else 1.0)
        pts.append((math.cos(a) * rr, 0.004 + rnd.uniform(0, size * 0.18), math.sin(a) * rr * 0.6))
    c = (0.0, 0.01, 0.0)
    for i in range(n):
        a, b = pts[i], pts[(i + 1) % n]
        colour = STEEL if i % 2 else STEEL_DARK
        m.tri(c, a, b, colour)
        m.tri(c, b, a, colour)
    return rest(m)


def clod(seed, size):
    rnd = random.Random(seed)
    m = Mesh()
    m.ellipsoid((0, size * 0.3, 0), (size, size * 0.6, size * rnd.uniform(0.7, 1.2)), FROZEN_EARTH, seed=seed, rock=True)
    m.ellipsoid((size * 0.3, size * 0.2, size * 0.2), (size * 0.5, size * 0.35, size * 0.5), EARTH_DARK, seed=seed + 1, rock=True)
    return rest(m)


def masonry(seed, size):
    rnd = random.Random(seed)
    m = Mesh()
    m.box((0, size * 0.22, 0), (size, size * 0.44, size * rnd.uniform(0.5, 0.8)), MASONRY)
    m.ellipsoid((size * 0.25, size * 0.3, 0), (size * 0.5, size * 0.4, size * 0.4), MASONRY_PALE, seed=seed, rock=True)
    return rest(rotated_about_y(m, rnd.uniform(0, 360)))


def blood_decal(seed, radius):
    """A flat irregular stain a centimetre above the contact, dark on the snow."""
    rnd = random.Random(seed)
    m = Mesh()
    n = 14
    ring = []
    for i in range(n):
        a = i / n * math.tau
        rr = radius * (0.55 + 0.45 * abs(math.sin(a * 2.7 + seed)) * rnd.uniform(0.6, 1.0))
        ring.append((math.cos(a) * rr, 0.012, math.sin(a) * rr))
    c = (rnd.uniform(-0.1, 0.1) * radius, 0.012, rnd.uniform(-0.1, 0.1) * radius)
    for i in range(n):
        a, b = ring[i], ring[(i + 1) % n]
        m.tri(c, b, a, BLOOD if i % 3 else BLOOD_DARK)
    # spatter drops along a direction
    d = rnd.uniform(0, math.tau)
    for k in range(rnd.randint(3, 7)):
        t = radius * (1.1 + k * 0.35)
        x, z = math.cos(d) * t + rnd.uniform(-0.1, 0.1), math.sin(d) * t + rnd.uniform(-0.1, 0.1)
        s = radius * rnd.uniform(0.06, 0.16)
        m.tri((x - s, 0.012, z - s), (x + s, 0.012, z - s * 0.5), (x, 0.012, z + s), BLOOD_DARK)
    return m


def helmet(colour, seed):
    rnd = random.Random(seed)
    m = Mesh()
    m.ellipsoid((0, 0.09, 0), (0.25, 0.19, 0.29), colour, seed=seed)
    m.box((0, 0.015, 0), (0.29, 0.02, 0.33), colour)
    # lying on its crown or its rim; tilt a little
    return rest(rotated_about_y(m, rnd.uniform(0, 360)))


def mess_tin():
    m = Mesh()
    m.box((0, 0.045, 0), (0.17, 0.09, 0.10), ALUMINIUM)
    m.box((0, 0.095, 0), (0.175, 0.01, 0.105), (120, 122, 118))
    return rest(m)


def canteen():
    m = Mesh()
    m.ellipsoid((0, 0.045, 0), (0.09, 0.22, 0.16), CANVAS)
    m.limb((0, 0.05, 0.1), (0, 0.05, 0.14), 0.02, 0.018, STEEL)
    return rest(rotated_about_y(m, 90))


def gas_mask_canister():
    m = Mesh()
    m.limb((-0.135, 0.06, 0), (0.135, 0.06, 0), 0.062, 0.062, FIELD_GREY, sides=10)
    m.limb((0.135, 0.06, 0), (0.15, 0.06, 0), 0.065, 0.065, (78, 80, 74), sides=10)
    return rest(m)


def track_link(width, pitch, colour):
    m = Mesh()
    m.box((0, 0.03, 0), (width, 0.06, pitch * 0.9), colour)
    m.box((0, 0.075, 0), (width * 0.3, 0.03, pitch * 0.5), STEEL_DARK)
    m.limb((-width * 0.5, 0.03, pitch * 0.5), (width * 0.5, 0.03, pitch * 0.5), 0.02, 0.02, STEEL, sides=6)
    return rest(m)


def plate_fragment(seed):
    rnd = random.Random(seed)
    m = Mesh()
    m.box((0, 0.02, 0), (rnd.uniform(0.3, 0.7), 0.04, rnd.uniform(0.2, 0.5)), STEEL_DARK)
    m.tri((0.2, 0.045, 0.1), (0.35, 0.05, -0.2), (0.05, 0.045, -0.25), RUST)
    return rest(rotated_about_y(m, rnd.uniform(0, 360)))


def snow_lump(seed, size):
    rnd = random.Random(seed)
    m = Mesh()
    m.ellipsoid((0, size * 0.22, 0), (size, size * 0.45, size * rnd.uniform(0.7, 1.3)), SNOW, seed=seed)
    return rest(m)


def drift_tail(length):
    """A low wind tail: the drift that forms behind an obstacle, pointing along +Z (downwind)."""
    m = Mesh()
    m.ellipsoid((0, 0.1, length * 0.25), (1.1, 0.34, length), SNOW, seed=3)
    m.ellipsoid((0.3, 0.07, length * 0.45), (0.7, 0.2, length * 0.8), (226, 230, 236), seed=5)
    return rest(m)


def burnt_board(seed):
    rnd = random.Random(seed)
    m = Mesh()
    m.box((0, 0.015, 0), (rnd.uniform(0.6, 1.3), 0.03, 0.14), SCORCH)
    return rest(rotated_about_y(m, rnd.uniform(0, 360)))


def stretcher():
    m = Mesh()
    m.limb((-0.3, 0.05, -1.1), (-0.3, 0.05, 1.1), 0.02, 0.02, (110, 92, 70), sides=6)
    m.limb((0.3, 0.05, -1.1), (0.3, 0.05, 1.1), 0.02, 0.02, (110, 92, 70), sides=6)
    m.box((0, 0.04, 0), (0.6, 0.015, 1.8), CANVAS)
    return rest(m)


def meshes():
    out = {
        'case_792': cartridge(0.057, 0.012, 1),
        'case_762': cartridge(0.054, 0.0125, 2),
        'case_45mm': cartridge(0.310, 0.056, 3),
        'case_76mm': cartridge(0.385, 0.090, 4),
        'case_88mm': cartridge(0.571, 0.112, 5),
        'mess_tin': mess_tin(),
        'canteen': canteen(),
        'gas_mask_canister': gas_mask_canister(),
        'helmet_m40': helmet(FIELD_GREY, 11),
        'helmet_ssh40': helmet(OLIVE, 12),
        'track_link_tiger': track_link(0.725, 0.13, TRACK),
        'track_link_t34': track_link(0.50, 0.172, TRACK),
        'stretcher': stretcher(),
        'drift_tail_short': drift_tail(2.2),
        'drift_tail_long': drift_tail(4.5),
    }
    for i in range(4):
        out[f'shard_{i}'] = shard(20 + i, [0.14, 0.22, 0.32, 0.45][i])
        out[f'clod_{i}'] = clod(30 + i, [0.16, 0.24, 0.34, 0.5][i])
        out[f'masonry_{i}'] = masonry(40 + i, [0.3, 0.42, 0.55, 0.75][i])
        out[f'blood_{i}'] = blood_decal(50 + i, [0.28, 0.4, 0.55, 0.75][i])
        out[f'snow_lump_{i}'] = snow_lump(60 + i, [0.25, 0.4, 0.6, 0.9][i])
        out[f'plate_fragment_{i}'] = plate_fragment(70 + i)
        out[f'burnt_board_{i}'] = burnt_board(80 + i)
    return out


def build(root, force=False):
    root = Path(root)
    dest = root / PREFIX
    dest.mkdir(parents=True, exist_ok=True)
    receipt = {}
    staging = Path(__file__).resolve().parents[2] / 'artifacts' / 'authoring' / 'scatter-props-staging'
    staging.mkdir(parents=True, exist_ok=True)
    for name, mesh in meshes().items():
        out = dest / (name + '.glb')
        staged = staging / (name + '.glb')
        rec = mesh.write(staged)
        data = staged.read_bytes()
        staged.unlink()
        if out.exists() and out.read_bytes() != data and not force:
            raise RuntimeError(f'Preserving existing different output {out}; pass --force to rewrite')
        out.write_bytes(data)
        rec.update(path=PREFIX + name + '.glb', sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))
        receipt[name] = rec
    doc = dict(schema='panzer.scatter-props', tool='tools/worldgen/scatter_props.py', version=1,
               source='authored procedural geometry; inferred composite shapes of attested battlefield litter, not replicas',
               assets=receipt)
    (dest / RECEIPT).write_text(json.dumps(doc, indent=1, sort_keys=True) + '\n')
    return doc


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--force', action='store_true')
    args = parser.parse_args()
    doc = build(args.root, args.force)
    print(json.dumps({k: dict(triangles=v['triangles'], bounds=v['bounds']) for k, v in doc['assets'].items()}, indent=1))
