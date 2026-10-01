#!/usr/bin/env python3
"""Upgrade only Canyon v2's saved look and blood film; preserve the previous workshop/assets."""
import hashlib
import json
import shutil
from pathlib import Path

import numpy as np
import canyon_ground_splatter as blood
import canyon_v2_assets as props
from canyon_surface_study import ground_at
from canyon_look_review import apply_look

ROOT = Path(__file__).resolve().parents[2]


def upgrade():
    dest = ROOT/'assets/worldgen/workshops/gaea-canyon-run-v2'
    archive = dest.with_name('gaea-canyon-v5-preserved')
    if archive.exists():
        raise RuntimeError('v5 archive exists; refusing to overwrite this upgrade')
    shutil.copytree(dest, archive)
    manifest = json.loads((dest/'manifest.v1.json').read_text())
    edits = json.loads((dest/'manifest_edits.json').read_text())
    heights = np.load(dest/'heights.npy')
    route = np.array(edits['canyon_course']['route'])
    groups = [[] for _ in range(12)]
    before = list(manifest['placements'])
    for p in before:
        if '/casualty_' in manifest['assets'][p[0]]:
            nearest = int(np.argmin(np.sum((route-[p[1],p[3]])**2,axis=1)))
            groups[min(int(nearest/len(route)*12),11)].append(p)
    manifest['placements'] = [p for p in before if not any(n in manifest['assets'][p[0]]
        for n in ['/blood_snow_','/blood_drops_'])]
    deposits, assets = [], []
    for i, group in enumerate(groups):
        mesh = props.Mesh()
        for p in group:
            x,z=p[1],p[3]
            rec=blood.splatter(mesh,lambda a,b:ground_at(heights,a,b),x+.25,z,blood.seed_at(x,z))
            deposits.append(dict(x=x,z=z,**rec))
        if mesh.points:
            asset=mesh.write(dest/f'blood_drops_v6_{i:02}.glb')
            assets.append(asset)
            manifest['assets'].append(asset['path'])
            manifest['placements'].append([len(manifest['assets'])-1,0,0,0,0,1])
            print(f'sector {i}: {len(group)} sites, {asset["triangles"]} triangles',flush=True)
    apply_look(edits)
    manifest['authoring']['tool_version']=6
    edits['base_placements']=len(manifest['placements'])
    (dest/'manifest.v1.json').write_text(json.dumps(manifest,separators=(',',':'))+'\n')
    (dest/'manifest_edits.json').write_text(json.dumps(edits,indent=2)+'\n')
    receipt=dict(schema='panzer.canyon-look-upgrade.v1',previous_workshop=str(archive),
        previous_manifest_sha256=hashlib.sha256((archive/'manifest.v1.json').read_bytes()).hexdigest(),
        unchanged_heightfield_sha256=hashlib.sha256((dest/'heights.npy').read_bytes()).hexdigest(),
        sites=len(deposits),cells=sum(d['drops'] for d in deposits),assets=assets,
        film='Connected 14 mm cells and 6-32 mm isolated cuboids; authored aftermath, not fluid simulation.')
    (dest/'look-upgrade.receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    # Keep the canonical generator's overwrite guard accurate without losing its original receipt.
    receipt_path=dest/'canyon-course.receipt.json'
    course=json.loads(receipt_path.read_text())
    for name in course['outputs']:
        course['outputs'][name]=hashlib.sha256((dest/name).read_bytes()).hexdigest()
    course['version']=6
    course['look_upgrade']='look-upgrade.receipt.json'
    receipt_path.write_text(json.dumps(course,indent=2)+'\n')
    print(json.dumps(receipt,indent=2))


if __name__ == '__main__':
    upgrade()
