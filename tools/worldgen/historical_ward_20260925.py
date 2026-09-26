#!/usr/bin/env python3
"""Reusable factory ward/independent doctor cellar; architecture is explicitly inferred.

Only new ward scene/receipt files are written. Existing Pizza/source GLBs, the winter map,
and the running editor are untouched. Run from this repository with the bundled Python.
"""
from __future__ import annotations

import json
import math
import random
import struct
from pathlib import Path
import numpy as np
from historical_scene_common_20260925 import Scene, RECEIPT_ROOT, save_json, resolve, node_matrix, sha

FLOOR = -2.8


def audit_routes(scene, routes):
    """Sample moving standing-body rays against exact transformed source triangles.

    This deliberately does not claim an engine capsule/terrain/playtest result. It catches
    accidentally closed door leaves, walls, beams and dressing crossing the authored routes.
    """
    cache={}
    def triangles(raw):
        if raw in cache: return cache[raw]
        data=resolve(raw).read_bytes(); size=struct.unpack_from('<I',data,12)[0]
        doc=json.loads(data[20:20+size]); base=28+size; out=[]
        def accessor(index):
            a=doc['accessors'][index]; v=doc['bufferViews'][a['bufferView']]
            n={'SCALAR':1,'VEC3':3}[a['type']]
            dtype={5121:'u1',5123:'<u2',5125:'<u4',5126:'<f4'}[a['componentType']]
            width=np.dtype(dtype).itemsize
            return np.ndarray((a['count'],n),dtype=dtype,buffer=data,
                offset=base+v.get('byteOffset',0)+a.get('byteOffset',0),
                strides=(v.get('byteStride',width*n),width))
        def visit(index,parent):
            node=doc['nodes'][index]; model=parent@node_matrix(node)
            if 'mesh' in node:
                for part in doc['meshes'][node['mesh']]['primitives']:
                    if part.get('mode',4)!=4: continue
                    p=accessor(part['attributes']['POSITION'])
                    p=p@model[:3,:3].T+model[:3,3]
                    indices=accessor(part['indices']).reshape(-1) if 'indices' in part else np.arange(len(p))
                    out.append(p[indices.reshape(-1,3)])
            for child in node.get('children',[]): visit(child,model)
        for index in doc['scenes'][doc.get('scene',0)]['nodes']: visit(index,np.eye(4))
        cache[raw]=np.concatenate(out); return cache[raw]
    objects=[]
    for item in scene.items:
        a=math.radians(item['deg']); c,sn=math.cos(a),math.sin(a)
        rotation=np.array([[c,0,sn],[0,1,0],[-sn,0,c]])
        t=(triangles(item['asset'])*item['s'])@rotation.T+[item['dx'],item['dy'],item['dz']]
        objects.append((item['id'],t,t.min((0,1)),t.max((0,1))))
    def intersects(a,b,t):
        delta=b-a; e1=t[:,1]-t[:,0]; e2=t[:,2]-t[:,0]
        p=np.cross(delta,e2); det=np.einsum('ij,ij->i',e1,p); valid=np.abs(det)>1e-9
        if not valid.any(): return False
        e1,e2,p=e1[valid],e2[valid],p[valid]; inverse=1/det[valid]
        tv=a-t[valid,0]; u=np.einsum('ij,ij->i',tv,p)*inverse
        q=np.cross(tv,e1); v=q@delta*inverse; f=np.einsum('ij,ij->i',e2,q)*inverse
        return bool(((u>=-1e-7)&(v>=-1e-7)&(u+v<=1.0000001)&(f>=1e-5)&(f<=1)).any())
    hits=[]; count=0
    for route in routes:
        for first,last in zip(route['points_m'],route['points_m'][1:]):
            first,last=np.asarray(first),np.asarray(last)
            steps=math.ceil(np.linalg.norm(last-first)/.25)
            for step in range(steps):
                a=first+(last-first)*step/steps; b=first+(last-first)*(step+1)/steps
                for x,z in [(0,0),(.28,0),(-.28,0),(0,.28),(0,-.28)]:
                    for height in [.12,.8,1.7]:
                        aa=a+[x,height,z]; bb=b+[x,height,z]; count+=1
                        for ident,t,lo,hi in objects:
                            if ((np.maximum(aa,bb)>=lo)&(np.minimum(aa,bb)<=hi)).all() and intersects(aa,bb,t):
                                hits.append({'route':route['id'],'item':ident,'ray_start_m':aa.tolist()})
    assert not hits, hits[:8]
    return {'sampled_body_rays':count,'blocking_source_triangle_hits':len(hits),
        'step_m':.25,'body_radius_m':.28,'sample_heights_m':[.12,.8,1.7],
        'method':'Exact active GLB scene triangles, instance transforms, five radial offsets and three heights per movement segment',
        'limits':'Discrete standing-body rays, not a swept capsule, source physics engine, terrain acceptance or native playtest'}


def build():
    s = Scene('a1_factory_cellar_ward', 'ward', half=32, spacing=.5)
    rng = random.Random(25224)

    def terrain(x, z):
        h = np.zeros_like(x)
        ward = (x >= -18.25) & (x <= -1.75) & (z >= -4.25) & (z <= 8.25)
        doctor = (x >= 7.75) & (x <= 16.25) & (z >= -.25) & (z <= 8.25)
        h = np.where(ward | doctor, FLOOR - .15, h)
        for cx, start, end in [(-8, -14, -4), (10, -10, 0)]:
            ramp = (np.abs(x-cx) <= 2.0) & (z >= start) & (z < end)
            h = np.where(ramp, FLOOR * (z-start)/(end-start), h)
        return h
    s.set_terrain(terrain)

    def on(key, x, z, floor=FLOOR, **kw):
        scale = kw.get('scale', 1)
        sy = scale if np.isscalar(scale) else scale[1]
        lo, _ = s.bank.bounds(key)
        return s.put(key, x, z, y=floor-float(lo[1])*sy, **kw)

    # Two independent cuts, each with an actual open doorway and a broad descending approach.
    # No wall or door leaf is placed across the body route.
    for zone, xs, zs in [
        ('ward', [-16, -12, -8, -4], [-2, 2, 6]),
        ('doctor', [10, 14], [2, 6]),
    ]:
        for x in xs:
            for z in zs:
                s.put('floor_ceiling_hr_5', x, z, FLOOR,
                    role=f'{zone}: exposed cellar slab, top exactly at walking datum')
                ceiling = 'floor_ceiling_hr_2_hole' if (zone, x, z) == ('ward', -16, 6) else 'floor_ceiling_hr_1'
                s.put(ceiling, x, z, .2,
                    role=f'{zone}: overhead surviving slab; three metres of clear cellar height')
    def open_threshold(x,z,zone):
        # Ordinary registered masonry modules preserve both visible and runtime-solid jambs.
        # Other Pizza doorway variants are open visually but lack a runtime collision profile.
        for side in [-1,1]:
            on('wall_hr_1',x+side*1.45,z,scale=[.275,1,1],
                role=f'{zone}: solid masonry jamb around a clear 1.8m cellar opening')
        on('wall_hr_1',x,z,FLOOR+2.35,scale=[.45,.65/3,1],
            role=f'{zone}: masonry lintel, 2.35m above standing route')
    for x in [-16, -12, -8, -4]:
        if x == -8: open_threshold(x,-4,'ward')
        else: on('wall_hr_1', x, -4,role='ward: load-bearing cellar masonry')
        on('wall_hr_1', x, 8, role='ward: rear retaining masonry')
    for x in [-18, -2]:
        for z in [-2, 2, 6]:
            on('wall_hr_1', x, z, yaw=90, role='ward: retained cellar side wall')
    for x in [10, 14]:
        if x == 10: open_threshold(x,0,'doctor')
        else: on('wall_hr_1',x,0,role='doctor: separate ruined-house cellar wall')
        on('wall_hr_1', x, 8, role='doctor: rear cellar retaining masonry')
    for x in [8, 16]:
        for z in [2, 6]:
            on('wall_hr_1', x, z, yaw=90, role='doctor: separate cellar enclosure')

    # Low retaining returns stay outside the 4m-wide ramps and follow their actual grade.
    for cx, start in [(-8, -14), (10, -10)]:
        for zoff in [2, 6, 9]:
            z = start + zoff
            for side in [-1, 1]:
                s.put('wall_rx_1_1_3_height', cx + side*2.25, z,
                    y=FLOOR*zoff/10-.12, yaw=90,
                    scale=[.8 if zoff < 9 else .4, .65, 1],
                    role='inferred cellar access retaining return; clear of standing route')

    # Factory silhouette: repeated masonry window bays and steel structure above the ward.
    # It is an industrial district cue, never a reconstruction of an identified factory facade.
    for x in [-16, -12, -8, -4]:
        on('window_hr_1', x, 8, .6, role='factory: surviving rear industrial window bay')
        if x != -4:
            on('window_hr_1', x, 8, 3.6, role='factory: upper industrial silhouette visible from tank approach')
        on('wall_rx_1_1_3_height', x, -4, .6,
            scale=[1, .35 if x == -8 else .8, 1],
            role='factory: broken front parapet above cellar; no path obstruction')
    for z in [-2, 2, 6]:
        on('window_hr_1', -18, z, .6, yaw=90, role='factory: west window rhythm')
        on('window_hr_1', -18, z, 3.6, yaw=90, role='factory: surviving tall west elevation')
        on('wall_rx_1_1_3_height', -2, z, .6, yaw=90,
            scale=[1, .6 if z == -2 else 1, 1], role='factory: collapsed east elevation residue')
    for x in [-17.3, -13.3, -5.1, -2.7]:
        for z in [-3.3, 7.3]:
            on('beam_hc_vertical_1', x, z, .6, scale=[.65, 1, .65],
                role='factory: steel frame seated on the surviving slab')
    for z in [-3.3, 3.2, 7.3]:
        for x in [-14, -6]:
            on('beam_hc_horizonatal_1', x, z, 3.6,
                role='factory: surviving eight-metre horizontal structural member')
    for z in [0, 6]:
        on('beam_hc_horizonatal_1', -17.4, z, 6.55, yaw=90,
            scale=[.75, 1, 1], role='factory: high exposed roof edge beam')
    s.put('warehouse_hs_1_roof_1', -14, 2, 6.6,
        role='factory: one surviving industrial roof bay; surrounding roof is absent')
    for x, z in [(-17.1, -.3), (-3.1, -.3), (-17.1, 4.0), (-3.1, 4.0)]:
        on('pillar_hr_2', x, z, scale=[.7, 1, .7],
            role='ward: narrow slab support deliberately outside the central circulation spine')
    for x in [-16.8, -3.2]:
        for z in [-1, 1, 3, 5]:
            on('pipes_hr_1_horizontal_middle_1', x, z, -.15, yaw=90,
                role='ward: disused high service pipe establishes the industrial shell')
        on('pipes_hr_1_vertical_end_1', x, 7, FLOOR,
            role='ward: stopped service pipe; no working machinery implied')

    # Wholly ruined house over the separate doctor position: no second industrial ward facade.
    for x, z, yaw, height in [(10,8,0,.72),(14,8,0,.44),(8,2,90,.55),(8,6,90,.32),
                                (16,2,90,.48),(16,6,90,.85),(14,0,0,.38)]:
        on('wall_rx_1_1_3_height', x, z, .6, yaw=yaw, scale=[1,height,1],
            role='doctor: broken house shell above its independent cellar')
    for x,z in [(8,8),(16,8),(16,0)]:
        on('pillar_hr_1_broken', x,z,.6,scale=.8,role='doctor: masonry remnant of wholly ruined upper house')

    # Bedding is a material inference, not a census. Narrow lateral gaps and a wider shared
    # spine suggest pressure on the floor without creating a mattress obstacle course.
    beds=[]
    for x in [-16.0,-14.25,-5.65,-3.8]:
        for z in [-2.25,.7,3.65,6.5]: beds.append((x,z))
    beds += [(-11.05,1.1),(-11.05,4.1)]
    for n,(x,z) in enumerate(beds):
        yaw=90 + [-3,1,0,2][n%4]
        for dx in [-.40,.40]:
            on('wooden_board_1', x+dx,z+.75,FLOOR,yaw=0,scale=[1.6,1,1.0],
                role='ward: salvaged timber beneath bedding; construction and count inferred')
        on('old_mattress_mx_1' if n%3 else 'old_mattress_mp_1',x,z,FLOOR+.035,
            yaw=yaw,scale=[.90,.9,.90],role='ward: empty improvised bedding; no patient or casualty identity asserted')

    # One service vessel near the threshold, apart from a cold floor-fire site. The source
    # supports carried food and a floor fire; exact vessel form and abandoned/cold state are inferred.
    on('bucket_mx_1',-11.7,-2.8,FLOOR,role='ward: single ration-carrying vessel, form inferred from attested food delivery',
        evidence='attested_function_inferred_object')
    on('assets/authoring/battlefield/scorched_earth.glb',-11.8,6.6,FLOOR+.008,
        scale=[.72,.3,.65],role='ward: cold ash/scorch residue of a floor fire; cold aftermath interpretation',
        evidence='attested_floor_fire_inferred_cold_aftermath')
    for i in range(11):
        a=2*math.pi*i/12
        on('brick_mx_1',-11.8+math.cos(a)*.77,6.6+math.sin(a)*.67,FLOOR,
            yaw=-math.degrees(a),role='ward: loose brick edge around cold hearth; construction inferred')
    for i in range(8):
        on('wooden_plank_1',-12.8+(i%2)*.20,5.8+(i//2)*.14,FLOOR+.035*(i%2),
            yaw=14+i*7,scale=[.55,1,.8],role='ward: small stripped timber pieces beside the spent heat source')

    # Independent doctor room: domestic surfaces repurposed for care, no clinical equipment,
    # tools, occupied beds, named doctors or unsupported treatment scenarios.
    for x,z in [(14,2.0),(14,5.4)]:
        for dx in [-.4,.4]:
            on('wooden_board_1',x+dx,z+.75,FLOOR,scale=[1.6,1,1],
                role='doctor: improvised empty resting surface support')
        on('old_mattress_mx_1',x,z,FLOOR+.035,yaw=90,scale=.90,
            role='doctor: empty bedding in the separate care location; exact furnishing inferred')
    on('table_large_2_1',9.35,6.3,FLOOR,yaw=90,
        role='doctor: borrowed domestic work surface; no modern medical apparatus')
    on('chair_mp_1',10.65,6.2,FLOOR,yaw=-90,scale=1.3,
        role='doctor: single empty chair at the work surface; no individual presence asserted')
    on('plate_mp_1',9.25,6.3,FLOOR+.96,role='doctor: plain vessel on work surface, contents unspecified')
    on('glass_bottle_mx_1',9.42,5.8,FLOOR+.96,scale=.85,
        role='doctor: reused glass vessel; no drug, sterilization or treatment claim')
    on('wooden_crate_1',8.95,2.4,FLOOR,scale=.55,
        role='doctor: unmarked personal/store container, contents unspecified')
    on('shelf_mp_2',15.1,7.55,FLOOR,role='doctor: sparse domestic shelf against surviving cellar wall')

    # The few containers occupy edges and recesses, never the two cellar door mouths.
    for x,z,yaw in [(-17,-3,4),(-17,7.15,-6),(-12.8,-2.8,10),(-2.85,7.25,90)]:
        on('wooden_crate_1',x,z,FLOOR,yaw=yaw,scale=.46,
            role='ward: unmarked edge storage, kept out of floor circulation')
    for x,z in [(-16.7,5.2),(-14.6,-.9),(-5.3,2.2),(-3.6,5.1)]:
        on('wooden_plank_1',x,z,FLOOR,yaw=22,scale=[.65,1,1],
            role='ward: isolated displaced timber at bedding edge')

    # Exterior collapse fans read as the material missing from the walls above them. Deliberate
    # clusters leave both ramps and the exposed inter-cellar route clear.
    rubble=[(-20,-3),(-20,3),(-20,8),(-16,10),(-11,10),(-5,10),(.2,4),(.2,8),
            (6.1,5),(9.3,10.3),(14.4,10.1),(18,6),(18,1),(16.4,-2.5)]
    for cluster,(cx,cz) in enumerate(rubble):
        on('debris_bricks_mx_1',cx,cz,0,yaw=rng.uniform(-180,180),
            scale=[.7+rng.random()*.35,1,.7+rng.random()*.35],
            role='exterior: masonry collapse fan aligned with a damaged elevation')
        for n in range(5):
            on('brick_mx_1',cx+rng.uniform(-1,1),cz+rng.uniform(-.9,.9),0,
                yaw=rng.uniform(-180,180),scale=.85+rng.random()*.3,
                role=f'exterior: individual fragment in collapse fan {cluster+1}; spatial detail inferred')
    for x,z,yaw in [(-20,0,35),(-17,10,15),(-12,10.5,-20),(-.1,6,72),
                         (17.9,4,82),(13,10.5,-32),(6,6,76)]:
        on('beam_1',x,z,0,yaw=yaw,scale=[.7,.75,.75],
            role='exterior: fallen structural timber beside its parent shell')
    for cx,cz in [(-20,5),(-14,10.8),(-.6,9.8),(17.8,7.5)]:
        for n in range(7):
            on('wooden_plank_1',cx+rng.uniform(-.8,.8),cz+rng.uniform(-.65,.65),0,
                yaw=rng.uniform(-180,180),role='exterior: broken timber fan, clear of the exposed approach')
    for x,z in [(-20,-7),(-18,-9),(18,-5)]:
        on('metal_barrel_hr_1',x,z,0,scale=.9,role='industrial district: empty unmarked service vessel')

    s.socket('ward_threshold','foot_entry',[-8,FLOOR,-3.6],0)
    s.socket('ward_spine','foot_route',[-8,FLOOR,2],0)
    s.socket('ration_and_cold_hearth','story',[-11.7,FLOOR+.2,5.8],0)
    s.socket('doctor_separate_cellar','story',[11,FLOOR,4],0)
    s.socket('exposed_route_turn','foot_route',[10,0,-14],0)
    s.connectors += [
        {'id':'factory_district_approach','kind':'foot','position_m':[-8,0,-18],'heading_deg':0},
        {'id':'exposed_cellar_route','kind':'foot','position_m':[10,0,-14],'heading_deg':0},
    ]
    s.view('industrial_approach',[-8,1.65,-18],[-8,2.5,2])
    s.view('ward_first_person',[-8,FLOOR+1.65,-2.7],[-11.5,FLOOR+.6,5.4])
    s.view('doctor_first_person',[10.2,FLOOR+1.65,1.1],[13.3,FLOOR+.65,5.5])
    routes=[
        {'id':'ward_entry','width_m':1.2,'points_m':[[-8,0,-18],[-8,0,-14],[-8,FLOOR,-4],[-8,FLOOR,6.1]]},
        {'id':'separate_doctor_route','width_m':1.2,'points_m':[[-8,FLOOR,-2.5],[-8,FLOOR,-4],[-8,0,-14],[10,0,-14],[10,0,-10],[10,FLOOR,0],[10,FLOOR,4]]},
    ]
    s.features += [
        {'id':'main_ward','bounds_xz_m':[-18,-4,-2,8],'floor_m':FLOOR,'clear_height_m':3.0,
         'empty_bedding_objects':len(beds),'historical_count':False},
        {'id':'doctor_cellar','bounds_xz_m':[8,0,16,8],'floor_m':FLOOR,'separate_building':True,
         'layout_distance_and_furnishing':'inferred; source only establishes another cellar after a search'},
        {'id':'circulation','routes':routes,'status':'authored clear routes; runtime traversal review required'},
        {'id':'cold_floor_fire','position_m':[-11.8,FLOOR,6.6],
         'state':'unlit inferred aftermath of attested floor fire; no fire emitter'},
    ]
    s.notes += [
        'KP 25224 describes a factory cellar, darkness, a fire built on its floor, food carried in, and a doctor in the cellar of another ruined house.',
        'The 50m Volga distance in the diary belongs to a later trench position. It is not used to locate either cellar.',
        'Bedding material/count, one ration vessel, architectural dimensions, cold hearth state, fixtures and route length are production inferences; the source does not give this plan.',
        'No patients, bodies, named identities, readable labels, modern medical equipment, or exact casualty totals are represented.',
        'Both cellar approaches are separate 4m terrain ramps. Main ward has a protected central spine; doctor room has its own open threshold.',
        'Each clear 1.8m by 2.35m threshold is assembled from collision-registered ordinary masonry jambs/lintel; no closed door leaf or unregistered doorway shell is used.',
        'Primary shells, framing, bedding and fragments are existing Pizza assets. One existing scorched-earth asset supplies cold fire residue. All original asset bytes are unchanged.',
        'Placement remains restricted to the Stalingrad factory/Volga winter district; this is not a domestic ward transplant or an exact named factory reconstruction.',
    ]
    assert 200 <= len(s.items) <= 450, len(s.items)
    route_audit=audit_routes(s,routes)
    path=s.save()
    save_json(RECEIPT_ROOT/'ward.routes.json',{'scene':str(path.relative_to(path.parents[4])),
        'scene_sha256':sha(path),'generator_sha256':sha(Path(__file__)),
        'body_height_m':1.7,'body_radius_m':.28,'routes':routes,'static_source_geometry_audit':route_audit,
        'evidence_boundary':'Measured source-bound assembly and sampled source-triangle clearance; not native/GPU collision acceptance'})
    return s


if __name__ == '__main__':
    build()
