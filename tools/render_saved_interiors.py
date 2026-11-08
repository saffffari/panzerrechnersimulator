#!/usr/bin/env python3
from pathlib import Path
import os,subprocess,signal,time,json,sys,shutil
root=Path('/home/alex/panzer');stable=root/'editor-stable';run=Path(sys.argv[1]).resolve();mode=sys.argv[2] if len(sys.argv)>2 else 'render'
keep_closed='--keep-closed' in sys.argv[3:]
env=os.environ.copy()
for line in subprocess.check_output(['systemctl','--user','show-environment'],text=True).splitlines():
 if line.startswith(('DISPLAY=','WAYLAND_DISPLAY=','XAUTHORITY=','XDG_RUNTIME_DIR=')):
  k,v=line.split('=',1);env[k]=v
env.update(KESSEL_ASSET_ROOT='/windows/shop/panzerrechnersystem',KESSEL_INTERNAL_RES='960x540',KESSEL_REAL_STEPS='7,7,4',KESSEL_TRANSPORT_QUALITY='full',KESSEL_TISSUE_SOLVE='reference')
def relaunch():
 if keep_closed:
  print('Editor remains closed as requested',flush=True);return
 clean=env.copy()
 for k in ['KESSEL_FEA_RANGE_FILM','KESSEL_TISSUE_SOLVE','KESSEL_TRANSPORT_QUALITY','KESSEL_REAL_STEPS']:clean.pop(k,None)
 p=subprocess.Popen([str(stable/'launch.sh')],cwd=stable,env=clean,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
 print('Editor relaunched:',p.pid,flush=True)
if mode in ('preview','render'):
 marker=Path('/home/alex/.cache/panzerrechnersystem/window-live.pid')
 if marker.exists():
  pid=int(marker.read_text());proc=Path(f'/proc/{pid}/cmdline')
  if proc.exists():
   cmd=proc.read_bytes();assert str(stable/'target/release/panzerrechnersystem').encode() in cmd and b'fea-range' in cmd
   for name in ['fea_range_state.json','fea_range_cameras.json','fea_range_presets.json']:
    shutil.copy2(stable/'checkout/target'/name,run/('backup-'+name))
   os.kill(pid,signal.SIGTERM)
   for _ in range(100):
    if not proc.exists():break
    time.sleep(.1)
env['KESSEL_FEA_RANGE_FILM']=str(run/('preview-plan.json' if mode=='preview' else 'plan.json'))
try:
 with (run/(mode+'.log')).open('w') as out:
  p=subprocess.Popen([str(root/'fea-range/target/release/panzerrechnersystem'),'fea-range','--shots',str(run/'unused')],cwd=root/'voxel-runtime',env=env,stdout=out,stderr=subprocess.STDOUT)
  (run/(mode+'-pid.txt')).write_text(str(p.pid));print('Rendering pid',p.pid,'log',run/(mode+'.log'),flush=True)
  status=p.wait()
 print('Render result',status,flush=True)
 if status:raise RuntimeError('Render failed; see log')
except:
 relaunch();raise
else:
 if mode!='preview':relaunch()
