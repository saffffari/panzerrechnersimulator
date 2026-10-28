#!/usr/bin/env python3
"""Engine-native, reproducible aftermath props for the Canyon v2 wreckage layer. Metres; sRGB albedo.

Inferred composite props of attested Kessel road conditions (docs/ENVSTORY.md "The Frozen Horses",
docs/research/PHOTO_REFERENCE_ANALYSIS_20260928.md [05-01], [05-02], [05-05]): a burnt and an
abandoned bonneted cargo lorry, dead draught horses in harness, panje sledges, a shell crater rim,
a gun-pit parapet, dropped rifles, greatcoat heaps and a loose spoked wheel. None is a replica of a
named make, unit or person; no purchased geometry is copied. Files whose names end in
`_wreck_burnt.glb` / `_wreck_detail.glb` under assets/authoring/battlefield/ receive the engine's
source-surface wreck collision (src/manifest_wreck_collision.rs); the low scars and kit do not.

Run from the checkout: python tools/worldgen/canyon_v2_wreckage_props.py [--root PATH]
Existing different output files are preserved (pass --force to rewrite them).
"""
import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from canyon_v2_assets import Mesh, SNOW, WOOD  # noqa: E402

PREFIX = 'assets/authoring/battlefield/'
RECEIPT = 'wreckage-props.receipt.json'
SOOT = (21, 20, 19)
RUST = (74, 54, 41)
SCORCH = (34, 29, 25)
EARTH = (58, 46, 36)
FROZEN_EARTH = (78, 68, 58)
WINTER_WASH = (158, 160, 156)
FIELD_GREY = (92, 95, 88)
CANVAS = (118, 112, 96)
HIDE = (66, 46, 33)
HIDE_PALE = (104, 86, 70)
COAT = (74, 72, 66)
COAT_B = (60, 65, 61)
GUNMETAL = (44, 43, 41)
STRAP = (36, 30, 26)


def rotated(mesh, axis, degrees, lift=True):
    """Rigidly rotate a built mesh about a principal axis; optionally rest it on y = 0."""
    a = math.radians(degrees)
    c, s = math.cos(a), math.sin(a)
    if axis == 'x':
        R = np.array([[1, 0, 0], [0, c, -s], [0, s, c]])
    elif axis == 'y':
        R = np.array([[c, 0, s], [0, 1, 0], [-s, 0, c]])
    else:
        R = np.array([[c, -s, 0], [s, c, 0], [0, 0, 1]])
    pts = np.asarray(mesh.points) @ R.T
    nrm = np.asarray(mesh.normals) @ R.T
    if lift:
        pts[:, 1] -= pts[:, 1].min()
    mesh.points = pts.tolist()
    mesh.normals = nrm.tolist()
    return mesh


def wheel(m, centre, radius, width, colour, spokes=8, rim=.07):
    """Spoked wheel in the X-normal plane (rolls along Z)."""
    x, y, z = centre
    n = 14
    for i in range(n):
        a = i * math.tau / n
        b = (i + 1) * math.tau / n
        p = (x, y + radius * math.sin(a), z + radius * math.cos(a))
        q = (x, y + radius * math.sin(b), z + radius * math.cos(b))
        m.limb(p, q, rim, rim, colour, 6)
    for i in range(spokes):
        a = i * math.tau / spokes
        m.limb((x, y, z), (x, y + radius * .9 * math.sin(a), z + radius * .9 * math.cos(a)), .028, .028, colour, 5)
    m.limb((x - width * .5, y, z), (x + width * .5, y, z), radius * .22, radius * .22, colour, 8)


def lorry(burnt):
    """Generic bonneted 3-tonne cargo lorry, slat sides, canvas bows; +Z is forward."""
    m = Mesh()
    body = SOOT if burnt else FIELD_GREY
    body_b = RUST if burnt else WINTER_WASH
    bed = SCORCH if burnt else WOOD
    frame = SOOT if burnt else GUNMETAL
    for x in (-.55, .55):
        m.box((x, .62, -.2), (.12, .16, 5.8), frame)
    for z in (2.25, -1.55):
        m.limb((-1.0, .5, z), (1.0, .5, z), .06, .06, GUNMETAL)
    for x, z in [(-.95, 2.25), (.95, 2.25), (-.98, -1.55), (.98, -1.55)]:
        if burnt and x > 0 and z < 0:
            wheel(m, (x, .5, z), .40, .22, RUST, 6, .035)   # tyre burnt away, bare rim
        else:
            wheel(m, (x, .5, z), .50, .26, frame, 8, .09)
    m.box((0, 1.12, 2.35), (1.5, .78, 1.5), body)            # bonnet
    m.box((0, .82, 3.1), (1.56, .5, .12), body_b)             # radiator face
    m.box((0, 1.5, 1.1), (2.0, 1.3, 1.5), body)               # cab
    m.box((0, 2.14, 1.1), (2.06, .08, 1.56), body_b)          # cab roof
    for x in (-1.0, 1.0):
        m.box((x, 1.0, 2.3), (.26, .12, 1.6), body_b)         # mudguards
    if burnt:
        m.box((0, 1.62, 1.86), (1.6, .62, .06), SCORCH)       # empty windscreen frame
    else:
        m.box((0, 1.62, 1.86), (1.6, .62, .05), (96, 112, 118))
        m.box((-1.03, 1.25, .55), (.06, .9, .9), body_b)       # driver's door hanging open
    m.box((0, .96, -1.45), (2.2, .10, 3.7), bed)              # flat bed
    for side in (-1, 1):
        if burnt:
            m.box((side * 1.25, .55, -1.45), (.06, .7, 3.6), SCORCH)   # slat side collapsed outward
        else:
            for k in range(4):
                m.box((side * 1.08, 1.12 + k * .17, -1.45), (.05, .07, 3.6), WOOD)
            for z in (-3.2, -1.45, .3):
                m.box((side * 1.1, 1.3, z), (.08, .7, .08), WOOD)
    m.box((0, 1.2, -3.28), (2.2, .5, .05), SCORCH if burnt else WOOD)   # tailboard
    bow = RUST if burnt else GUNMETAL
    for z in (-3.0, -1.95, -.9, .1):
        for side in (-1, 1):
            m.limb((side * 1.05, 1.3, z), (side * 1.05, 2.45, z), .03, .03, bow, 5)
        m.limb((-1.05, 2.45, z), (1.05, 2.45, z), .03, .03, bow, 5)
    if not burnt:
        m.box((0, 2.5, -1.45), (2.2, .06, 3.3), CANVAS)       # canvas tilt
        for side in (-1, 1):
            m.box((side * 1.1, 1.95, -1.45), (.04, 1.1, 3.3), CANVAS)
        m.box((0, 2.55, -1.45), (2.2, .05, 3.3), SNOW)        # snow on the tilt
        m.box((0, 2.2, 1.1), (2.0, .06, 1.5), SNOW)           # snow on the cab roof
    else:
        m.box((0, .98, -1.4), (1.6, .05, 2.2), SNOW)          # drifted snow on the open bed
    return m


def draught_horse(pose):
    """Dead draught horse on its side in harness, head toward +Z; legs toward +X (pose 0) / -X (pose 1)."""
    m = Mesh()
    side = 1 if pose == 0 else -1
    hide = HIDE if pose == 0 else (52, 38, 30)
    m.ellipsoid((0, .40, .05), (.72, .62, 1.75), hide)                  # barrel
    m.ellipsoid((side * .05, .32, .95), (.42, .36, .72), hide)          # neck
    m.ellipsoid((side * .12, .24, 1.45), (.26, .24, .62), hide)         # head
    m.ellipsoid((side * .04, .46, .02), (.56, .30, 1.2), HIDE_PALE)     # belly
    legs = [((side * .28, .36, .62), (side * .85, .25, .70), (side * 1.35, .12, .62)),
            ((side * .26, .44, .70), (side * .78, .40, .95), (side * 1.22, .20, 1.10)),
            ((side * .30, .34, -.60), (side * .90, .22, -.78), (side * 1.42, .10, -.95)),
            ((side * .26, .46, -.52), (side * .82, .42, -.30), (side * 1.30, .18, -.05))]
    for a, b, c in legs:
        m.limb(a, b, .13, .09, hide)
        m.limb(b, c, .08, .06, hide)
        m.ellipsoid(c, (.16, .12, .16), (38, 32, 28))
    m.limb((-side * .1, .62, -.72), (-side * .45, .40, -1.25), .05, .02, (40, 30, 24))   # tail
    m.limb((0, .52, .72), (0, .18, .72), .08, .08, STRAP)                # collar
    m.box((0, .62, .1), (.74, .04, .12), STRAP)                          # girth straps
    m.box((0, .62, -.45), (.74, .04, .10), STRAP)
    m.ellipsoid((-side * .10, .66, -.05), (.5, .05, 1.0), SNOW)          # windward mantle
    return m


def sledge(overturned):
    """Panje sledge: two runners, stanchions, plank bed, shafts forward (+Z)."""
    m = Mesh()
    for x in (-.48, .48):
        m.limb((x, .12, -1.35), (x, .12, 1.3), .05, .05, WOOD)
        m.limb((x, .12, 1.3), (x, .42, 1.62), .05, .035, WOOD)          # curled nose
        for z in (-1.1, -.3, .5, 1.1):
            m.limb((x, .12, z), (x, .48, z), .04, .04, WOOD)
    for x in np.linspace(-.5, .5, 6):
        m.box((float(x), .5, -.05), (.16, .04, 2.5), WOOD)
    for x in (-.55, .55):
        m.box((x, .62, -.05), (.05, .2, 2.4), WOOD)
    m.limb((-.42, .5, 1.25), (-.48, .18, 3.0), .035, .03, WOOD)         # shafts to the ground
    m.limb((.42, .5, 1.25), (.52, .22, 3.0), .035, .03, WOOD)
    if overturned:
        rotated(m, 'z', 158.0)
    else:
        m.box((0, .56, -.2), (1.0, .08, 1.9), SNOW)                      # load long gone; snow on the bed
    return m


def crater_scar():
    """Shell crater rim: a ring of thrown frozen earth around a dark floor; unit radius, scaled per site."""
    m = Mesh()
    n = 12
    for i in range(n):
        a = i * math.tau / n
        r = .92 + .08 * math.sin(i * 2.3)
        m.ellipsoid((math.cos(a) * r, .07, math.sin(a) * r), (.46, .20, .46), EARTH if i % 3 else FROZEN_EARTH, i)
    for i in range(n):
        a = (i + .5) * math.tau / n
        m.ellipsoid((math.cos(a) * 1.18, .03, math.sin(a) * 1.18), (.4, .07, .4), SCORCH, i + 5)
    rim = [(math.cos(i * math.tau / 24) * .78, .04, math.sin(i * math.tau / 24) * .78) for i in range(24)]
    for i in range(24):
        m.tri((0, .0, 0), rim[(i + 1) % 24], rim[i], SCORCH)             # the floor rests on the terrain
    return m


def gun_pit_parapet():
    """Heaped snow-and-earth parapet in front of a gun, open to the rear (-Z); about 0.7 m high."""
    m = Mesh()
    for i in range(9):
        a = math.radians(-100 + i * 25)
        x, z = math.sin(a) * 3.0, math.cos(a) * 3.0
        m.ellipsoid((x, .30, z), (1.4, .72, 1.1), FROZEN_EARTH, i)
        m.ellipsoid((x, .50, z), (1.2, .34, .9), SNOW, i + 3)
    return m


def dropped_rifle():
    m = Mesh()
    m.limb((-.55, .03, 0), (.12, .03, 0), .025, .022, (72, 50, 32))    # stock and fore-end
    m.limb((.1, .035, 0), (.58, .035, 0), .013, .011, GUNMETAL)        # barrel
    m.box((-.02, .06, 0), (.12, .05, .03), GUNMETAL)                   # bolt housing
    return m


def greatcoat_heap(pose):
    m = Mesh()
    col = COAT if pose == 0 else COAT_B
    m.ellipsoid((0, .09, 0), (1.1, .2, .7), col, pose)
    m.ellipsoid((.25, .14, .15), (.5, .16, .4), col, pose + 2)
    m.ellipsoid((-.3, .12, -.1), (.42, .14, .36), (82, 76, 66), pose + 4)
    m.ellipsoid((-.05, .2, .05), (.6, .05, .4), SNOW, pose + 6)
    return m


def loose_wheel():
    m = Mesh()
    wheel(m, (0, .09, 0), .62, .14, (48, 44, 40), 10, .05)
    return rotated(m, 'z', 90.0)


def meshes():
    return {
        'lorry_wreck_burnt': lorry(True), 'lorry_wreck_detail': lorry(False),
        'draught_horse_a_wreck_detail': draught_horse(0), 'draught_horse_b_wreck_detail': draught_horse(1),
        'panje_sledge_wreck_detail': sledge(False), 'panje_sledge_overturned_wreck_detail': sledge(True),
        'crater_scar': crater_scar(), 'gun_pit_parapet': gun_pit_parapet(),
        'dropped_rifle': dropped_rifle(), 'greatcoat_heap_a': greatcoat_heap(0), 'greatcoat_heap_b': greatcoat_heap(1),
        'loose_wheel': loose_wheel(),
    }


def build(root, force=False):
    root = Path(root)
    dest = root / PREFIX
    dest.mkdir(parents=True, exist_ok=True)
    receipt = {}
    for name, mesh in meshes().items():
        out = dest / (name + '.glb')
        # Mesh.write reports paths relative to this tool's checkout, so stage beside it.
        staging = Path(__file__).resolve().parents[2] / 'artifacts' / 'authoring' / 'wreckage-props-staging'
        staging.mkdir(parents=True, exist_ok=True)
        staged = staging / (name + '.glb')
        rec = mesh.write(staged)
        data = staged.read_bytes()
        staged.unlink()
        if out.exists() and out.read_bytes() != data and not force:
            raise RuntimeError(f'Preserving existing different output {out}; pass --force to rewrite')
        out.write_bytes(data)
        collision = ('source-surface wreck proxies' if name.endswith(('_wreck_burnt', '_wreck_detail'))
                     else 'none (low dressing)')
        rec.update(path=PREFIX + name + '.glb', sha256=hashlib.sha256(data).hexdigest(), bytes=len(data),
                   collision=collision)
        receipt[name] = rec
    doc = dict(schema='panzer.canyon-v2-wreckage-props', tool='tools/worldgen/canyon_v2_wreckage_props.py',
               version=1,
               source='authored procedural geometry; inferred composite props, not replicas of a named make, unit or person',
               references=['docs/ENVSTORY.md "The Frozen Horses"',
                           'docs/research/PHOTO_REFERENCE_ANALYSIS_20260928.md [05-01] [05-02] [05-05]',
                           'docs/research/SOVIET_AT_WEAPONS_BRIEF_20260928.md (45 mm crews, horse draught)'],
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
