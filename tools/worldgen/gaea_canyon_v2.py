#!/usr/bin/env python3
"""Separate populated Canyon v2; v1 and the source Gaea export remain untouched.

Fixed geographic landmarks, repeatable clustered dressing, independently seeded live defence.
Run from the checkout: python tools/worldgen/gaea_canyon_v2.py
"""
import argparse
import json
import math
from pathlib import Path

import numpy as np
from PIL import Image
import canyon_course as cc
import canyon_v2_assets as props
import canyon_winter_surface
import canyon_ground_splatter
from canyon_look_review import apply_look
import gaea_canyon_run as gaea

ROOT=Path(__file__).resolve().parents[2]
DEST=ROOT/'assets/worldgen/workshops/gaea-canyon-run-v2'
VERSION=6
BASE_DRESS=cc.dress
BASE_BUILDINGS=cc.place_buildings
GROUPS=[
 ('hamlet',(.045,.18),(18.,105.),{'wooden_house':.83,'merchant_brick_house':.17},25),
 ('settlement',(.245,.445),(18.,150.),{'wooden_house':.64,'merchant_brick_house':.30,'workshop':.06},64),
 ('saddle',(.46,.55),(22.,105.),{'wooden_house':.65,'workshop':.35},14),
 ('factory',(.57,.735),(18.,150.),{'workshop':.68,'apartment_block':.12,'merchant_brick_house':.20},40),
 ('strongpoint',(.82,.93),(22.,125.),{'merchant_brick_house':.5,'workshop':.3,'wooden_house':.2},30),
]
CLUSTERS=[]
ASSET_RECEIPT={}
EXTRA_ASSETS={
    'pine3':cc.NAT+'pine_tree_n_3.glb', 'bare8':cc.NAT+'tree_8.glb',
    'bare9':cc.NAT+'tree_9.glb', 'wheat':cc.NAT+'wheat_1.glb',
    'yard_fence':cc.MP2+'Modular Props/fence_b_1.glb',
    'fuel_barrel':cc.MP2+'Props/metal_barrel_hr_1.glb',
    'wood_barrel':cc.MP2+'Props/wooden_barrel_1.glb',
    'bucket':cc.MP2+'Props/bucket_mx_1.glb', 'bench':cc.MP2+'Props/bench_mx_1.glb',
    'crate2':cc.MP2+'Props/wooden_crate_2_b.glb', 'jerrycan':cc.MP2+'Props/jerrycan_mx_1.glb',
}


def terrain_materials(edits,H):
    """Preserve both Gaea inputs; pack rock sRGB + linear snow coverage for one GPU sample.

    Alpha 32..223 identifies this documented terrain format; ordinary opaque legacy drapes use
    alpha 255. Coverage is a material mixture, not mesh opacity. Native source files stay intact.
    """
    import shutil
    folder=ROOT/'assets/terrain/canyon_v2';folder.mkdir(parents=True,exist_ok=True)
    rock=np.asarray(Image.open(gaea.SOURCE/'SatMap_Out.png').convert('RGB'))
    snow=np.asarray(Image.open(gaea.SOURCE/'snow.png'),dtype=float)
    if snow.max()>1:snow/=65535. if snow.max()>255 else 255.
    if snow.ndim==3:snow=snow[:,:,0]
    assert rock.shape[:2]==snow.shape and np.isfinite(snow).all()
    rock,snow,vegetation,wear=canyon_winter_surface.author(rock,snow,edits['canyon_course']['route'],edits['building_instances'],heights=H)
    rgba=np.dstack([rock,np.uint8(np.rint(32+191*np.clip(snow,0,1)))])
    Image.fromarray(rgba,'RGBA').save(folder/'rock_snow_material.png')
    Image.fromarray(np.dstack([vegetation,np.zeros_like(vegetation),np.zeros_like(vegetation)]),'RGB').save(folder/'ground_class.png')
    for name in ['SatMap_Out.png','snow.png']:shutil.copyfile(gaea.SOURCE/name,folder/name)
    return dict(texture='assets/terrain/canyon_v2/rock_snow_material.png',
        material_encoding='gaea-rock-srgb-snow-linear-alpha32-223-v1',
        rock_map='assets/terrain/canyon_v2/SatMap_Out.png',snow_map='assets/terrain/canyon_v2/snow.png',
        source_hashes={name:cc.sha256_file(gaea.SOURCE/name) for name in ['SatMap_Out.png','snow.png']},
        authored_wear=wear,ground_class='assets/terrain/canyon_v2/ground_class.png',grass_season='winter')


def place_buildings(site):
    """Retain landmarks, then join their pockets with roadside plots and broken street frontage."""
    out=BASE_BUILDINGS(site)
    for group,fa,fb in [('hamlet',.09,.17),('settlement',.27,.44),('factory',.60,.71),('strongpoint',.845,.93)]:
        stations=max(2,int((fb-fa)*site.rv.L/27.))
        for j,f in enumerate(np.linspace(fa,fb,stations)):
            for side in (-1,1):
                key=f"canyon.{group}.frontage.b{j:02d}.{'left' if side<0 else 'right'}"
                _,_,tx,tz=site.rv.at(float(f))
                yaw=(round(math.degrees(math.atan2(side*tz,-side*tx))/90)*90)%360
                typ='wooden_house' if cc.rnd(key,2)<.82 else 'merchant_brick_house'
                w,d,st,cellar=cc.choose_size(typ,key)
                for setback in (17.,26.,40.):
                    x,z=site.rv.world(float(f),side*(setback+cc.rnd(key,3)*3))
                    x,z=round(x*2)/2,round(z*2)/2
                    rect=cc.footprint(x,z,w,d,yaw)
                    if not site.rect_ok(rect,lane=9.5,wall=6.,gap=5.):continue
                    out.append(dict(id=key,group=group,typology=typ,width_m=w,depth_m=d,storeys=st,cellar=cellar,
                        condition='burned' if typ=='wooden_house' else 'partly_collapsed',
                        x=x,z=z,yaw_deg=yaw,rect=rect,frac=cc.r3(float(f)),side=side,razed=False,
                        seed=cc.uid.fnv1a32(key)^(cc.SEED&0xffffffff)))
                    site.rects.append(rect)
                    break
    out.sort(key=lambda b:b['id'])
    site.buildings=out
    return out


def dress(site,H):
    dr=BASE_DRESS(site,H);rv=site.rv
    route=np.column_stack([rv.x,rv.z]); starts=route[:-1];delta=np.diff(route,axis=0)
    lengths=(delta*delta).sum(1)
    def route_distance(x,z):
        p=np.array([x,z]);t=np.clip(((p-starts)*delta).sum(1)/lengths,0,1)
        return float(np.linalg.norm(p-(starts+delta*t[:,None]),axis=1).min())
    reserved=[]
    def free(x,z,r=1.,lane=12.):
        return (site.on_floor(x,z,2) and route_distance(x,z)>lane+r
                and dr.free(x,z,r+1) and math.hypot(x-site.spawn[0],z-site.spawn[1])>25+r
                and all(math.hypot(x-a,z-b)>r+c+1 for a,b,c in reserved))
    def yaw(f):
        _,_,tx,tz=rv.at(f);return math.degrees(math.atan2(tx,tz))
    def locate(f,side,r,key,band=(20,65)):
        for trial in range(35):
            ff=f+(cc.rnd(key,trial+13)-.5)*.02
            x,z=rv.world(ff,side*(band[0]+cc.rnd(key,trial+90)*(band[1]-band[0])))
            if free(x,z,r):return x,z,ff
        return None
    # Rockfall masses alternate with occupied pockets: separate silhouettes, not uniform confetti.
    for j,f in enumerate(np.linspace(.055,.925,47)):
        side=-1 if j%2 else 1; key=f'v2.rockfall.{j}'
        kind=f'rock_{j%3}';r=[5.3,8.2,3.5][j%3]
        at=locate(float(f),side,r,key,(28,85))
        if not at:continue
        x,z,ff=at;dr.add('large_rock',kind,x,z,yaw(ff)+(j%4)*90,1.,-.55)
        reserved.append((x,z,r))
        for k in range(3):
            a=cc.rnd(key,k+170)*math.tau;xx=x+math.cos(a)*(r+2);zz=z+math.sin(a)*(r+2)
            if free(xx,zz,2):dr.add('talus_rock','rock_2',xx,zz,k*112,.42,-.14)
    # Defensive work sites and abandoned transport columns, at cover breaks along the whole run.
    for j,f in enumerate(np.linspace(.085,.90,26)):
        key=f'v2.post.{j}';at=locate(float(f),1 if j%2 else -1,5.,key,(18,51))
        if not at:continue
        x,z,ff=at;angle=yaw(ff)+180
        dr.add('outpost','outpost',x,z,angle,1.,-.07);reserved.append((x,z,4.5))
        cluster=dict(id=key,kind='abandoned_field_position',fraction=ff,x=x,z=z,bodies=0)
        CLUSTERS.append(cluster)
        dr.add('destroyed_gun','wreck_gun45',x,z+2.8,angle+17,1.,-.05)
        for k in range(6):
            a=cc.rnd(key,k+2)*math.tau;r=5+cc.rnd(key,k+20)*7
            xx,zz=x+math.cos(a)*r,z+math.sin(a)*r
            if free(xx,zz,.6,7):dr.add('field_equipment',['crate','board','charred_plank'][k%3],xx,zz,cc.rnd(key,k+50)*360,1.)
    for j,f in enumerate(np.linspace(.05,.935,43)):
        key=f'v2.convoy.{j}';at=locate(float(f),-1 if j%3 else 1,5.,key,(17,45))
        if not at:continue
        x,z,ff=at;kind=['wreck_kv1','wreck_pz3','wreck_cart','wreck_cart'][j%4]
        dr.add('additional_wreck',kind,x,z,yaw(ff)+cc.rnd(key,111)*80-40,1.,-.10)
        reserved.append((x,z,4))
        CLUSTERS.append(dict(id=key,kind='destroyed_transport',fraction=ff,x=x,z=z,bodies=0))
    # Grounded anonymous combat dead gather around the destroyed positions, with smaller groups
    # near ruined walls. Their counts/poses are authored inference, not a claim about a named event.
    for b in site.buildings:
        if not b['razed'] and cc.rnd(b['id'],840)<.40:
            CLUSTERS.append(dict(id=b['id']+'.aftermath',kind='ruined_position',fraction=b['frac'],x=b['x'],z=b['z'],bodies=0))
    stains=[props.Mesh() for _ in range(12)]
    casualties=[]
    for j,c in enumerate(CLUSTERS):
        target=3+j%4
        for k in range(target*5):
            if c['bodies']>=target:break
            a=cc.rnd(c['id'],k+930)*math.tau;rr=4+12*cc.rnd(c['id'],k+1120)
            x,z=c['x']+math.cos(a)*rr,c['z']+math.sin(a)*rr
            if not site.on_floor(x,z,1) or route_distance(x,z)<6 or not dr.free(x,z,1.1):continue
            if any(math.hypot(x-a,z-b)<2.5 for a,b in casualties):continue
            # No floating figures on steep slopes: source body contact spans about two metres.
            heights=[dr.ground(x+dx,z+dz) for dx,dz in [(-.8,0),(.8,0),(0,-.8),(0,.8)]]
            if max(heights)-min(heights)>.28:continue
            dr.add('combat_dead',f'casualty_{(j+k)%6}',x,z,cc.rnd(c['id'],k+500)*360,1.,-.045)
            casualties.append((x,z));c['bodies']+=1
            # Coordinate seed survives the independent surface-study workflow and relocation
            # of unrelated props. Tiny deposited cubes replace the metre-wide nested stamps.
            bx,bz=cc.r3(x),cc.r3(z)
            rec=canyon_ground_splatter.splatter(stains[min(int(c['fraction']*12),11)],
                dr.ground,bx+.25,bz,canyon_ground_splatter.seed_at(bx,bz))
            dr.counts['blood_drops']=dr.counts.get('blood_drops',0)+rec['drops']
    # Debris and abandoned belongings tie the new architecture to the ground.
    for b in site.buildings:
        for k in range(22):
            a=cc.rnd(b['id'],k+300)*math.tau;rr=max(b['width_m'],b['depth_m'])*.60+1+7*cc.rnd(b['id'],k+370)
            x,z=b['x']+math.cos(a)*rr,b['z']+math.sin(a)*rr
            if free(x,z,.7,9):dr.add('yard_debris',['brick_stack','debris','charred_plank','crate','log','board'][k%6],x,z,k*53,.75+cc.rnd(b['id'],k+380)*.4,-.04)
    # Sheltered copses and broken hedgerows: larger irregular patches, with open route and
    # deliberate sightline gaps between them. No summer fern / palm / ornamental garden assets.
    for j,f in enumerate(np.linspace(.025,.975,86)):
        key=f'v2.copse.{j}';side=1 if j%2 else -1
        x,z=rv.world(float(f),side*(26+cc.rnd(key,2)*100))
        for k in range(19):
            a=cc.rnd(key,k+31)*math.tau;r=math.sqrt(cc.rnd(key,k+59))*23
            xx,zz=x+math.cos(a)*r,z+math.sin(a)*r*.58
            if not free(xx,zz,1.2,13):continue
            if abs(dr.ground(xx+1,zz)-dr.ground(xx-1,zz))>.9:continue
            kind=['bare8','bare9','bare9','bare8','pine2','pine3'][k%6]
            scale=.55+cc.rnd(key,k+71)*.55
            dr.add('copse_foliage',kind,xx,zz,cc.rnd(key,k+101)*360,scale,-.045)
            reserved.append((xx,zz,.75))
        # Persistent dry stalk beds skirt the copse instead of isolated green clumps.
        for k in range(52):
            a=cc.rnd(key,k+210)*math.tau;r=math.sqrt(cc.rnd(key,k+290))*28
            xx,zz=x+math.cos(a)*r,z+math.sin(a)*r*.55
            if free(xx,zz,.18,8):
                dr.add('dry_stalk_bed','wheat',xx,zz,cc.rnd(key,k+371)*360,
                       [.55,.24+cc.rnd(key,k+410)*.28,.55],-.045)
    # Everyday use survives destruction: a wood store at houses, fuel/tool stores at workshops.
    # Objects share a yard orientation and purpose; damaged fence gaps face the road.
    for j,b in enumerate(site.buildings):
        if b['razed']:continue
        a=math.radians(b['yaw_deg']);co,si=math.cos(a),math.sin(a)
        def local(lx,lz):return b['x']+lx*co+lz*si,b['z']-lx*si+lz*co
        back=-b['depth_m']*.5-4.2
        x,z=local(0,back)
        if not free(x,z,1.1,10):continue
        industrial=b['typology'] in ('workshop','apartment_block')
        palette=['fuel_barrel','fuel_barrel','jerrycan','crate2','crate'] if industrial else ['wood_barrel','bucket','bench','crate2','log']
        for k,kind in enumerate(palette):
            xx,zz=local((k-2)*1.15,back-(k%2)*.8)
            if free(xx,zz,.40,9):dr.add('yard_supplies',kind,xx,zz,b['yaw_deg'],1.,-.02)
        if not industrial:
            for k in range(7):
                xx,zz=local(3+(k%3)*.45,back-2-(k//3)*.48)
                if free(xx,zz,.4,9):dr.add('timber_store','log',xx,zz,b['yaw_deg']+90,[.38,.38,.38],-.02)
        # Surviving boundaries and garden trees read as adjoining plots even after roofs vanish.
        for k in range(-3,4):
            if k==0 or cc.rnd(b['id'],k+1600)<.20:continue
            xx,zz=local(k*3.,back-6.)
            if free(xx,zz,1.55,10):dr.add('yard_fence','yard_fence',xx,zz,b['yaw_deg'],[.72,.50,1.],-.03)
        for k in range(7):
            lx=(-1 if k%2 else 1)*(b['width_m']*.5+5.+cc.rnd(b['id'],k+1730)*3)
            xx,zz=local(lx,back+3-k*2.7)
            if free(xx,zz,.9,9):
                dr.add('yard_bare_trees','bare8' if k%2 else 'bare9',xx,zz,cc.rnd(b['id'],k+1790)*360,
                       .65+cc.rnd(b['id'],k+1800)*.45,-.045)
                reserved.append((xx,zz,.7))
    for i,m in enumerate(stains):
        if not m.points:continue
        name=f'blood_drops_v{VERSION}_{i:02}';rec=m.write(ROOT/props.PREFIX/(name+'.glb'))
        ASSET_RECEIPT[name]=rec;cc.ASSETS[name]=rec['path']
        # Each small cuboid rests on the shared heightfield and slightly enters the snow.
        dr.items.append((name,0.,0.,0.,0.,1.))
    dr.counts['blood_splatter_sites']=len(casualties)
    return dr


def build(dest):
    CLUSTERS.clear();ASSET_RECEIPT.clear()
    dest=Path(dest)
    receipt_path=dest/'canyon-course.receipt.json'
    previous=json.loads(receipt_path.read_text()) if receipt_path.exists() else {}
    for name in cc.OUTPUT_FILES:
        p=dest/name
        if p.exists() and previous.get('outputs',{}).get(name)!=cc.sha256_file(p):
            raise ValueError(f'Preserving hand edits in {p}; choose --dest elsewhere')
    v1=ROOT/'assets/worldgen/workshops/gaea-canyon-run'
    original={n:cc.sha256_file(v1/n) for n in cc.OUTPUT_FILES}
    ASSET_RECEIPT.update(props.build())
    ASSET_RECEIPT['resting_log']=props.resting_log(ROOT/cc.ASSETS['log'])
    cc.ASSETS['log']=ASSET_RECEIPT['resting_log']['path']
    cc.ASSETS.update({k:v['path'] for k,v in ASSET_RECEIPT.items()})
    cc.ASSETS.update(EXTRA_ASSETS)
    cc.HARD.update(['outpost','rock_0','rock_1','rock_2','wreck_cart','wreck_gun45'])
    cc.GROUPS=GROUPS;cc.CRATERS=50;cc.RAZED_PLOTS=9
    cc.dress=dress;cc.place_buildings=place_buildings
    manifest,edits,H,receipt=gaea.build()
    manifest['terrain'].update(terrain_materials(edits,H))
    vegetation=np.asarray(Image.open(ROOT/manifest['terrain']['ground_class']))[:,:,0]
    stalks=canyon_winter_surface.restrict_stalks(manifest,vegetation)
    manifest['terrain']['authored_wear'].update(stalks)
    edits['base_placements']=len(manifest['placements'])
    receipt['dressing']['dry_stalk_bed']=stalks['stalks_after']
    manifest['authoring'].update(tool='tools/worldgen/gaea_canyon_v2.py',tool_version=VERSION,variant='canyon-v2')
    edits['canyon_course'].update(variant='canyon-v2',tool='tools/worldgen/gaea_canyon_v2.py')
    # Broad winter overcast, actual ephemeris and cloud occupancy; no blue post-process veil.
    apply_look(edits)
    edits['canyon_v2_population']=dict(seed=cc.SEED,clusters=CLUSTERS,provenance={
        'class':'attested_condition',
        'scope':'Fictional Gaea mechanics course. Counts, local placements, casualty poses and rock formations are authored inference.',
        'buildings':'docs/research/STALINGRAD_URBAN_FABRIC_20260928.md and PHOTO_REFERENCE_ANALYSIS_20260928.md',
        'photos':['Bundesarchiv 146-1992-055-33 (field shelter construction only, December 1941)',
                  'NAC 2-942 (destroyed tank in snowy Stalingrad ruins, January 1943)',
                  'Museum Stalingrad Battle KP 23628/5 NEG 7844 (Banny ravine / factory relationship, winter 1943)'],
        'primary_record':'https://stalingrad1942-1943.ru/node/2970',
        'modern_visual_reference':dict(url='https://www.youtube.com/watch?v=yrrso5JDR5I',
            scope='User-supplied modern footage: winter surface, ruined plot continuity, bare tree silhouette, transient impact contrast only. Not WWII damage or historical event evidence.',
            notes='docs/research/CANYON_FOOTAGE_DIRECTION_20260930.md'),
        'aftermath':'Recurring battlefield dead and abandoned equipment, not a staged massacre or named victims. Existing research URBAN_INTRO_TABLEAUX_20260928.md S12-S14 documents frozen dead.'})
    dest.mkdir(parents=True,exist_ok=True)
    (dest/'manifest.v1.json').write_text(cc.dumps(manifest,indent=None))
    (dest/'manifest_edits.json').write_text(cc.dumps(edits));np.save(dest/'heights.npy',H.astype(np.float32))
    assert original=={n:cc.sha256_file(v1/n) for n in cc.OUTPUT_FILES}
    receipt.update(schema='panzer.canyon-v2',version=VERSION,v1_preserved=original,assets=ASSET_RECEIPT,
                   library_assets={k:dict(path=p,sha256=cc.sha256_file(ROOT/p)) for k,p in EXTRA_ASSETS.items()},
                   clusters=len(CLUSTERS),outputs={n:cc.sha256_file(dest/n) for n in cc.OUTPUT_FILES})
    receipt_path.write_text(cc.dumps(receipt))
    print(json.dumps({k:receipt[k] for k in ['standing_buildings','dressing','clusters','gun_candidates','travel_surface']},indent=2))


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--dest',type=Path,default=DEST)
    build(parser.parse_args().dest)
