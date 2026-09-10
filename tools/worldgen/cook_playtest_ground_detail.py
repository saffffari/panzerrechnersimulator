#!/usr/bin/env python3
"""Immutable Playtest soil/class-map and source-mesh boulder seating cook; CPU only."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import numpy as np
import world_authoring as world
from make_ground_tex import read_png
sys.path.insert(0, str(world.ROOT/'tools'))
from fit_crew_steel_helmet import read_glb, accessor
from validate_tiger_hatches import matrix

SOURCE_TILES='source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted/PSX Textures v3.1/PSX Textures/256/Color Maps/'

def mesh_triangles(path):
    doc,binary,_=read_glb(path); rows=[]
    def walk(i,parent):
        n=doc['nodes'][i]; m=parent@matrix(n)
        if 'mesh' in n:
            for p in doc['meshes'][n['mesh']]['primitives']:
                v=accessor(doc,binary,p['attributes']['POSITION'])@m[:3,:3].T+m[:3,3]
                ids=accessor(doc,binary,p['indices']).reshape(-1,3)
                rows.append(v[ids])
        for c in n.get('children',[]):walk(c,m)
    for i in doc['scenes'][doc.get('scene',0)]['nodes']:walk(i,np.eye(4))
    return np.concatenate(rows)

def transformed(tri,p):
    angle=np.radians(p[4]); c,s=np.cos(angle),np.sin(angle)
    rotation=np.array([[c,0,s],[0,1,0],[-s,0,c]])
    return (tri*np.asarray(p[5]))@rotation.T+np.asarray(p[1:4])

def bottom_samples(tri,n=48):
    """Uniform projected-area samples of the actual lower rock envelope, never its AABB floor."""
    v=tri.reshape(-1,3);lo,hi=v.min(0),v.max(0)
    xx=lo[0]+(np.arange(n)+.5)/n*(hi[0]-lo[0]);zz=lo[2]+(np.arange(n)+.5)/n*(hi[2]-lo[2])
    x,z=np.meshgrid(xx,zz);x=x.ravel();z=z.ravel();lower=np.full(len(x),np.inf)
    for t in tri:
        a=t[0];b=t[1]-a;c=t[2]-a;det=b[0]*c[2]-b[2]*c[0]
        if abs(det)<1e-12:continue
        u=((x-a[0])*c[2]-(z-a[2])*c[0])/det
        q=(b[0]*(z-a[2])-b[2]*(x-a[0]))/det
        hit=(u>=0)&(q>=0)&(u+q<=1)
        y=a[1]+u*b[1]+q*c[1];lower=np.where(hit,np.minimum(lower,y),lower)
    good=np.isfinite(lower)
    return x[good],z[good],lower[good]

def linear(rgb):
    v=rgb.astype(np.float64)/255
    return np.where(v<=.04045,v/12.92,((v+.055)/1.055)**2.4)

def encoded(rgb):
    return np.round(np.clip(np.where(rgb<=.0031308,rgb*12.92,1.055*np.maximum(rgb,0)**(1/2.4)-.055),0,1)*255).astype(np.uint8)

def cook(source,name):
    manifest,heights,edits=world.load_world(source)
    if manifest.get('authoring',{}).get('mode')!='playtest':raise ValueError('requires authored Playtest source')
    if edits.get('scene_blocks'):raise ValueError('scene overlays require explicit baking first')
    world.materialize_edits(manifest,edits)
    before=json.loads(json.dumps(manifest['placements'])); source_hash=world.world_hash(source)
    # The 0.40 m source slab consumed a narrow 2.10 m doorway's headroom, leaving only
    # 1.70 m clear. Use the matching 2.50 m-high open frame without scaling the building.
    opening_swaps=[]
    for i,path in enumerate(manifest['assets']):
        if Path(path).name=='doorway_hr_1.glb':
            replacement=str(Path(path).with_name('doorway_hr_2_wide.glb'))
            world.model_path(replacement)
            opening_swaps.append({'index':i,'from':path,'to':replacement,
                'clear_height_before_m':1.70,'clear_height_after_m':2.10,
                'note':'open frame only; dynamic door leaf is a separate missing mechanic'})
            manifest['assets'][i]=replacement
    spacing=manifest['terrain']['spacing']; extent=spacing*(len(heights)-1);half=extent/2
    cache={};seated=[]
    for identity,p in zip(manifest['placement_ids'],manifest['placements']):
        path=manifest['assets'][p[0]]
        if not Path(path).name.startswith('stone_'):continue
        if path not in cache:cache[path]=mesh_triangles(path)
        tri=transformed(cache[path],p);x,z,y=bottom_samples(tri)
        g=np.array([world.sample_height(heights,spacing,float(a),float(b)) for a,b in zip(x,z)])
        # Seat 45% of the projected lower surface into terrain. This removes balancing on a
        # single lowest corner while preserving the source orientation and visible upper shape.
        delta=min(0.0,float(np.quantile(g-y,.55)))
        old_fraction=float(np.mean(y<=g+.005));p[2]+=delta
        new_fraction=float(np.mean(y+delta<=g+.005))
        if new_fraction<.44:raise ValueError('rock seating failed source surface support target')
        seated.append({'id':identity,'asset':path,'lowered_m':-delta,'support_fraction_before':old_fraction,
                       'support_fraction_after':new_fraction,'area_samples':len(x)})
    # Land-class sampling matches this revision's extent. This mask controls geometry, while
    # a separate smooth albedo drape carries macro material colour and repeated textures add detail.
    n=1024;axis=np.linspace(-half,half,n,dtype=np.float32);x=axis[None,:];z=axis[:,None]
    road=np.full((n,n),1e6,np.float32)
    for x0,z0,x1,z1 in manifest['authoring']['roads']:
        dx,dz=x1-x0,z1-z0;t=np.clip(((x-x0)*dx+(z-z0)*dz)/max(1,dx*dx+dz*dz),0,1)
        road=np.minimum(road,np.hypot(x-x0-t*dx,z-z0-t*dz))
    dirt=np.clip((8-road)/3,0,1);dirt=dirt*dirt*(3-2*dirt)
    grove=np.clip(1-(((x-220)/72)**2+((z-100)/80)**2),0,1)
    bare=np.zeros_like(dirt)
    for p in manifest['placements']:
        path=manifest['assets'][p[0]]
        if 'floor_ceiling' not in path:continue
        if path not in cache:cache[path]=mesh_triangles(path)
        tri=transformed(cache[path],p).reshape(-1,3);lo,hi=tri.min(0),tri.max(0)
        bare=np.maximum(bare,((x>=lo[0]-.5)&(x<=hi[0]+.5)&(z>=lo[2]-.5)&(z<=hi[2]+.5)))
    feature=manifest['authoring'].get('runtime_feature_building')
    if feature:
        marker=next(v for v in manifest['markers'] if v['name']==feature['marker_name'])
        bare=np.maximum(bare,(abs(x-marker['x'])<=4.75)&(abs(z-marker['z'])<=4.75))
    dirt=np.maximum(dirt,bare)
    pasture=(1-grove)*(1-dirt);woods=grove*(1-dirt)
    mask=np.round(np.stack([pasture,woods,np.zeros_like(pasture)],axis=-1)*255).astype(np.uint8)
    names=['grass_7.png','dirt_5.png','dirt_2.png'];means=[];sources=[]
    for tile in names:
        path=world.repo_path(SOURCE_TILES+tile);rgb=read_png(str(path))
        means.append(linear(rgb).mean(axis=(0,1)))
        sources.append({'path':SOURCE_TILES+tile,'sha256':world.file_hash(path),'linear_mean_rgb':means[-1].tolist()})
    grass,soil,litter=means
    rgb=encoded(grass*pasture[...,None]+litter*woods[...,None]+soil*dirt[...,None])
    key=hashlib.sha256(rgb.tobytes()+mask.tobytes()).hexdigest()[:16]
    output=world.repo_path(f'assets/worldgen/derived/{key}',exists=False);output.mkdir(parents=True,exist_ok=True)
    for filename,image in [('playtest_ground_macro.png',rgb),('playtest_ground_class.png',mask)]:
        path=output/filename
        if not path.exists():world.write_png(path,image)
        elif not np.array_equal(read_png(str(path)),image):raise ValueError('derived content collision')
    manifest['terrain']['texture']=(output/'playtest_ground_macro.png').relative_to(world.ROOT).as_posix()
    manifest['terrain']['ground_class']=(output/'playtest_ground_class.png').relative_to(world.ROOT).as_posix()
    manifest['authoring']['ground_detail']={'recipe':'source-pizza-macro-and-repeated-detail-v1','sources':sources,
       'mask_channels':'R pasture, G woodland, B crop; roads and slab footprints bare','world_size_m':extent,
       'macro':'linear-light source material means; no baked large pixel hash','rock_seating':'45 percent projected lower-envelope terrain intersection; visual burial, not solved rigid-body equilibrium'}
    assert len(before)==len(manifest['placements'])
    for old,new in zip(before,manifest['placements']):
        assert old[:2]==new[:2] and old[3:]==new[3:], 'only rock Y may change'
    recipe={'mode':'playtest-ground-detail','version':1,'source_world_hash':source_hash,'rock_seating':seated,
       'source_materials':sources,'opening_swaps':opening_swaps,'mask_extent_m':extent,'macro_texel_m':extent/n,
       'preserved':'all placement IDs, XZ, yaw, scale, source GLBs, heightfield, markers and scene provenance'}
    return world.publish(name,manifest,heights,edits,source={'manifest':source,'hash':source_hash},recipe=recipe)

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--source',required=True)
    p.add_argument('--name',default='playtest-ground-detail');p.add_argument('--receipt',required=True);a=p.parse_args()
    out=world.repo_path(a.receipt,exists=False)
    if out.exists():raise ValueError('preserving existing receipt')
    result=cook(a.source,a.name);out.parent.mkdir(parents=True,exist_ok=True);out.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'manifest':result['manifest'],'rocks':len(result['recipe']['rock_seating']),'receipt':str(out)}))
