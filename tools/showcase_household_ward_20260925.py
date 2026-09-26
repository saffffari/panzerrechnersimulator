#!/usr/bin/env python3
"""Bounded source-game still review of the two cellar workshops; no native window control."""
import argparse,hashlib,json,os,subprocess,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
CAP=ROOT/'captures/astra'

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('slug',choices=['household','ward'])
    parser.add_argument('--version',default='v3',choices=['v1','v2','v3'])
    args=parser.parse_args()
    scene=next(s for s in json.loads((ROOT/'artifacts/authoring/historical-20260925/workshops.json').read_text())['scenes'] if s['slug']==args.slug)
    binary=ROOT/'target/release/panzerrechnersystem'
    env=os.environ.copy();env.update(KESSEL_RENDERER='shader',KESSEL_INTERNAL_RES='640x360',KESSEL_ASSET_ROOT='/home/alex/panzer/tank_simulator',KESSEL_ACT='1',KESSEL_RECON_MANIFEST=str(ROOT/scene['manifest']),KESSEL_CAPTURE_FRAME_PAUSE_MS='1',KESSEL_CAPTURE_IDLE_FACTOR='0')
    views=scene['views'][:3]
    receipt={'schema':'panzer.historical-still-review.v1','slug':args.slug,'manifest':scene['manifest'],'manifest_sha256':digest(ROOT/scene['manifest']),'binary_sha256':digest(binary),'views':[], 'scope':'Staged exact recon cameras in source game, not evidence of gameplay traversal. Terrain fog override 0.0018; ordinary daytime shader renderer. All architecture and furnishings are inferred; not exact historic plans.'}
    for view in views:
        out=CAP/f"showcase_20260925_{args.slug}_{args.version}_{view['id']}.png"
        if out.exists():raise RuntimeError(f'Refusing overwrite {out}')
        log=out.with_suffix('.log');cam=[*view['eye_m'],view['yaw_deg'],view['pitch_deg'],0.0018]
        cmd=[str(binary),'screenshot',str(out),*map(str,cam)]
        start=time.monotonic()
        with log.open('w') as f:subprocess.run(cmd,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
        receipt['views'].append({**view,'fog_density':.0018,'image':str(out),'image_sha256':digest(out),'command':cmd,'log':str(log),'wall_seconds':time.monotonic()-start,'visual_review':'pending'})
        (CAP/f'showcase_20260925_{args.slug}_{args.version}_stills.receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
        print(out,flush=True)
if __name__=='__main__':main()
