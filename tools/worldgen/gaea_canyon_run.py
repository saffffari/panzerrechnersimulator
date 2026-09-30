#!/usr/bin/env python3
"""Canyon Run on Alex's Gaea canyon, a placeholder for the unbuilt winter map.

Preserves the original 5000 m / 2500 m export scale and north-up orientation.
A low-gradient route is fitted through the authored valley; winter buildings,
cover and defence candidates use the existing course/urban grammar. Runtime
rolls the enemy deployment independently of these stable landmarks.
"""
import argparse
import heapq
import json
import math
from pathlib import Path
import numpy as np
from PIL import Image
import canyon_course as cc

ROOT = Path(__file__).resolve().parents[2]
TOOL_VERSION = 4
SOURCE = Path('/citadel/gaea_maps/panzer/canyon')
DEST = ROOT / 'assets/worldgen/workshops/gaea-canyon-run'
EXTENT = 5000.0
ELEVATION = 2500.0
RES = 1025
# Pixel coordinates on the source: the route follows the valley's two arms and
# its northern saddle, then breaks east. These are course-design anchors only.
ANCHORS = [(1400, 2780), (1860, 1550), (2310, 1580), (2450, 2460), (3440, 2240)]


class SurfaceGrid(cc.uid.Grid):
    """The same diagonal triangle surface as src/terrain.rs, including at pads."""
    def sample(self, H, x, z):
        gx, gz = np.clip([self.col(x), self.col(z)], 0, self.res - 1)
        ix, iz = min(int(gx), self.res-2), min(int(gz), self.res-2)
        tx, tz = gx-ix, gz-iz
        a,b,c,d = H[iz,ix],H[iz,ix+1],H[iz+1,ix],H[iz+1,ix+1]
        return float(a+(b-a)*tx+(c-a)*tz if tx+tz <= 1 else
                     d+(c-d)*(1-tx)+(b-d)*(1-tz))


def walkable_nodes(H, spacing):
    # Both real triangles, buffered by a full cell for the Tiger's tracks and
    # smoothing. Coarse endpoint grades miss narrow Gaea ridges between samples.
    a,b,c,d=H[:-1,:-1],H[:-1,1:],H[1:,:-1],H[1:,1:]
    grades=np.maximum(np.hypot(b-a,c-a),np.hypot(d-c,d-b))/spacing
    pad=np.pad(grades,((1,1),(1,1)),constant_values=np.inf)
    node=np.maximum.reduce([pad[i:i+len(H),j:j+len(H)] for i in (0,1) for j in (0,1)])
    pad=np.pad(node,1,constant_values=np.inf)
    return np.maximum.reduce([pad[i:i+len(H),j:j+len(H)] for i in range(3) for j in range(3)]) <= .40


def astar(H, start, goal, spacing, clear):
    n = H.shape[0]
    def cell(p): return tuple(int(round(v / 4095 * (n-1))) for v in p)
    start, goal = cell(start), cell(goal)
    todo = [(0., 0., start)]; costs = {start: 0.}; parents = {}
    while todo:
        _, cost, at = heapq.heappop(todo)
        if cost != costs.get(at): continue
        if at == goal: break
        x,z=at
        for dx,dz in ((1,0),(-1,0),(0,1),(0,-1),(1,1),(-1,1),(1,-1),(-1,-1)):
            q=(x+dx,z+dz)
            if not (2 <= q[0] < n-2 and 2 <= q[1] < n-2) or not clear[q[1],q[0]]: continue
            length=math.hypot(dx,dz)*spacing
            slope=abs(float(H[q[1],q[0]]-H[z,x]))/length
            if slope > .32: continue
            # Low valleys and gentle grades, without modifying the source geology.
            extra=length*(1.+(max(0.,float(H[q[1],q[0]])-45.)/55.)**2+slope*6.)
            v=cost+extra
            if v < costs.get(q, float('inf')):
                costs[q]=v; parents[q]=at
                heuristic=math.hypot(q[0]-goal[0],q[1]-goal[1])*spacing
                heapq.heappush(todo,(v+heuristic,v,q))
    if goal not in costs: raise ValueError(f'No passable Gaea route: {start} -> {goal}')
    out=[goal]
    while out[-1] != start: out.append(parents[out[-1]])
    return np.array(out[::-1],dtype=float)*spacing-EXTENT*.5


class GaeaRoute(cc.Ravine):
    def __init__(self, H):
        spacing=EXTENT/(len(H)-1);clear=walkable_nodes(H,spacing)
        pieces=[astar(H,a,b,spacing,clear) for a,b in zip(ANCHORS,ANCHORS[1:])]
        route=np.concatenate([p[:-1] for p in pieces]+[pieces[-1][-1:]])
        # One-cell smoothing removes the grid staircase without cutting off valley bends.
        for _ in range(3): route[1:-1]=(route[:-2]+2*route[1:-1]+route[2:])*.25
        s=np.r_[0.,np.cumsum(np.linalg.norm(np.diff(route,axis=0),axis=1))]
        sample=np.r_[np.arange(0,s[-1],8.),s[-1]]
        route=np.column_stack([np.interp(sample,s,route[:,i]) for i in range(2)])
        self.x,self.z=route.T
        self.s=np.r_[0.,np.cumsum(np.hypot(np.diff(self.x),np.diff(self.z)))];self.L=float(self.s[-1])
        dx,dz=np.gradient(self.x),np.gradient(self.z);n=np.hypot(dx,dz)
        self.tx,self.tz=dx/n,dz/n
    @staticmethod
    def width(f):
        return cc.interp_profile([(0,190),(.15,150),(.25,190),(.36,300),(.48,170),(.62,290),(.77,170),(.88,260),(1,180)],f)
    def half_width(self,f,side): return self.width(f)*.5


def fields_for_route(rv, X, Z, H):
    # Chunked nearest samples, bounded memory; the route need not be monotonic in Z.
    pts=np.column_stack([X.ravel(),Z.ravel()]); route=np.column_stack([rv.x,rv.z])
    ids=np.empty(len(pts),dtype=np.int32)
    for start in range(0,len(pts),4096):
        p=pts[start:start+4096]
        d=((p[:,None,:]-route[None,:,:])**2).sum(axis=2)
        ids[start:start+len(p)]=d.argmin(axis=1)
    ix=ids.reshape(H.shape); dx=X-rv.x[ix];dz=Z-rv.z[ix]
    D=dx*(-rv.tz[ix])+dz*rv.tx[ix];F=rv.s[ix]/rv.L
    grid=SurfaceGrid(RES,EXTENT/(RES-1))
    route_h=np.array([grid.sample(H,x,z) for x,z in route])
    # Buildings and cover only occupy low, gentle ground beside the route.
    gz,gx=np.gradient(H,grid.sp)
    E=np.maximum(np.abs(D)-rv.width(F)*.5,(H-route_h[ix]-5.)*8.)
    E=np.maximum(E,(np.hypot(gx,gz)-.23)*160.)
    return dict(F=F,D=D,E=E,tw=np.zeros_like(H),P=H,floor=route_h[ix])


def build(source=SOURCE):
    src=Image.open(source/'Export.png')
    if src.mode not in ('I;16','I;16L'): raise ValueError('Gaea export must remain native 16-bit')
    raw=np.array(src,dtype=np.float64)
    if raw.shape != (4096,4096):
        raise ValueError('Route anchors require the original 4096-square canyon export')
    # Preserve endpoints and source precision; deterministic bilinear resampling to 4.883 m grid.
    axis=np.linspace(0,raw.shape[0]-1,RES);lo=axis.astype(int);hi=np.minimum(lo+1,raw.shape[0]-1);w=axis-lo
    rows=raw[lo]*(1-w[:,None])+raw[hi]*w[:,None]
    H=(rows[:,lo]*(1-w[None,:])+rows[:,hi]*w[None,:])*ELEVATION/65535.
    rv=GaeaRoute(H); grid=SurfaceGrid(RES,EXTENT/(RES-1));X,Z=np.meshgrid(grid.axis,grid.axis)
    fields=fields_for_route(rv,X,Z,H)
    # Centre-relative runtime datum, retained precisely in the receipt.
    datum=float(H[RES//2,RES//2]);H-=datum;fields['floor']-=datum
    site=cc.Site(rv,grid,H,fields)
    cc.place_buildings(site)
    original=H.copy()
    mounds,craters=cc.apply_pads(site,X,Z,H)
    # Keep the playable track on the unchanged Gaea surface. Building aprons and
    # shell scars blend back to that surface outside a 16 m-wide travel strip.
    H[:]=original+(H-original)*cc.smoothstep((np.abs(fields['D'])-8.)/6.)
    site.H=H
    Dr=cc.dress(site,H)
    # Legible rhythm: shelter at the start, approach cover at each bend, repair lay-bys,
    # then the factory and final blocking position. Keep the central 24 m clear.
    for j,f in enumerate(np.linspace(.07,.92,43)):
        for side in (-1,1):
            x,z=rv.world(float(f),side*(23+12*cc.rnd(j,54)))
            if site.on_floor(x,z,3) and Dr.free(x,z,3):
                Dr.add('route_trace','log' if j%3 else 'crate',x,z,math.degrees(math.atan2(rv.at(f)[2],rv.at(f)[3])),1.0)
                if j%2==0: Dr.add('route_drift','snow_drift',x+1.6,z,.0,(2.4,2.,1.2),-.05)
    # Follow the curved valley through both arenas. Straight chords between the
    # old procedural map's four corners cut across this Gaea terrain's ridges.
    beats=[]
    for name,a,b in [('A',.84,.92),('B',.60,.69)]:
        fs=np.linspace(a,b,12)
        beats.append((name,[(float(f),-5.) for f in fs]+[(float(f),5.) for f in fs[::-1]]))
    street,course=cc.defence(site,Dr,beats)
    # More alternative infantry posts, grouped by encounter sector. Stable terrain and
    # landmarks; runtime chooses six different defended corners on each new seed.
    posts=[[] for _ in range(3)]
    for b in site.buildings:
        if b['razed'] or b['group'] not in ('settlement','factory','strongpoint'): continue
        sector=('settlement','factory','strongpoint').index(b['group'])
        x0,x1,z0,z1=b['rect']
        for x,z in [(x0-2,z0-2),(x1+2,z1+2)]:
            if site.on_floor(x,z,2) and Dr.free(x,z,1):
                posts[sector].append(dict(x=cc.r3(x),z=cc.r3(z),heading_deg=0,host=b['id']))
    if all(posts):
        count=min(map(len,posts));course['garrison']=[posts[s][j] for j in range(count) for s in range(3)]
    course['tool']='tools/worldgen/gaea_canyon_run.py';course['tool_version']=TOOL_VERSION
    course['provenance']={
        'terrain':'User-authored Gaea canyon export; winter placeholder, not a surveyed Stalingrad ravine.',
        'buildings':cc.PROVENANCE['buildings'],
        'defence':cc.PROVENANCE['defence'],
        'scope':'Generative mechanics course, not a recorded engagement. Escorts currently walk beside tanks.'}
    course['route']=[[cc.r3(x),cc.r3(z)] for x,z in zip(rv.x,rv.z)]
    course['design']=[dict(name=n,from_fraction=a,to_fraction=b) for n,a,b in [
        ('sheltered_start',0,.12),('first_gun_approach',.12,.27),('ruined_settlement',.28,.44),
        ('saddle_crossing',.44,.56),('factory_patrol',.57,.72),('blocking_gun',.72,.83),('final_armour',.83,1.)]]
    travel=validate_travel_surface(rv,grid,H)
    instances=[{k:b[k] for k in ('id','typology','seed','width_m','depth_m','storeys','cellar','condition','x','z','yaw_deg')} for b in site.buildings if not b['razed']]
    manifest=cc.manifest_document(site,H,Dr,course)
    manifest['authoring'].update(tool='tools/worldgen/gaea_canyon_run.py',tool_version=TOOL_VERSION)
    manifest['terrain'].update(res=RES,spacing=grid.sp)
    for zone in course['gun_zones']:
        if len(zone['candidates'])<8: raise ValueError(f'Insufficient legal gun sites: {zone}')
    if len(instances)<25: raise ValueError(f'Insufficient settlement detail: {len(instances)} buildings')
    return manifest,dict(base_placements=len(manifest['placements']),base_markers=len(manifest['markers']),ops=[],
        building_instances=instances,street_encounter=street,canyon_course=course,
        environment_lighting=cc.LIGHTING,display_tuning=cc.DISPLAY,sound_emitters=[],authored_lights=[]),H,dict(
        source_directory=str(source),source_height_sha256=cc.sha256_file(source/'Export.png'),
        source_extent_m=EXTENT,source_elevation_range_m=[0,ELEVATION],runtime_datum_m=datum,
        runtime_spacing_m=grid.sp,route_length_m=rv.L,standing_buildings=len(instances),dressing=Dr.counts,
        travel_surface=travel,
        provenance=course['provenance'],
        gun_candidates={z['id']:len(z['candidates']) for z in course['gun_zones']},
        terrain_changes='Local building pads and shell craters; original source unchanged. Gaea canyon is a winter placeholder, not the future winter campaign map.',
        mounds=mounds,craters=craters)


def validate_travel_surface(rv,grid,H):
    distance=np.arange(0,rv.L,2.)
    route=np.column_stack([np.interp(distance,rv.s,v) for v in (rv.x,rv.z)])
    tangent=np.gradient(route,axis=0);tangent/=np.linalg.norm(tangent,axis=1)[:,None]
    side=np.column_stack([-tangent[:,1],tangent[:,0]])
    grades=[];heights=[]
    for offset in (-4.,-2.,0.,2.,4.):
        pts=route+side*offset;h=np.array([grid.sample(H,*p) for p in pts]);heights.append(h)
        grades.extend(np.abs(np.diff(h))/np.linalg.norm(np.diff(pts,axis=0),axis=1))
    max_grade=float(max(grades));cross=float(np.max(np.abs(heights[-1]-heights[0]))/8.)
    if max_grade>.40 or cross>.40:
        raise ValueError(f'Tiger travel strip has an unsafe grade: along={max_grade}, cross={cross}')
    return dict(sample_spacing_m=2,checked_width_m=8,max_along_grade=max_grade,max_cross_grade=cross,
                max_along_degrees=math.degrees(math.atan(max_grade)),surface='same triangles as terrain.rs')


def write(source=SOURCE,dest=DEST,plan=None):
    dest=Path(dest)
    # Protect all hand edits, including building/weather edits without an ops entry.
    existing=dest/'manifest_edits.json'
    if existing.exists() and json.loads(existing.read_text()).get('ops'):
        raise ValueError(f'Preserving editor operations in {existing}; choose a new output directory')
    previous=dest/'canyon-course.receipt.json'
    if any((dest/name).exists() for name in cc.OUTPUT_FILES):
        hashes=json.loads(previous.read_text()).get('outputs',{}) if previous.exists() else {}
        for name in cc.OUTPUT_FILES:
            p=dest/name
            if p.exists() and hashes.get(name)!=cc.sha256_file(p):
                raise ValueError(f'Preserving hand-edited {p}; choose a new output directory')
    manifest,edits,H,receipt=build(Path(source));dest.mkdir(parents=True,exist_ok=True)
    (dest/'manifest.v1.json').write_text(cc.dumps(manifest,indent=None));np.save(dest/'heights.npy',H.astype('<f4'))
    (dest/'manifest_edits.json').write_text(cc.dumps(edits))
    receipt.update(schema='panzerrechnersystem.gaea-canyon-run',version=1,tool_version=TOOL_VERSION,
        outputs={f:cc.sha256_file(dest/f) for f in cc.OUTPUT_FILES})
    (dest/'canyon-course.receipt.json').write_text(cc.dumps(receipt))
    if plan:
        import matplotlib
        matplotlib.use('Agg')
        import matplotlib.pyplot as plt
        fig,ax=plt.subplots(figsize=(10,10));ax.imshow(H,extent=(-2500,2500,2500,-2500),cmap='terrain')
        route=np.array(edits['canyon_course']['route']);ax.plot(route[:,0],route[:,1],color='white',lw=2)
        for b in edits['building_instances']: ax.plot(b['x'],b['z'],'s',color='orange',ms=3)
        for zone in edits['canyon_course']['gun_zones']:
            sites=[edits['street_encounter']['gun_sites'][i] for i in zone['candidates']]
            ax.scatter([s['x'] for s in sites],[s['z'] for s in sites],s=1,alpha=.4)
        for b in edits['street_encounter']['beats']:
            pts=np.array(b['points']);ax.plot(pts[:,0],pts[:,1],color='red',lw=1)
        ax.set_title('Canyon Run — Gaea terrain, route, buildings, candidate defences');ax.set_aspect('equal')
        fig.savefig(plan,dpi=130);plt.close(fig)
    print(json.dumps({k:v for k,v in receipt.items() if k not in ('mounds','craters')},indent=2))

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--source',type=Path,default=SOURCE);p.add_argument('--dest',type=Path,default=DEST);p.add_argument('--plan',type=Path)
    a=p.parse_args();write(a.source,a.dest,a.plan)
