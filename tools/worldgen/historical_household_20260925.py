#!/usr/bin/env python3
"""Reusable Solnechnaya Street material interpretation; never alters a campaign map.

Run from shader-graphics: python tools/worldgen/historical_household_20260925.py
The September event's later winter remains are interpreted through owned modular assets.
Source testimony establishes relations, not this footprint, furniture or exact counts.
"""
from __future__ import annotations
import math
from collections import Counter
import numpy as np
from historical_scene_common_20260925 import Scene, ROOT, RECEIPT_ROOT, save_json


def build():
    s=Scene('a1_burned_cellar_household','household',half=32,spacing=.25)
    def terrain(x,z):
        # Basement excavation ends under its retaining walls. Separate west access ramp;
        # no invented claim that the witnessed courtyard trench connected to this cellar.
        house=np.maximum(np.abs(x)-5.75,np.maximum(-7.75-z,z-3.75))
        basement=-2.83*np.clip((.65-house)/.65,0,1)
        ramp_depth=-2.43*np.clip((x+16)/10,0,1)
        ramp_edge=np.clip((1.85-np.abs(z+2))/.65,0,1)
        ramp=np.where((x>=-16)&(x<=-5.75),ramp_depth*ramp_edge,0)
        center=8.2+.65*np.sin(np.clip((z-8)/14,0,1)*math.pi)
        sides=np.clip((1.5-np.abs(x-center))/.65,0,1)
        ends=np.clip(np.minimum((z-8)/3,(22-z)/3),0,1)
        trench=-.9*sides*ends
        return np.minimum(np.minimum(basement,ramp),trench)
    s.set_terrain(terrain)
    soot=s.bank.variant('beam_hc_horizonatal_1','household_joist',[.19,.16,.13])
    splinter=s.bank.variant('wooden_plank_1','household_splinter',[.22,.19,.16])
    splinter2=s.bank.variant('wooden_plank_2','household_splinter2',[.26,.22,.18])
    darkbricks=s.bank.variant('debris_bricks_mx_1','household_ash_bricks',[.38,.35,.32])
    darkbrick=s.bank.variant('brick_mx_2','household_ash_brick',[.43,.36,.31])
    roles=Counter()
    def put(key,x,z,y=None,yaw=0,scale=1,role='material residue',evidence='reconstruction_choice'):
        roles[role]+=1
        ident=s.put(key,x,z,y,yaw=yaw,scale=scale,role=role,evidence=evidence)
        s.evidence[-1].update(source_ids=['poz_1943_03_14','poz_1943_en_ed'],
            source_scope='Event context only; this object type, pose, dimension and coordinate are inferred.' if evidence=='reconstruction_choice' else 'Attested material relationship; this particular mesh, count and placement remain inferred.')
        return ident
    # 12m by12m shell: cellar datum -2.4; ground-floor datum0; upper floor3.15.
    for x in (-4,0,4):
        for z in (-6,-2,2):
            put('floor_ceiling_hr_1',x,z,-2.8,role='cellar floor')
            put('floor_ceiling_hr_1',x,z,-.4,role='ground-floor deck')
    # Cellar retaining fabric. West central bay is a true2.1m opening with an independent ramp.
    for x in (-4,0,4):
        for z in (-8,4):put('wall_hr_1',x,z,-2.4,scale=[1,.8,1],role='cellar retaining wall')
    for z in (-6,-2,2):
        put('wall_hr_1',6,z,-2.4,yaw=90,scale=[1,.8,1],role='cellar retaining wall')
    for z in (-6,2):put('wall_hr_1',-6,z,-2.4,yaw=90,scale=[1,.8,1],role='cellar retaining wall')
    for z in (-3.5,-.5):put('wall_hr_1',-6,z,-2.4,yaw=90,scale=[.235,.8,1],role='cellar entrance jamb')
    put(soot,-6,-2,-.22,yaw=90,scale=[.27,.45,1],role='cellar threshold lintel')
    # Burned residential wall remains, not a surveyed reconstruction of the witness's house.
    for x in (-4,0,4):put('window_hr_1',x,-8,0,role='surviving residential back wall',evidence='attested_condition_interpreted')
    for z in (-6,-2,2):
        put('wall_hr_1',6,z,0,yaw=90,role='surviving residential side wall',evidence='attested_condition_interpreted')
        put('window_hr_1',-6,z,0,yaw=90,role='surviving residential side window',evidence='attested_condition_interpreted')
    # Main entrance is unobstructed for modern review; exact wartime blockade is not frozen in place.
    for x in (-1.5,1.5):put('wall_hr_1',x,4,0,scale=[.235,1,1],role='open domestic entrance jamb')
    put('wall_hr_1',0,4,2.45,scale=[.53,.183,1],role='entrance lintel')
    put('window_hr_1',-4,4,0,yaw=180,role='escaped house window relationship',evidence='attested_condition_interpreted')
    put('wall_hr_1_hole_1',4,4,0,yaw=180,role='later open collapse beside escaped-window facade')
    # A narrow north range still carries upper floor fragments. The front half is open to weather.
    for x in (-4,0,4):put('floor_ceiling_hr_1',x,-6,2.75,role='surviving upper deck fragment')
    for x in (-4,0):put('window_hr_1',x,-8,3.15,role='two-storey remnant silhouette',evidence='attested_condition_interpreted')
    put('wall_hr_1',4,-8,3.15,scale=[1,.42,1],role='broken upper wall stub')
    put('window_hr_1',-6,-6,3.15,yaw=90,role='upper side remnant')
    put('wall_hr_1',6,-6,3.15,yaw=90,scale=[1,.30,1],role='broken upper side stub')
    put('pillar_hr_1_broken',6,4,0,role='broken facade corner')
    # Accessible upper remnant stair rises towards -Z and ends flush with the3.15m deck.
    put('stairs_5_wood',-4,-2,.15,scale=[.85,1,1],role='upper-floor circulation reconstruction')
    for x in (-4.8,-3.2):put('pillar_hr_1_broken',x,-3.85,3.15,scale=[.25,.55,.25],role='fractured stair landing edge')
    # Charred structure follows wall bearings; high joists leave standing foot routes open.
    for i,z in enumerate((-7.3,-6.15,-5.0)):
        put(soot,0,z,5.97,scale=[1.42,.72,.85],role='burned roof joist',evidence='attested_condition_interpreted')
    for i,x in enumerate((-5.35,-4.15,-2.8,-1.35,.35,2.0,3.55,5.15)):
        put(soot,x,-.55,3.02,yaw=90,scale=[.91,.62,.72],role='burned floor joist',evidence='attested_condition_interpreted')
    for x,z,yaw,length in [(-5.1,2.7,28,1.6),(4.8,.6,-24,2.2),(4.25,-3.0,42,1.25),(5.0,-5.4,16,1.7),(-2.9,-6.0,-18,1.1),(3.4,2.9,78,1.45)]:
        put(soot,x,z,.055,yaw=yaw,scale=[length/8,.36,.8],role='fallen charred structural end')
    # Empty domestic residue is deliberately sparse; no invented family/body tableau or table setting.
    put('table_large_3',3.8,-6.25,.015,yaw=7,scale=.90,role='unattested domestic work surface')
    put('chair_mp_2',4.1,-4.92,.015,yaw=145,scale=1.28,role='unattested displaced chair')
    put('bed_1_frame',1.0,-6.1,.015,yaw=0,scale=.80,role='unattested empty bed frame')
    put('bucket_mx_1',-4.8,-5.8,-2.38,scale=.85,role='unattested cellar container')
    put('wooden_crate_1',-4.8,1.8,-2.38,yaw=-12,scale=.70,role='unattested cellar storage remnant')
    put('bench_mx_1',3.9,-6.5,-2.38,yaw=0,scale=.85,role='unattested empty cellar bench')
    put('wooden_crate_1',4.7,.8,-2.38,yaw=14,scale=.60,role='unattested cellar storage remnant')
    # Modest enclosure establishes a household plot; the central court and west ramp stay unblocked.
    for x in (-10,-6,-3,3,6,10):put('wall_hr_1',x,23,0,scale=[.70 if abs(x)==3 else .96,.25,1],role='courtyard boundary residue')
    for z in (9,13,17,21):
        put('wall_hr_1',13,z,0,yaw=90,scale=[.94,.24,1],role='courtyard boundary residue')
        put('wall_hr_1',-13,z,0,yaw=90,scale=[.94,.16,1],role='courtyard boundary residue')
    # Spoil banks and remnant stakes track the independent courtyard trench, never the cellar ramp.
    for row,z in enumerate(np.linspace(10.5,19.5,13)):
        c=8.2+.65*math.sin((float(z)-8)/14*math.pi)
        for side in (-1,1):
            x=c+side*1.62
            put('gravel_pile_hr_1',x,float(z),yaw=(row*47+side*23)%360,scale=[.42,.66,.42],role='courtyard trench spoil',evidence='attested_condition_interpreted')
            if row%2==0:put('beam_hc_vertical_1',c+side*.95,float(z),s.ground(c+side*.95,float(z))-.10,scale=[.10,.34,.18],role='inferred trench edge timber')
    # Distinct collapse fans, not uniform map-wide noise. Negative space is kept around both paths.
    fans=[(4.5,6.1,2.15,1.5), (7.05,.3,1.0,3.0), (2.8,-8.7,2.7,.7),(-7.0,-6.0,.65,1.35)]
    for fan,(cx,cz,rx,rz) in enumerate(fans):
        for i in range(16):
            a=i*2.3999632297+fan*.61;rad=math.sqrt((i+.35)/16)
            x=cx+math.cos(a)*rx*rad;z=cz+math.sin(a)*rz*rad
            put(darkbricks if i%3==0 else 'debris_bricks_mx_2',x,z,yaw=(i*73+fan*39)%360,scale=.24+.045*(i%4),role=f'collapse fan {fan+1} masonry')
            if i%2==0:put(darkbrick,x+.16,z-.12,s.ground(x+.16,z-.12)+.02,yaw=(i*31)%360,scale=.8+.11*(i%3),role=f'collapse fan {fan+1} individual brick')
        for i in range(6):
            a=.48+i*.74;rad=.45+.08*i
            x=cx+math.cos(a)*rx*rad;z=cz+math.sin(a)*rz*rad
            put(splinter if i%2 else splinter2,x,z,s.ground(x,z)+.03,yaw=fan*32+i*27,scale=[.8+.15*i,1,.8],role=f'collapse fan {fan+1} burned lath')
    # Broken floorboards remain only in the lost front-room wings, keeping a 2m central lane.
    for side in (-1,1):
        for i in range(14):
            x=side*(2.3+.21*(i%7));z=.15+.58*(i//7)+.11*(i%3)
            put(splinter if i%2 else splinter2,x,z,.035+.01*(i%2),yaw=4+(i%4)*13,scale=[.8+.13*(i%3),1,.82],role='interrupted front-room floor lath')
    # Ash and fragments in cellar corners leave the sheltered center and threshold readable.
    for cx,cz in [(-4.8,-6.65),(4.7,-5.7),(4.7,2.55)]:
        for i in range(5):
            x=cx+.24*math.sin(i*2);z=cz+.3*math.cos(i*2)
            put(darkbricks,x,z,-2.385,yaw=i*67,scale=.18+.025*i,role='cellar edge masonry residue')
    s.socket('house_window_focus','story',[-4,1.2,4.5],180)
    s.socket('cellar_shelter','story',[-1,-1.1,-2],270)
    s.socket('courtyard_trench','story',[8.85,-.05,15],0)
    s.connectors.extend([
        {'id':'street_approach','kind':'pedestrian','position_m':[0,0,26],'heading_deg':180},
        {'id':'west_cellar_access','kind':'pedestrian','position_m':[-17,0,-2],'heading_deg':90}])
    s.view('street_house_read',[2.0,1.70,19],[0,1.9,0])
    s.view('house_threshold',[.0,1.70,3.0],[2.0,1.25,-5.3])
    s.view('cellar_shelter',[-4.8,-.70,-2.0],[3.0,-1.05,-4.3])
    s.view('exterior_review',[17,12,23],[-1,1.0,0])
    routes=[{'id':'street_to_house','stature_m':1.8288,'radius_m':.30,'points_m':[[0,0,26],[0,0,10],[0,0,5],[0,0,1],[1.6,0,-2.8],[2.3,0,-4.8]],'support':'Outdoor terrain then ground-floor source slabs; open2.1m entrance.'},
        {'id':'separate_cellar_descent','stature_m':1.8288,'radius_m':.30,'points_m':[[-17,0,-2],[-15,-.243,-2],[-12,-.972,-2],[-9,-1.701,-2],[-6,-2.4,-2],[-3,-2.4,-2],[1,-2.4,-2]],'support':'Scene terrain ramp then cellar source slabs; cellar clearance2.0m to underside of deck.'}]
    s.features.extend([{'kind':'pedestrian_routes','routes':routes},
        {'kind':'historical_relation','attested':'Two-storey house, cellar shelter, courtyard trench shelter, fire and escape through house windows; walls survived.',
         'not_attested':'Exact plan, dimensions, connection of cellar to trench, the selected furniture inventory or its placement, exact counts, snow during September.'},
        {'kind':'materials','description':'Soot variants alter material factors only; owned source geometry/textures remain unchanged. Structural paths remain original for existing collision classification.'}])
    s.notes.extend(['District-constrained Solnechnaya Street residential interpretation. Do not place in a generic village, airfield or factory-only context.',
        'Winter dressing means later remains of a September fire, not a snowfall contemporaneous with the witnessed escape.',
        'No people, graves, named possessions, readable paper/sign text, flames or active flight tableau. Object absence stays unresolved.',
        'Modern circulation geometry is a design reconstruction: cellar side entrance and courtyard trench remain physically separate. The front window represents the attested escape relationship; its precise frame position and survival are inferred.',
        'All piece counts describe this authored block only and are not archival counts. Cellar/exterior lighting comes from openings and environmental sky; no invented practical lamp added.'])
    assert 200<=len(s.items)<=450,len(s.items)
    path=s.save()
    save_json(RECEIPT_ROOT/'household.layout-receipt.json',{'scene':str(path.relative_to(ROOT)),'piece_count':len(s.items),'roles':dict(roles),
        'routes':routes,'cellar_floor_m':-2.4,'ceiling_underside_m':-.4,'cellar_clear_height_m':2.0,
        'preservation':'New reusable block only. No campaign manifest or existing user operations touched.',
        'historical_caveat':'Per-object/source relationships are in metadata.item_evidence. Layout, furniture and winter staging are inference, not exact-site evidence.',
        'next_validation':'Root isolated workshop import and GPU/foot traversal review; source-bound route sampling follows in this producer.'})
    return s,path

def check_routes(scene):
    # Source triangles, not just bounding boxes: a conservative sampled body footprint.
    # Nine vertical rays are a bounded clearance check, not native swept-capsule verification.
    import sys
    sys.path.insert(0,str(ROOT/'tools'))
    from validate_tiger_hatches import vertical_hits
    from build_winter_approach_revision import glb_geometry
    from historical_scene_common_20260925 import resolve
    cache={};meshes=[]
    for item in scene.items:
        raw=item['asset']
        if raw not in cache:cache[raw]=glb_geometry(resolve(raw))[0]
        a=math.radians(item['deg']);c,q=math.cos(a),math.sin(a)
        r=np.array([[c,0,q],[0,1,0],[-q,0,c]])
        t=(cache[raw]*item['s'])@r.T+[item['dx'],item['dy'],item['dz']]
        meshes.append((item['id'],t,t.reshape(-1,3).min(0),t.reshape(-1,3).max(0)))
    checks=[];contacts=[]
    for route in scene.features[0]['routes']:
        for a,b in zip(route['points_m'],route['points_m'][1:]):
            a=np.array(a);b=np.array(b)
            for w in np.linspace(0,1,math.ceil(np.linalg.norm((b-a)[[0,2]])/.25)+1):
                p=a+(b-a)*w
                for ox,oz in [(0,0),(.3,0),(-.3,0),(0,.3),(0,-.3),(.21,.21),(-.21,.21),(.21,-.21),(-.21,-.21)]:
                    x,z=p[0]+ox,p[2]+oz
                    for ident,t,lo,hi in meshes:
                        if not(lo[0]<=x<=hi[0] and lo[2]<=z<=hi[2] and hi[1]>p[1]+.08 and lo[1]<p[1]+1.8288):continue
                        hits=vertical_hits(t.reshape(-1,3),np.arange(t.size//3),(x,z,p[1]+.08,p[1]+1.8288))
                        if hits:contacts.append({'route':route['id'],'feet':p.tolist(),'sample_xz':[x,z],'item':ident,'hits':hits})
                checks.append({'route':route['id'],'feet':p.tolist()})
    receipt={'scope':'Nine vertical body rays per route sample at <=.25m spacing using actual source triangles; not a native swept-capsule or gameplay traversal proof.',
        'stature_m':1.8288,'radius_m':.3,'sample_count':len(checks),'vertical_ray_count':len(checks)*9,
        'source_triangle_count':sum(len(t) for _,t,_,_ in meshes),'contacts':contacts,'pass':not contacts,'samples':checks}
    save_json(RECEIPT_ROOT/'household.route-clearance.json',receipt)
    if contacts:raise ValueError(f"{len(contacts)} body-ray contacts: {contacts[:3]}")
    print(f"Source clearance passed: {len(checks)} route samples / {len(checks)*9} body rays")

if __name__=='__main__':
    scene,path=build()
    check_routes(scene)
