#!/usr/bin/env python3
"""One bounded native GPU workload at a time; never run beside a live playtest."""
from pathlib import Path
import os,sys,subprocess,json,time,hashlib
root=Path(__file__).resolve().parent.parent
variant=sys.argv[1];kind=sys.argv[2]
assert variant in ('baseline','candidate') and kind in ('fire','dense','mechanisms','cabin','idle')
base=Path('/home/alex/panzer/canyon-v2-20260930')
binary=(base if variant=='baseline' else root)/'target/release/panzerrechnersystem'
tag=f'hotfix_{variant}_{kind}_20261002'
if len(sys.argv)>3:
 suffix=sys.argv[3]
 assert suffix.isalnum(), 'Use a simple attempt suffix'
 tag+='_'+suffix
prefix=root/'captures'/tag
log=root/'artifacts/runtime'/f'{tag}.log'
receipt=log.with_suffix('.invocation.json')
assert not receipt.exists(),f'Preserve prior result: {receipt}'
env={k:v for k,v in os.environ.items() if not k.startswith('KESSEL_')}
env.update(KESSEL_ASSET_ROOT=str(root),KESSEL_RENDERER='shader',KESSEL_NO_RT='1',
 KESSEL_INTERNAL_RES='800x450',KESSEL_DITHER_PATTERN='screen_bayer8',KESSEL_GEOMETRY_AA='msaa4',
 KESSEL_SOURCE_VISIBILITY='maps',KESSEL_CANYON_V2='1',KESSEL_ENABLE_PIZZA_AUDIO='1',
 KESSEL_TRANSPORT_QUALITY='full',KESSEL_CANYON_SEED='6855553909752338594',
 KESSEL_GPU_PROFILE='0' if 'nogpu' in sys.argv[3:] else '1',KESSEL_WINTER_BENCH_TAG=tag)
env['KESSEL_PLAYTEST_MANIFEST']=str(root/'assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json')
for update in ('manual','catchup'):
 if update in sys.argv[3:]:env['KESSEL_WINTER_BENCH_UPDATE']=update
if kind in ('fire','dense','idle'):
 args=['winter-bench',kind,'60',str(root/'artifacts/runtime'/f'{tag}.json')]
elif kind=='mechanisms': args=['winter-drive-review',str(prefix),'manual']
else: args=['combat-scene-suite',str(prefix),'winter_canyon_inside','8','preview']
with binary.open('rb') as f: sha=hashlib.file_digest(f,'sha256').hexdigest()
data=dict(binary=str(binary),sha256=sha,args=args,environment={k:v for k,v in env.items() if k.startswith('KESSEL_')},
 scope='Sequential offscreen workload; full update+fenced render for benches, not native vsync FPS',start=time.time())
receipt.write_text(json.dumps(data,indent=2)+'\n')
print('START',tag,sha,flush=True)
try:
 with log.open('w') as out:
  p=subprocess.run([str(binary),*args],cwd=root,env=env,stdout=out,stderr=subprocess.STDOUT,timeout=360)
 data['exit_code']=p.returncode
except subprocess.TimeoutExpired: data['exit_code']=124
finally:
 data['wall_seconds']=time.time()-data['start'];receipt.write_text(json.dumps(data,indent=2)+'\n')
print('DONE',tag,data['exit_code'],round(data['wall_seconds'],2),flush=True)
sys.exit(data['exit_code'])
