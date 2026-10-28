#!/usr/bin/env python3
"""Regression checks for the Canyon v2 wreckage overlay on a small synthetic workshop.

    python tools/worldgen/test_canyon_v2_wreckage.py          (pytest also collects it)
"""
import json
import math
import sys
from pathlib import Path

import tempfile

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import canyon_v2_wreckage as W  # noqa: E402


def synthetic_workshop(tmp_path, res=257, spacing=4.8828125):
    """A flat valley floor with a straight route, two houses and a few base props."""
    root = tmp_path / 'root'
    dest = root / 'workshop'
    dest.mkdir(parents=True)
    half = (res - 1) * spacing / 2
    x = np.linspace(-half, half, res)
    # a gentle valley: the floor along x = 0 .. 2 m, walls rising beyond 90 m either side
    H = np.clip((np.abs(x[None, :]) - 90.0) * 0.6, 0.0, 200.0) + 0.5 * np.sin(x[:, None] / 40.0)
    np.save(dest / 'heights.npy', H.astype(np.float32))
    route = [[0.0, float(z)] for z in np.arange(half - 60, -half + 60, -8.0)]
    for path in W.ASSETS.values():
        (root / path[0]).parent.mkdir(parents=True, exist_ok=True)
        (root / path[0]).write_bytes(b'glTF')
    assets = ['assets/pizza/_extracted/PSX Nature v1.7.3/x/tree_8.glb',
              W.MP2 + 'Props/wooden_crate_2_a.glb', W.CV + 'rock_0.glb']
    placements = []
    for i, z in enumerate(np.arange(-400, 400, 23.0)):
        placements.append([0, 30.0 + (i % 5) * 9.0, 0.0, float(z), 0.0, 1.0])          # trees
        placements.append([1, -16.0 - (i % 3) * 2.0, 0.0, float(z) + 5.0, 0.0, 1.0])  # crates
    placements.append([2, 40.0, 0.0, 100.0, 0.0, 1.0])
    manifest = dict(assets=assets, placements=placements, markers=[], terrain=dict(heights='heights.npy', spacing=spacing, res=res))
    (dest / 'manifest.v1.json').write_text(json.dumps(manifest))
    edits = dict(base_placements=len(placements), base_markers=0, ops=[],
                 canyon_course=dict(route=route, start=dict(x=0.0, z=route[0][1]), goal=dict(x=0.0, z=route[-1][1]),
                                    garrison=[dict(x=20.0, z=-150.0)]),
                 building_instances=[dict(x=22.0, z=50.0, width_m=12.0, depth_m=8.0, yaw_deg=90.0),
                                     dict(x=-24.0, z=-200.0, width_m=10.0, depth_m=10.0, yaw_deg=0.0)],
                 street_encounter=dict(beats=[dict(points=[[5.0, 0.0], [5.0, -36.0]])]))
    (dest / 'manifest_edits.json').write_text(json.dumps(edits))
    return root, dest


def test_layer_is_deterministic_and_removable(tmp_path):
    root, dest = synthetic_workshop(tmp_path)
    first = W.apply(root, dest, W.DEFAULT_SEED, 1.0)
    edits_a = json.loads((dest / 'manifest_edits.json').read_text())
    assert first['result']['ops'] > 100
    assert edits_a['base_placements'] == 71
    second = W.apply(root, dest, W.DEFAULT_SEED, 1.0)
    edits_b = json.loads((dest / 'manifest_edits.json').read_text())
    assert second['result']['removed_previous_ops'] == first['result']['ops']
    assert edits_a['ops'] == edits_b['ops'], 'same seed must reproduce the layer byte for byte'
    assert W.apply(root, dest, W.DEFAULT_SEED + 1, 1.0)['edits_sha256'] != second['edits_sha256']
    W.apply(root, dest, W.DEFAULT_SEED, 1.0, remove=True)
    stripped = json.loads((dest / 'manifest_edits.json').read_text())
    assert stripped['ops'] == [] and W.TAG not in stripped
    assert (root / 'artifacts' / 'authoring' / 'canyon-v2-wreckage' / 'manifest_edits.before.json').exists()


def test_every_placement_respects_the_route_floor_and_lane(tmp_path):
    root, dest = synthetic_workshop(tmp_path)
    W.apply(root, dest, W.DEFAULT_SEED, 1.0)
    ws = W.Workshop(root, dest)
    manifest = json.loads((dest / 'manifest.v1.json').read_text())
    edits = json.loads((dest / 'manifest_edits.json').read_text())
    adds = [op for op in edits['ops'] if op['op'] == 'add']
    deletes = [op for op in edits['ops'] if op['op'] == 'delete']
    assert adds and deletes
    for op in adds:
        assert op['tag'] == W.TAG and op['scene'].startswith(W.TAG)
        assert all(math.isfinite(op[k]) for k in ('x', 'y', 'z', 'deg'))
        scale = op['s'] if isinstance(op['s'], list) else [op['s']]
        assert all(v > 0 for v in scale)
        assert (root / op['asset']).is_file(), op['asset']
        dist, _ = ws.route.nearest(op['x'], op['z'])
        assert dist >= W.LANE_HALF_M - 1.0 - 1e-6, (op['asset'], dist)
        assert abs(op['y'] - ws.ground(op['x'], op['z'])) <= 0.15, 'placements rest on the heightfield'
        if 'tilt_deg' in op:
            assert max(abs(v) for v in op['tilt_deg']) <= 9.0
        assert math.hypot(op['x'] - ws.spawn[0], op['z'] - ws.spawn[1]) >= W.SPAWN_CLEAR_M
    for op in deletes:
        stem = Path(manifest['assets'][manifest['placements'][op['i']][0]]).stem
        assert stem in W.DISPLACEABLE, 'only loose base debris may be displaced'
    assert len({op['i'] for op in deletes}) == len(deletes)
    block = edits[W.TAG]
    assert block['provenance']['class'] == 'attested_condition'
    assert block['counts']['displaced_base_props'] == len(deletes)
    assert sum(v for k, v in block['counts'].items() if k != 'displaced_base_props') == len(adds)


def test_vehicles_keep_out_of_buildings_and_defence_posts(tmp_path):
    root, dest = synthetic_workshop(tmp_path)
    W.apply(root, dest, W.DEFAULT_SEED, 1.0)
    ws = W.Workshop(root, dest)
    edits = json.loads((dest / 'manifest_edits.json').read_text())
    for op in edits['ops']:
        if op['op'] != 'add':
            continue
        for x0, x1, z0, z1 in ws.rects:
            assert not (x0 < op['x'] < x1 and z0 < op['z'] < z1), (op['asset'], 'inside a house')
        for kx, kz in ws.keep:
            assert math.hypot(op['x'] - kx, op['z'] - kz) >= 7.0, 'garrison and encounter posts stay clear'


def test_cameras_follow_the_route(tmp_path):
    root, dest = synthetic_workshop(tmp_path)
    cams = W.recon_cameras(W.Workshop(root, dest))
    assert [c['name'] for c in cams] == [n for n, _ in W.CAMERA_FRACTIONS]
    for cam in cams:
        assert abs(cam['x']) < 1e-6 and abs(cam['yaw']) < 1e-6, 'the straight route runs toward -z, which is yaw 0'


if __name__ == '__main__':
    tests = [v for k, v in sorted(globals().items()) if k.startswith('test_')]
    for test in tests:
        with tempfile.TemporaryDirectory() as tmp:
            test(Path(tmp))
        print('ok', test.__name__)
    print(f'{len(tests)} passed')
