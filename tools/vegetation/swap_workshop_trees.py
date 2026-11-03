#!/usr/bin/env python3
"""Copy a spring workshop and replace its library trees with the spring 1943 set, plus a lineup
of every generated asset in a cleared field for review stills.

    python3 tools/vegetation/swap_workshop_trees.py SRC_WORKSHOP DST_WORKSHOP

The source workshop is only read. Species follow the place: white willow, black poplar and birch
on the river's banks (with willow scrub), lime, horse chestnut, apple and cherry in the town's
streets and gardens, oak, birch, wild pear and poplar in the woods and field margins, with hazel,
hawthorn, blackthorn, elder and dog rose under and around them. Damage follows the fighting: the
nearer the bridge and the houses, the more trees burnt, shattered or felled. Every choice is a
counter hash of the placement index (no RNG state). Writes DST/vegetation_swap.receipt.json with
the lineup's rows and cameras.
"""
import json
import math
import shutil
import sys
from collections import Counter
from pathlib import Path

import numpy as np

VEG = 'assets/vegetation/spring1943/'
LIBRARY_TREES = ('tree_1', 'tree_5', 'tree_6', 'tree_7', 'tree_8', 'tree_9', 'pine_tree_n_1', 'pine_tree_n_2')
BRIDGE = np.array([2.0, -1.0])


def h01(i, salt):
    n = (i * 2654435761 + salt * 40503 + 0x9E3779B9) & 0xFFFFFFFF
    n ^= n >> 15
    n = (n * 2246822519) & 0xFFFFFFFF
    n ^= n >> 13
    return (n & 0xFFFFFF) / float(0x1000000)


def pick(table, u):
    total = sum(w for _n, w in table)
    acc = 0.0
    for name, w in table:
        acc += w / total
        if u <= acc:
            return name
    return table[-1][0]


def seg_dist(p, line):
    best = 1e18
    for a, b in zip(line[:-1], line[1:]):
        ab = b - a
        t = np.clip(np.dot(p - a, ab) / max(np.dot(ab, ab), 1e-9), 0, 1)
        best = min(best, np.linalg.norm(p - (a + ab * t)))
    return best


def main(src, dst):
    src, dst = Path(src), Path(dst)
    assert src.resolve() != dst.resolve()
    if dst.exists():
        shutil.rmtree(dst)
    shutil.copytree(src, dst)
    m = json.loads((src / 'manifest.v1.json').read_text())
    edits = json.loads((src / 'manifest_edits.json').read_text())
    heights = np.load(src / m['terrain']['heights'])
    sp = m['terrain']['spacing']
    half = (heights.shape[0] - 1) * sp / 2

    def ground(x, z):
        gx = np.clip((x + half) / sp, 0, heights.shape[1] - 1.001)
        gz = np.clip((z + half) / sp, 0, heights.shape[0] - 1.001)
        ix, iz = int(gx), int(gz)
        tx, tz = gx - ix, gz - iz
        a, b, c, d = heights[iz, ix], heights[iz, ix + 1], heights[iz + 1, ix], heights[iz + 1, ix + 1]
        return float((a * (1 - tx) + b * tx) * (1 - tz) + (c * (1 - tx) + d * tx) * tz)

    river = np.array([[p[0], p[1]] for p in m['river']['centreline']])
    houses = np.array([[b['x'], b['z']] for b in edits['building_instances']])
    catalog = json.loads(Path(m.get('_veg_root', '/windows/shop/panzerrechnersystem') + '/' + VEG + 'spring1943.catalog.json').read_text())
    names = [a['name'] for a in catalog['assets']]
    assets = list(m['assets'])
    index = {}

    def asset(name):
        assert name in names, name
        path = VEG + name + '.glb'
        if path not in index:
            if path not in assets:
                assets.append(path)
            index[path] = assets.index(path)
        return index[path]

    def variants(prefix):
        return [n for n in names if n.startswith(prefix) and len(n) == len(prefix) + 1]

    library = {i for i, a in enumerate(assets) if Path(a).stem in LIBRARY_TREES}
    out, swapped, added = [], Counter(), Counter()
    for k, p in enumerate(m['placements']):
        if p[0] not in library:
            out.append(p)
            continue
        x, y, z = p[1], p[2], p[3]
        q = np.array([x, z])
        d_river = seg_dist(q, river) - m['river']['half_width_m']
        d_house = float(np.min(np.linalg.norm(houses - q, axis=1)))
        d_bridge = float(np.linalg.norm(q - BRIDGE))
        if d_river < 40:
            zone = 'river'
            sp_name = pick([('willow', 4), ('poplar', 3), ('birch', 2)], h01(k, 1))
        elif d_house < 70 or d_bridge < 260:
            zone = 'town'
            sp_name = pick([('linden', 4), ('chestnut', 3), ('apple', 2), ('cherry', 1.5), ('birch', 1)], h01(k, 1))
        else:
            zone = 'wood'
            sp_name = pick([('oak', 4), ('birch', 3), ('poplar', 1), ('pear', 1.2), ('linden', 1), ('cherry', 0.4)], h01(k, 1))
        war = 0.42 if d_bridge < 320 else (0.30 if d_house < 90 else 0.05)
        u = h01(k, 2)
        name = None
        if u < war:
            kind = pick([('burnt', 4), ('shattered', 4), ('fallen', 1.5)], h01(k, 3))
            if kind == 'fallen':
                pool = [n for n in names if n.startswith('fallen_' + sp_name)] or [n for n in names if n.startswith('fallen_')]
            else:
                pool = [n for n in names if n.startswith(f'tree_{sp_name}_{kind}_')] or \
                       [n for n in names if f'_{kind}_' in n and n.startswith('tree_')]
            name = pool[int(h01(k, 4) * len(pool))]
        elif zone == 'wood' and h01(k, 5) < 0.22:
            name = pick([('shrub_hazel_a', 3), ('shrub_hazel_b', 3), ('shrub_hawthorn_a', 2), ('shrub_hawthorn_b', 2),
                         ('shrub_blackthorn_a', 1.5), ('shrub_blackthorn_b', 1.5), ('shrub_elder_a', 1), ('shrub_elder_b', 1),
                         ('shrub_dogrose_a', 1), ('shrub_dogrose_b', 1)], h01(k, 6))
        else:
            v = variants(f'tree_{sp_name}_')
            name = v[int(h01(k, 4) * len(v))]
        scale = round(0.88 + 0.24 * h01(k, 7), 3)
        yaw = round(h01(k, 8) * 360.0, 1)
        out.append([asset(name), x, round(ground(x, z) - 0.05, 3), z, yaw, scale])
        swapped[name.split('_')[0] + ':' + zone] += 1
        # willow scrub and the odd shrub along the banks
        if zone == 'river' and h01(k, 9) < 0.7:
            for j in range(2):
                a = h01(k, 10 + j) * 2 * math.pi
                r = 4 + 6 * h01(k, 12 + j)
                sx, sz = x + r * math.cos(a), z + r * math.sin(a)
                if seg_dist(np.array([sx, sz]), river) - m['river']['half_width_m'] < 2:
                    continue
                sn = 'shrub_willowscrub_' + 'ab'[int(h01(k, 14 + j) * 2)]
                out.append([asset(sn), round(sx, 3), round(ground(sx, sz) - 0.05, 3), round(sz, 3),
                            round(h01(k, 16 + j) * 360, 1), round(0.85 + 0.3 * h01(k, 18 + j), 3)])
                added['shrub_willowscrub'] += 1
    # The lineup: every asset in rows of eight in a cleared field west of the start.
    rows = [names[i:i + 8] for i in range(0, len(names), 8)]
    by_name = {a['name']: a for a in catalog['assets']}
    x0, z0, dz = -980.0, -560.0, 70.0
    lo = np.array([x0 - 30, z0 - 40])
    hi = np.array([x0 + 200, z0 + dz * len(rows) + 20])
    out = [p for p in out if not (lo[0] < p[1] < hi[0] and lo[1] < p[3] < hi[1])]
    lineup = []
    for r, row in enumerate(rows):
        z = z0 + dz * r
        x = x0
        placed = []
        for n in row:
            a = by_name[n]
            w = a['bounds_max'][0] - a['bounds_min'][0]
            xx = x - a['bounds_min'][0]
            out.append([asset(n), round(xx, 3), round(ground(xx, z) - 0.02, 3), z, 0.0, 1.0])
            placed.append(dict(name=n, x=round(xx, 2), z=z))
            x += w + 3.0
        width = x - x0
        tall = max(by_name[n]['bounds_max'][1] for n in row)
        cx = x0 + width / 2
        dist = max(width * 0.62, tall * 1.6, 25.0)
        cam = [cx, ground(cx, z - dist) + max(2.0, tall * 0.35), z - dist]
        target = [cx, ground(cx, z) + tall * 0.42, z]
        lineup.append(dict(row=r, assets=placed, camera=cam, target=target))
    m['assets'] = assets
    m['placements'] = out
    m['authoring'] = dict(m['authoring'], vegetation='tools/vegetation/swap_workshop_trees.py', vegetation_source=str(src))
    (dst / 'manifest.v1.json').write_text(json.dumps(m, separators=(',', ':')) + '\n')
    # The edits overlay pins the base placement count; it carries no index-addressed ops here.
    assert not edits.get('ops'), 'source edits carry placement ops; re-target them before swapping'
    edits['base_placements'] = len(out)
    (dst / 'manifest_edits.json').write_text(json.dumps(edits, indent=1) + '\n')
    receipt = dict(source=str(src), swapped=dict(sorted(swapped.items())), added=dict(added), lineup=lineup,
                   placements=len(out))
    (dst / 'vegetation_swap.receipt.json').write_text(json.dumps(receipt, indent=1) + '\n')
    print(json.dumps(dict(swapped=receipt['swapped'], added=receipt['added'], placements=len(out)), indent=1))


if __name__ == '__main__':
    main(*sys.argv[1:3])
