"""Generate the private, continuous expedition theatre from the tracked mission/geography source.
No downloads, source assets modified, or unrelated asset directories scanned.
Terrain mesh and physical heightfield use the exact same generated samples.
"""
from pathlib import Path
import json, math, hashlib
import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
SPEC = ROOT / 'catalog/expedition_theatre.v1.json'
OUT = ROOT / 'assets/worldgen/expedition/the_march_v1'

def generate():
    spec = json.loads(SPEC.read_text(encoding='utf-8'))
    rng = np.random.default_rng(spec['seed'])
    n, extent = spec['grid_res'], spec['extent_m']
    spacing = extent / (n-1)
    axis = np.linspace(-extent/2, extent/2, n, dtype=np.float32)
    x,z = np.meshgrid(axis,axis)
    # Broad physical ridges make dead ground, alternate approaches and hull-down positions.
    h = (14 + 11*np.sin(x/790)*np.cos(z/980) + 7*np.sin((x+z)/430)
         + 2*np.cos(x/131)*np.sin(z/157)).astype(np.float32)
    h += 30*np.exp(-((x+3800)/1700)**2 - ((z+2900)/1600)**2)
    road_distance = np.full_like(h, 1e6)
    for road in spec['roads']:
        for a,b in zip(road,road[1:]):
            ax,az=a; bx,bz=b; vx,vz=bx-ax,bz-az
            t=np.clip(((x-ax)*vx+(z-az)*vz)/(vx*vx+vz*vz),0,1)
            d=np.sqrt((x-(ax+t*vx))**2+(z-(az+t*vz))**2)
            road_distance=np.minimum(road_distance,d)
    # Town yards are graded into their surroundings, without freestanding rectangular bases.
    yards=[spec['camp']]+[[spec['river']['center_x']+spec['river']['meander_m']*math.sin(s['position'][1]/spec['river']['wavelength_m'])-175,s['position'][1]] if s['kind']=='bridge' else s['position'] for s in spec['sites']]
    for px,pz in yards:
        ix=int(round((px+extent/2)/spacing)); iz=int(round((pz+extent/2)/spacing))
        target=float(h[iz,ix]); d=np.sqrt((x-px)**2+(z-pz)**2)
        blend=np.clip((220-d)/85,0,1); blend=blend*blend*(3-2*blend)
        h=h*(1-blend)+target*blend
    r=spec['river']; center=r['center_x']+r['meander_m']*np.sin(z/r['wavelength_m'])
    d=np.abs(x-center)
    bank=np.clip((d-r['half_width_m'])/55,0,1)
    h=(8+h*bank).astype(np.float32)
    # Embanked road approaches meet actual authored crossing meshes at the surveyed height.
    crossings=[]
    for cz in r['crossings_z']:
        cx=r['center_x']+r['meander_m']*math.sin(cz/r['wavelength_m'])
        blend=np.clip((125-np.abs(x-cx))/60,0,1)*np.clip((70-np.abs(z-cz))/40,0,1)
        # Ground stays below the deck in the channel. The bridge, not invisible terrain, supports it.
        desired=np.where(np.abs(x-cx)<42,8.,14.)
        h=h*(1-blend)+desired*blend
        crossings.append({'center':[cx,cz],'deck_y':14.,'half_length':52.,'half_width':5.5})
    def grade(px,pz):
        gx=(px+extent/2)/spacing; gz=(pz+extent/2)/spacing
        ix=max(0,min(n-2,int(gx))); iz=max(0,min(n-2,int(gz))); tx=gx-ix;tz=gz-iz
        a,b,c,d=map(float,(h[iz,ix],h[iz,ix+1],h[iz+1,ix],h[iz+1,ix+1]))
        return a+(b-a)*tx+(c-a)*tz if tx+tz<=1 else d+(c-d)*(1-tx)+(b-d)*(1-tz)
    OUT.mkdir(parents=True,exist_ok=True)
    np.save(OUT/'heights.npy',h.astype('<f4'))
    # Authored ground pigment: snow in the north, tilled ochre fields east, damp western woodland.
    noise=rng.normal(0,3,(n,n,1))
    west=np.array([87,91,70]);east=np.array([123,113,78]);snow=np.array([187,194,194])
    mix=np.clip((x+1300)/4600,0,1)[...,None]
    rgb=west*(1-mix)+east*mix
    snowline=np.clip((-z-1600)/1300,0,1)[...,None]
    rgb=rgb*(1-snowline)+snow*snowline
    # Crop boundaries need authored parcels/hedgerows. A world-aligned checkerboard here
    # read as renderer grid lines from above and repeated through woodland and snow.
    # Keep the same pigment noise draw so geometry/placement generation stays identical.
    rgb+=noise
    roads=np.clip((12-road_distance)/5,0,1)[...,None]
    road_tone=np.array([108,104,91])*(1-snowline)+np.array([153,157,153])*snowline
    rgb=rgb*(1-roads)+road_tone*roads
    Image.fromarray(np.clip(rgb,0,255).astype('uint8')).resize((4096,4096),Image.Resampling.BILINEAR).save(OUT/'ground.png')
    old=json.loads((ROOT/'assets/worldgen/revisions/act1-winter-approach/20260913-v1/manifest.v1.json').read_text())
    def asset(token):
        found=next((p for p in old['assets'] if token in p and (ROOT/p).is_file()),None)
        if found is None: raise RuntimeError('Required documented source asset missing: '+token)
        return found
    assets=[asset('pine_tree_n_1.glb'),asset('pine_tree_n_2.glb'),asset('debris_bricks_mx_1.glb'),asset('wooden_crate_2_a.glb'),asset('metal_barrel_hr_1.glb')]
    placements=[];buildings=[]
    def road_clear(px,pz,margin):
        for road in spec['roads']:
            for (ax,az),(bx,bz) in zip(road,road[1:]):
                vx,vz=bx-ax,bz-az
                t=max(0,min(1,((px-ax)*vx+(pz-az)*vz)/(vx*vx+vz*vz)))
                if math.hypot(px-ax-t*vx,pz-az-t*vz)<margin:return False
        return True
    def place(idx,px,pz,yaw=0,scale=1):
        # The road is an actual navigable corridor, not a line painted through props.
        if not road_clear(px,pz,10 if idx>1 else 20):return
        placements.append([idx,round(px,3),round(grade(px,pz),4),round(pz,3),round(yaw,2),scale])
    # Woodland clusters leave surveyed roads and mission yards usable.
    for _ in range(32000):
        px,pz=rng.uniform(-5700,5700,2)
        ix=int((px+extent/2)/spacing); iz=int((pz+extent/2)/spacing)
        if road_distance[iz,ix]<20 or abs(px-(r['center_x']+r['meander_m']*math.sin(pz/r['wavelength_m'])))<75: continue
        if any(math.hypot(px-sx,pz-sz)<240 for sx,sz in [spec['camp']]+[s['position'] for s in spec['sites']]):continue
        if px>900 and rng.random()<.72:continue
        cluster=(math.sin(px/290)+math.cos(pz/380)+math.sin((px+pz)/510))
        if cluster<.25:continue
        place(int(rng.integers(0,2)),px,pz,float(rng.uniform(0,360)),float(rng.uniform(.85,1.2)))
    for s in spec['sites']:
        px,pz=s['position']
        if s['kind']=='bridge':
            px=r['center_x']+r['meander_m']*math.sin(pz/r['wavelength_m'])-175
        # Two streets and side yards give separate attack lanes, cover and visible landmarks.
        # Source buildings retain usable interiors and destructible material pieces.
        rows=[-78,-44,44,78] if s['kind'] in ('assault','finale','workshop','defence') else [-44,44]
        lots=[(dx,dz) for dx in [-58,-28,28,58] for dz in rows]
        for j,(dx,dz) in enumerate(lots):
            if not road_clear(px+dx,pz+dz,19):continue
            typ='workshop' if (s['kind']=='workshop' and j%3==0) or j==0 else 'merchant_brick_house'
            condition=['intact','shelled','partly_collapsed','burned','intact'][j%5]
            buildings.append(dict(id=f"march.site{s['id']}.building{j}",typology=typ,seed=int(rng.integers(1,2**31)),width_m=12.,depth_m=8. if typ=='workshop' else 12.,storeys=1 if typ=='workshop' else 2,cellar=False,condition=condition,x=px+dx,z=pz+dz,yaw_deg=0. if dz>0 else 180.))
            for _ in range(14 if condition!='intact' else 4):
                angle=rng.uniform(0,math.tau);radius=rng.uniform(9,13)
                place(2,px+dx+math.sin(angle)*radius,pz+dz+math.cos(angle)*radius,float(rng.uniform(0,360)))
            for k in range(4):place(3 if k<3 else 4,px+dx-4+k*.85,pz+dz+9,0)
        for _ in range(12):
            dx,dz=rng.uniform(-12,12,2)
            if s['kind']=='dispatch' and abs(dx)<2 and abs(dz+9)<3:continue
            place(3 if s['kind']!='fuel' else 4,px+dx,pz+dz,float(rng.uniform(0,360)))
    def marker(name,kind,p):return {'name':name,'kind':kind,'x':p[0],'y':grade(*p),'z':p[1]}
    markers=[marker('Support detail','Spawn',spec['camp']),marker('Last corridor','Goal',spec['sites'][-1]['position'])]
    for s in spec['sites']:markers.append(marker(s['name'],'Bridge' if s['kind']=='bridge' else 'Church',s['position']))
    manifest={'schema':'kessel.act','version':1,'act_id':1,'act':'The March','seed':spec['seed'],
      'authoring':{'mode':'expedition','source':'catalog/expedition_theatre.v1.json','source_sha256':hashlib.sha256(SPEC.read_bytes()).hexdigest(),'generator_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()},
      'terrain':{'res':n,'spacing':spacing,'heights':'heights.npy','texture':(OUT/'ground.png').relative_to(ROOT).as_posix(),'grass_season':'spring'},
      'assets':assets,'placements':placements,'markers':markers,'wire_lines':[], 'expedition_crossings':crossings}
    edits={'base_markers':len(markers),'base_placements':len(placements),'ops':[],'building_instances':buildings,'authored_lights':[]}
    for name,obj in [('manifest.v1.json',manifest),('manifest_edits.json',edits)]:
        (OUT/name).write_text(json.dumps(obj,indent=2)+'\n',encoding='utf-8',newline='\n')
    receipt={'extent_m':extent,'height_samples':n*n,'triangles':2*(n-1)**2,'placements':len(placements),'buildings':len(buildings),'sites':len(spec['sites']),'crossings':crossings,'source_sha256':manifest['authoring']['source_sha256'],'provenance':spec['provenance']}
    (OUT/'generation_receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8',newline='\n')
    print(json.dumps(receipt))
if __name__=='__main__':generate()
