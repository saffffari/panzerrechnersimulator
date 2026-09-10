#!/usr/bin/env python3
"""CPU source-mesh proof for clear door openings and standing scale; no renderer involved."""
import json
from pathlib import Path
import sys
import numpy as np
import world_authoring as world
from cook_playtest_ground_detail import mesh_triangles,transformed
from validate_tiger_hatches import vertical_hits

SOURCE='assets/worldgen/revisions/playtest-functional-house/20260905T130318379870Z_39d90b32/manifest.v1.json'
NEW='assets/worldgen/revisions/playtest-ground-detail/20260905T151851623908Z_be1c0a5f/manifest.v1.json'
HEIGHT=1.8288; EYE=1.7

def hits(tri,x,z,low=-10,high=20):
    return vertical_hits(tri.reshape(-1,3),np.arange(tri.size//3), (x,z,low,high))

def inspect():
    old,oh,oe=world.load_world(SOURCE);new,nh,ne=world.load_world(NEW)
    assert np.array_equal(oh,nh) and old['placement_ids']==new['placement_ids']
    assert old['markers']==new['markers'] and len(old['placements'])==len(new['placements'])==619
    root=Path(old['assets'][0]).parent
    slab=mesh_triangles(str(root/'floor_ceiling_hr_1.glb'))
    narrow=mesh_triangles(str(root/'doorway_hr_1.glb'))
    wide=mesh_triangles(str(root/'doorway_hr_2_wide.glb'))
    slab_top=max(hits(slab,1,1))
    narrow_header=min(y for y in hits(narrow,0,0) if y>1)
    wide_header=min(y for y in hits(wide,0,0) if y>1)
    assert narrow_header-slab_top < HEIGHT
    assert wide_header-slab_top > HEIGHT+.25
    changed=[i for i,(a,b) in enumerate(zip(old['assets'],new['assets'])) if a!=b]
    assert len(changed)==1 and old['assets'][changed[0]].endswith('/doorway_hr_1.glb')
    assert new['assets'][changed[0]].endswith('/doorway_hr_2_wide.glb')
    rows=[];cache={}
    for zone in new['authoring']['zones']:
        if zone['id']=='functional_house':continue
        cx,cy,cz=zone['center_m'];position=(cx+1.0,cz+1.0);crossings=[]
        for p in new['placements']:
            path=new['assets'][p[0]]
            if not any(t in path for t in ('floor_ceiling','roof_')):continue
            if path not in cache:cache[path]=mesh_triangles(path)
            t=transformed(cache[path],p)
            if not (t[:,:,0].min()<=position[0]<=t[:,:,0].max() and t[:,:,2].min()<=position[1]<=t[:,:,2].max()):continue
            for y in hits(t,*position,cy-1,cy+8):crossings.append((y,path))
        floors=[v for v,p in crossings if 'floor_ceiling' in p]
        roofs=[v for v,p in crossings if 'roof_' in p]
        assert floors and roofs
        clearance=min(roofs)-max(floors)
        assert clearance>HEIGHT+.5,(zone['id'],clearance)
        rows.append({'zone':zone['id'],'sample_xz_m':position,'floor_top_m':max(floors),'ceiling_underside_m':min(roofs),'clearance_m':clearance})
    # Source cellar is an open ruin. The film's forced crouch is not a headroom measurement.
    scene=json.load(open(world.ROOT/'assets/worldgen/scene_blocks/astra_20260905/a1_burned_cellar_household.scene-block.v1.json'))
    terrain=np.array(scene['terrain']['heights_m'],dtype=np.float32).reshape(scene['terrain']['res'],scene['terrain']['res'])
    scene_tri=[]
    for item in scene['items']:
        t=mesh_triangles(item['asset']);p=[0,item['dx'],item['dy'],item['dz'],item['deg'],item['s']]
        scene_tri.append(transformed(t,p))
    scene_tri=np.concatenate(scene_tri);route=[]
    for z in np.linspace(-10,6,33):
        y=world.sample_height(terrain,scene['terrain']['spacing_m'],0,float(z))
        blocks=hits(scene_tri,0,float(z),y+.05,y+HEIGHT)
        assert not blocks,('cellar centre standing route obstructed',z,blocks)
        route.append({'x':0,'z':float(z),'feet_y':y,'head_y':y+HEIGHT,'body_axis_hits':len(blocks)})
    return {'schema':'kessel.playtest-metric-clearance','standing_stature_m':HEIGHT,'eye_height_m':EYE,
      'eye_to_crown_m':HEIGHT-EYE,'source_world':SOURCE,'new_world':NEW,'preserved_placements':619,
      'source_slab_top_m':slab_top,'old_lintel_m':narrow_header,'new_lintel_m':wide_header,
      'old_door_clearance_m':narrow_header-slab_top,'new_door_clearance_m':wide_header-slab_top,
      'doorway_placements_swapped':sum(p[0] in changed for p in new['placements']),
      'openings_scope':'open frame geometry only; operable door leaves remain a missing mechanic',
      'roof_samples':rows,'functional_house_storey_clearance_m':3.0-slab_top,
      'cellar_route':route,'cellar_scope':'33 centre-axis samples clear for standing; no intact cellar ceiling in this draft; not a swept-capsule or runtime traversal proof'}

if __name__=='__main__':
    report=inspect();output=world.ROOT/'artifacts/astra-audit-20260905/playtest-metric-clearance.json'
    output.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k not in ('cellar_route',)}))
