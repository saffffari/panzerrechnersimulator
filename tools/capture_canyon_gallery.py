#!/usr/bin/env python3
"""Real game-renderer stills: staged player viewpoints and explicit aerial recon cameras."""
import json,math,os,subprocess,hashlib
from pathlib import Path
import numpy as np
ROOT=Path(__file__).resolve().parents[1]
WORLD=ROOT/'assets/worldgen/workshops/gaea-canyon-run'
OUT=ROOT/'captures'
e=json.loads((WORLD/'manifest_edits.json').read_text());route=np.array(e['canyon_course']['route']);h=np.load(WORLD/'heights.npy')
def ground(p):
    x,z=np.clip((np.asarray(p)+2500)/4.8828125,0,1024);ix,iz=min(int(x),1023),min(int(z),1023);tx,tz=x-ix,z-iz
    a,b,c,d=h[iz,ix],h[iz,ix+1],h[iz+1,ix],h[iz+1,ix+1]
    return float(a+(b-a)*tx+(c-a)*tz if tx+tz<=1 else d+(c-d)*(1-tx)+(b-d)*(1-tz))
def xyz(p,above):return [float(p[0]),ground(p)+above,float(p[1])]
def angles(pos,target):
    dx,dy,dz=np.array(target)-pos
    return [math.degrees(math.atan2(dx,-dz)),math.degrees(math.atan2(dy,math.hypot(dx,dz)))]
base=dict(os.environ,KESSEL_RENDERER='shader',KESSEL_INTERNAL_RES='640x360',KESSEL_GRAPHICS_PRESET='high',
    KESSEL_RECON_MANIFEST=str(WORLD/'manifest.v1.json'),KESSEL_SHOT_CANYON='1',KESSEL_CANYON_SEED='29',XDG_RUNTIME_DIR='/run/user/1000')
shots=[]
for name,f in [('approach',.14),('settlement',.30),('factory',.60),('breakout',.89)]:
    j=round(f*(len(route)-1));p=route[j];q=route[min(j+15,len(route)-1)];d=q-p
    env=base|dict(KESSEL_SHOT_TIGER=f'{p[0]},{p[1]},{math.degrees(math.atan2(d[0],d[1]))}',
        KESSEL_SHOT_STATION='commander',KESSEL_SHOT_HEADOUT='1',KESSEL_SHOT_LAY=','.join(map(str,xyz(q,2.7))))
    shots.append(dict(name='player_'+name,title=name.title()+' — commander’s cupola',type='staged actual first-person view',env=env,args=[],position=xyz(p,2.7),fog=.0022))
for name,pos,target,fog in [
    ('aerial_course_plan',[425,1900,145],[425,0,120],.00018),
    ('aerial_settlement_detail',[-440,240,-190],[-190,10,-520],.00065),
    ('aerial_factory_detail',[590,250,470],[325,-5,190],.00065)]:
    args=list(pos)+angles(pos,target)+[fog]
    shots.append(dict(name=name,title=name.replace('_',' ').title(),type='aerial recon camera',env=base,args=args,position=pos,target=target,fog=fog))
j=155;p=route[j];q=route[j+12];d=q-p
shots.append(dict(name='held_paper',title='Survey held in the cupola',type='actual physical paper, staged first-person view',
    env=base|dict(KESSEL_SHOT_TIGER=f'{p[0]},{p[1]},{math.degrees(math.atan2(d[0],d[1]))}',KESSEL_SHOT_STATION='commander',KESSEL_SHOT_HEADOUT='1',KESSEL_SHOT_MAP='1',KESSEL_SHOT_LAY=','.join(map(str,xyz(q,2.7)))),args=[],position=xyz(p,2.7),fog=.0022))
receipt=[]
only=os.environ.get('CANYON_GALLERY_ONLY')
if only and (OUT/'canyon_gallery_20260930.json').exists():
    receipt=json.loads((OUT/'canyon_gallery_20260930.json').read_text())['shots']
    receipt=[s for s in receipt if not s['name'].startswith(only)]
for shot in shots:
    if only and not shot['name'].startswith(only):continue
    output=OUT/f"canyon_gallery_{shot['name']}_20260930.png"
    cmd=[str(ROOT/'target/release/panzerrechnersystem'),'screenshot',str(output)]+list(map(str,shot['args']))
    print('Rendering',shot['name'],flush=True)
    with output.with_suffix('.log').open('w') as log:
        result=subprocess.run(cmd,cwd=ROOT,env=shot['env'],stdout=log,stderr=subprocess.STDOUT)
    item={k:v for k,v in shot.items() if k!='env'};item.update(output=str(output),exit_code=result.returncode,command=cmd)
    receipt.append(item)
    (OUT/'canyon_gallery_20260930.json').write_text(json.dumps(dict(seed=29,binary_sha256=hashlib.sha256((ROOT/'target/release/panzerrechnersystem').read_bytes()).hexdigest(),
        note='Real game renders at shipping internal resolution. Player viewpoints staged. Aerials use reduced physical fog density for terrain inspection; player views retain authored winter weather. No AI renders.',shots=receipt),indent=2)+'\n')
    if result.returncode:raise SystemExit(f"Capture failed: {output.with_suffix('.log')}")
print('Gallery complete',flush=True)
