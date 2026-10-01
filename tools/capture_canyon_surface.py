#!/usr/bin/env python3
"""Reproducible native Canyon material review cameras; no image postprocessing."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'tools/worldgen'))
from canyon_surface_study import ground_at
from canyon_ground_splatter import seed_at

parser = argparse.ArgumentParser()
parser.add_argument('--world', type=Path, required=True)
parser.add_argument('--tag', required=True)
parser.add_argument('--resolution', default='1280x720', choices=['640x360','960x540','1280x720'])
parser.add_argument('--shot', choices=['player','ground','blood','splatter','grass','aerial'], required=True)
args = parser.parse_args()
world = args.world.resolve()
reference = ROOT/'assets/worldgen/workshops/gaea-canyon-run-v2'
edits = json.loads((reference/'manifest_edits.json').read_text())
manifest = json.loads((reference/'manifest.v1.json').read_text())
h = np.load(world/'heights.npy')
route = np.array(edits['canyon_course']['route'])
j = round(.30*(len(route)-1))
p, q = route[j], route[j+15]
def xyz(p, above): return [float(p[0]),ground_at(h,*p)+above,float(p[1])]
def angles(pos, target):
    dx,dy,dz = np.array(target)-pos
    return [math.degrees(math.atan2(dx,-dz)),math.degrees(math.atan2(dy,math.hypot(dx,dz)))]
env = dict(os.environ, KESSEL_RENDERER='shader', KESSEL_INTERNAL_RES=args.resolution,
    KESSEL_GRAPHICS_PRESET='high', KESSEL_RECON_MANIFEST=str(world/'manifest.v1.json'),
    KESSEL_SHOT_CANYON='1', KESSEL_CANYON_SEED='29', XDG_RUNTIME_DIR='/run/user/1000')
extra = []
if args.shot == 'player':
    d = q-p
    env.update(KESSEL_SHOT_TIGER=f'{p[0]},{p[1]},{math.degrees(math.atan2(d[0],d[1]))}',
        KESSEL_SHOT_STATION='commander', KESSEL_SHOT_HEADOUT='1',
        KESSEL_SHOT_LAY=','.join(map(str,xyz(q,2.7))))
    pos, target = xyz(p,2.7), xyz(q,2.7)
elif args.shot == 'ground':
    forward = (q-p)/np.linalg.norm(q-p)
    side = np.array([-forward[1],forward[0]])
    pos = xyz(p+side*6+forward*8,1.7)
    target = xyz(p+side*12+forward*18,.12)
    extra = pos+angles(pos,target)+[.0022]
elif args.shot == 'grass':
    study = json.loads((world/'manifest.v1.json').read_text())
    cover = np.asarray(Image.open(ROOT/study['terrain']['ground_class']))[:,:,0]
    cells = np.argwhere(cover > 240)[::8]
    cells = cells[(cells[:,0]>8)&(cells[:,0]<len(cover)-8)&(cells[:,1]>8)&(cells[:,1]<len(cover)-8)]
    supported = np.ones(len(cells),dtype=bool)
    for dx,dz in [(-4,-4),(4,-4),(4,4),(-4,4)]:
        supported &= cover[cells[:,0]+dz,cells[:,1]+dx] > 192
    cells = cells[supported]
    points = cells[:,::-1]/(len(cover)-1)*5000-2500
    center = points[np.argmin(np.linalg.norm(points-p,axis=1))]
    pos, target = xyz(center+[0,6],1.55), xyz(center,.16)
    extra = pos+angles(pos,target)+[.0022]
elif args.shot in ('blood','splatter'):
    bodies = [v for v in manifest['placements'] if '/casualty_' in manifest['assets'][v[0]]]
    b = min(bodies,key=lambda v:np.linalg.norm(np.array([v[1],v[3]])-p))
    target = xyz([b[1]+.22,b[3]],.09)
    pos = xyz([b[1]+1.65,b[3]+1.85],1.7)
    if args.shot == 'splatter':
        angle = np.random.default_rng(seed_at(b[1],b[3])).uniform(0,math.tau)
        direction = np.array([math.cos(angle),math.sin(angle)])
        center = np.array([b[1]+.25,b[3]])
        target, pos = xyz(center+direction*.52,.08), xyz(center+direction*2.1,1.6)
    extra = pos+angles(pos,target)+[.0022]
else:
    pos, target = [-440,240,-190],[-190,10,-520]
    extra = pos+angles(pos,target)+[.00065]
output = ROOT/'captures'/f'canyon_{args.tag}_{args.shot}_20260930.png'
assert not output.exists(), f'Preserve prior capture: {output}'
cmd = [str(ROOT/'target/release/panzerrechnersystem'),'screenshot',str(output)]+list(map(str,extra))
with output.with_suffix('.log').open('w') as log:
    result = subprocess.run(cmd,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
receipt = dict(world=str(world),camera=args.shot,position=pos,target=target,command=cmd,
    internal_resolution=list(map(int,args.resolution.split('x'))),exit_code=result.returncode,
    binary_sha256=hashlib.sha256(Path(cmd[0]).read_bytes()).hexdigest(),
    manifest_sha256=hashlib.sha256((world/'manifest.v1.json').read_bytes()).hexdigest(),
    edits_sha256=hashlib.sha256((world/'manifest_edits.json').read_bytes()).hexdigest(),
    authored_lighting=json.loads((world/'manifest_edits.json').read_text())['environment_lighting'],
    note='Native engine. Staged camera. Aerial reduces authored fog for inspection.')
output.with_suffix('.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt,indent=2))
raise SystemExit(result.returncode)
