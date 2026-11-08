"""Read only the authorized game asset tree; extract the opening road corridor, not full terrain."""
import json,math,pathlib,hashlib
source=pathlib.Path('/windows/shop/panzerrechnersystem/assets/worldgen/workshops/gaea-canyon-run-v2/manifest_edits.json')
j=json.loads(source.read_bytes())['canyon_course'];start=j['start'];route=j['route'];cumulative=[0.0]
for a,b in zip(route,route[1:]):cumulative.append(cumulative[-1]+math.dist(a,b))
best=(1e30,0)
for i,(a,b) in enumerate(zip(route,route[1:])):
 dx,dz=b[0]-a[0],b[1]-a[1];t=max(0,min(1,((start['x']-a[0])*dx+(start['z']-a[1])*dz)/(dx*dx+dz*dz)))
 d=math.hypot(start['x']-a[0]-dx*t,start['z']-a[1]-dz*t)
 if d<best[0]:best=(d,cumulative[i]+math.hypot(dx,dz)*t)
origin_s=best[1]
def sample(s):
 absolute=origin_s+s
 for i in range(len(route)-1):
  if cumulative[i+1]>=absolute:
   t=(absolute-cumulative[i])/(cumulative[i+1]-cumulative[i]);a,b=route[i:i+2]
   return [round(s,6),round(a[0]+(b[0]-a[0])*t-start['x'],6),round(a[1]+(b[1]-a[1])*t-start['z'],6)]
 raise ValueError('corridor exceeds source road')
knots=sorted(set([-40.0,0.0,1000.0,1120.0]+[s-origin_s for s in cumulative if -40<s-origin_s<1120]))
out={'schema':'panzer-web-canyon-corridor/v1','source':str(source),'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'native_start':start,'road_s_m':[-40,1120],'enemy_start_s_m':1000,'floor_half_width_m':28,'note':'Clipped road only; terrain and rocks are original coarse procedural proxies, not native heightfield.','points':[sample(s) for s in knots]}
path=pathlib.Path(__file__).resolve().parents[1]/'src/canyon-road.json';path.write_text(json.dumps(out,separators=(',',':'))+'\n');print(f'{path}: {len(knots)} road knots, {path.stat().st_size} bytes')
