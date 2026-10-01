#!/usr/bin/env python3
"""Make an isolated material / ground-cover study from an existing Canyon v2.

Landmarks, enemies and terrain geometry are copied unchanged. Native Gaea maps and
the previous playable version are preserved for same-camera comparisons.
"""
import argparse
import hashlib
import json
import shutil
from pathlib import Path

import numpy as np
from PIL import Image

import canyon_ground_splatter as blood
import canyon_v2_assets as props
import canyon_winter_surface as surface
import gaea_canyon_run as gaea

ROOT = Path(__file__).resolve().parents[2]


def ground_at(h, x, z):
    x, z = np.clip((np.array([x, z])+2500)/4.8828125, 0, 1024)
    ix, iz = min(int(x), 1023), min(int(z), 1023)
    tx, tz = x-ix, z-iz
    a, b, c, d = h[iz, ix], h[iz, ix+1], h[iz+1, ix], h[iz+1, ix+1]
    return float(a+(b-a)*tx+(c-a)*tz if tx+tz <= 1 else d+(c-d)*(1-tx)+(b-d)*(1-tz))


def seed_at(x, z):
    return blood.seed_at(x, z)


def build(source, dest):
    source, dest = Path(source).absolute(), Path(dest).absolute()
    assert source.resolve() != dest.resolve()
    dest.mkdir(parents=True, exist_ok=True)
    manifest = json.loads((source/'manifest.v1.json').read_text())
    edits = json.loads((source/'manifest_edits.json').read_text())
    h = np.load(source/'heights.npy')
    route = np.array(edits['canyon_course']['route'])
    rock = np.asarray(Image.open(gaea.SOURCE/'SatMap_Out.png').convert('RGB'))
    snow = np.asarray(Image.open(gaea.SOURCE/'snow.png'), dtype=np.float32)
    if snow.max() > 1:
        snow /= 65535 if snow.max() > 255 else 255
    if snow.ndim == 3:
        snow = snow[:, :, 0]
    rock, snow, vegetation, receipt = surface.author(rock, snow, route,
        edits['building_instances'], heights=h)
    Image.fromarray(np.dstack([rock, np.uint8(np.rint(32+191*snow))])).save(dest/'rock_snow_material.png')
    Image.fromarray(np.dstack([vegetation, np.zeros_like(vegetation), np.zeros_like(vegetation)])).save(dest/'ground_class.png')
    manifest['terrain'].update(texture=str((dest/'rock_snow_material.png').relative_to(ROOT)),
        ground_class=str((dest/'ground_class.png').relative_to(ROOT)), authored_wear=receipt)
    meshes = [props.Mesh() for _ in range(12)]
    deposits = []
    placements = []
    receipt.update(surface.restrict_stalks(manifest, vegetation))
    for placement in manifest['placements']:
        name = manifest['assets'][placement[0]]
        if '/blood_snow_' in name:
            continue
        x, z = placement[1], placement[3]
        placements.append(placement)
        if '/casualty_' in name:
            nearest = int(np.argmin(np.sum((route-[x, z])**2, axis=1)))
            sector = min(int(nearest/len(route)*12), 11)
            rec = blood.splatter(meshes[sector], lambda a,b:ground_at(h,a,b), x+.25, z, seed_at(x,z))
            deposits.append(dict(x=x, z=z, **rec))
    for i, mesh in enumerate(meshes):
        if not mesh.points:
            continue
        asset = mesh.write(dest/f'blood_drops_{i:02}.glb')
        manifest['assets'].append(asset['path'])
        placements.append([len(manifest['assets'])-1, 0, 0, 0, 0, 1])
    manifest['placements'] = placements
    manifest['authoring']['variant'] = dest.name
    edits['base_placements'] = len(placements)
    receipt.update(blood_sites=len(deposits), blood_cubes=sum(r['drops'] for r in deposits),
        blood_summed_footprint_m2=sum(r['summed_footprint_m2'] for r in deposits),
        blood_width_m=[.006,.032],
        source_manifest_sha256=hashlib.sha256((source/'manifest.v1.json').read_bytes()).hexdigest())
    (dest/'manifest.v1.json').write_text(json.dumps(manifest, separators=(',',':'))+'\n')
    (dest/'manifest_edits.json').write_text(json.dumps(edits, indent=2)+'\n')
    (dest/'surface-study.receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
    (dest/'blood-deposits.json').write_text(json.dumps(deposits, indent=2)+'\n')
    shutil.copyfile(source/'heights.npy', dest/'heights.npy')
    print(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--source', type=Path, default=ROOT/'assets/worldgen/workshops/gaea-canyon-run-v2')
    parser.add_argument('--dest', type=Path, required=True)
    args = parser.parse_args()
    build(args.source, args.dest)
