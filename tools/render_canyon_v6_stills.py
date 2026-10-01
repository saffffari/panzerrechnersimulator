#!/usr/bin/env python3
"""Native game stills for the extended Canyon review; no external images or post grading."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parents[1]
CAP=ROOT/'captures'
WORLD=ROOT/'assets/worldgen/workshops/gaea-canyon-run-v2'
EXE=ROOT/'target/release/panzerrechnersystem'


def main():
    prior=json.loads((CAP/'canyon_v2_gallery_20260930.json').read_text())
    names=['aerial_course_plan','aerial_settlement_detail','aerial_factory_detail','aftermath_position']
    records=[]
    env=dict(os.environ,KESSEL_RENDERER='shader',KESSEL_INTERNAL_RES='640x360',
        KESSEL_GRAPHICS_PRESET='high',KESSEL_RECON_MANIFEST=str(WORLD/'manifest.v1.json'),
        KESSEL_SHOT_CANYON='1',XDG_RUNTIME_DIR='/run/user/1000')
    for name in names:
        shot=next(s for s in prior['shots'] if s['name']==name)
        args=list(shot['args'])
        if not name.startswith('aerial'):
            args[-1]=.0046
        output=CAP/f'canyon_v6_{name}_20260930.png'
        if output.exists():
            raise RuntimeError(f'Preserving existing {output}')
        cmd=[str(EXE),'screenshot',str(output),*map(str,args)]
        with output.with_suffix('.log').open('w') as log:
            subprocess.run(cmd,env=env,cwd=ROOT,stdout=log,stderr=subprocess.STDOUT,check=True)
        records.append(dict(name=name,output=output.name,command=cmd,
            camera_kind='staged aerial inspection' if name.startswith('aerial') else 'staged standing-height inspection',
            fog_density=args[-1],reduced_fog_for_aerial=name.startswith('aerial'),
            sha256=hashlib.sha256(output.read_bytes()).hexdigest()))
        (CAP/'canyon_v6_stills_20260930.json').write_text(json.dumps(dict(shots=records,
            native_resolution=[640,360],binary_sha256=hashlib.sha256(EXE.read_bytes()).hexdigest(),
            manifest_sha256=hashlib.sha256((WORLD/'manifest.v1.json').read_bytes()).hexdigest(),
            edits_sha256=hashlib.sha256((WORLD/'manifest_edits.json').read_bytes()).hexdigest()),indent=2)+'\n')
        print(output,flush=True)


if __name__=='__main__':
    main()
