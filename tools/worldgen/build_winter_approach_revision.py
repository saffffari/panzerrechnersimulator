#!/usr/bin/env python3
"""Build an additive, owner-private Act I winter approach scene from preserved game assets.

CPU-only. No source mesh, canonical manifest, route, terrain, or runtime code is changed.
Run from shader-graphics: python tools/worldgen/build_winter_approach_revision.py [--check]
"""
from __future__ import annotations
import argparse, copy, hashlib, json, math, struct
from collections import Counter
from pathlib import Path
import numpy as np

NAT = 'assets/pizza/_extracted/PSX Nature v1.7.3/PSX Nature (Tree branches separated)/Models/GLB (recommended)/'
MEGA = 'assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/'
REVISION = 'assets/worldgen/revisions/act1-winter-approach/20260913-v1'
SOURCE = 'assets/worldgen/act1/manifest.v1.json'
ASSETS = {
 'snag_tall': NAT+'tree_8.glb', 'snag_short': NAT+'tree_9.glb',
 'stump': NAT+'tree_stump_1.glb', 'log': NAT+'tree_log_1.glb',
 'stone2': NAT+'stone_2.glb', 'stone3': NAT+'stone_3.glb',
 'fence': MEGA+'Modular Props/fence_b_1.glb',
 'wall': MEGA+'Modular Structures/wall_hr_1_hole_1.glb',
 'door': MEGA+'Modular Structures/doorway_hr_1.glb',
 'pillar': MEGA+'Modular Structures/pillar_hr_1_broken.glb',
 'rubble': MEGA+'Debris & Misc/debris_bricks_mx_1.glb',
 'bricks': MEGA+'Debris & Misc/bricks_stacked_mx_1.glb',
 'crate': MEGA+'Props/wooden_crate_2_a.glb',
 'barrel': MEGA+'Props/metal_barrel_hr_1.glb',
}
# Authored groups, not stochastic world-wide scattering. X/Z in existing Act I metres.
# Each item: mesh alias, x, z, yaw degrees, scale; scales may be non-uniform.
GROUPS = {
 'near_snags': [
  ('snag_tall',1887,207,32,1.18), ('snag_short',1896,215,147,1.25),
  ('snag_short',1880,213,273,.84), ('stump',1887,216,47,1.12),
  ('log',1891,210,128,1.12), ('log',1892,212,137,.80),
  ('stone3',1881,211,227,.67), ('stone2',1898,218,56,.55),
 ],
 'broken_boundary_cache': [
  ('fence',1916,231,12,[1.0,.86,1]), ('fence',1920.3,230.1,21,[.85,.64,1]),
  ('fence',1927,228,31,[1.05,.95,1]), ('fence',1931.1,225.4,43,[.85,.71,1]),
  ('crate',1923,234,15,2.0), ('crate',1925.1,235,112,1.3),
  ('barrel',1920.4,235.4,31,1.0), ('barrel',1921.4,236.1,132,.85),
  ('log',1932.5,228.5,53,.75), ('stump',1936,223,242,1.25),
  ('snag_tall',1938,220,208,.99), ('snag_short',1942,225,28,1.1),
  ('rubble',1915,233,65,1.1), ('rubble',1929,231,31,.85),
 ],
 'ruined_field_shelter': [
  ('wall',1954,178,12,[1.35,1.05,1.1]),
  ('wall',1960,174,102,[1.1,.68,1.0]),
  ('door',1948.3,177,-5,[1.05,.92,1]),
  ('pillar',1945.5,177,13,[1.25,1.35,1.25]),
  ('pillar',1961.5,169.5,97,[1.1,.8,1.1]),
  ('rubble',1952,182,30,2.4), ('rubble',1957,180,130,1.5),
  ('rubble',1960,170,9,1.9), ('bricks',1948,180,71,2),
  ('crate',1957,175,21,1.6), ('barrel',1959.5,171,14,.94),
  ('snag_tall',1965,165,43,1.27), ('snag_short',1972,169,211,1.32),
  ('snag_tall',1956,159,129,.89), ('stump',1966,172,38,1.35),
  ('log',1966,168,216,1.2), ('stone3',1970,172,245,.85),
 ],
 'east_field_edge': [
  ('snag_short',1901,282,63,1.2), ('snag_tall',1908,288,254,.98),
  ('snag_short',1916,281,127,.82), ('log',1911,283,18,1.1),
  ('stump',1916,287,52,1.45), ('fence',1918,275,72,[.85,.73,1]),
  ('fence',1919,270.7,84,[.72,.90,1]), ('stone2',1902,286,112,.85),
 ],
 'west_exit_remnant': [
  ('snag_tall',1798,226,116,.90), ('snag_short',1806,220,222,1.15),
  ('stump',1801,229,14,1.3), ('log',1799,223,21,.92),
  ('fence',1792,228,20,[1.0,.82,1]), ('fence',1786.9,227,5,[.7,.62,1]),
  ('rubble',1791,231,5,1.3), ('stone2',1804,226,27,.67),
 ],
}

def sha(data): return hashlib.sha256(data).hexdigest()
def json_bytes(value): return (json.dumps(value, indent=2, ensure_ascii=False)+'\n').encode()

def node_matrix(node):
 if 'matrix' in node: return np.array(node['matrix'],dtype=float).reshape(4,4).T
 x,y,z,w=node.get('rotation',(0,0,0,1)); out=np.eye(4)
 out[:3,:3]=np.array([[1-2*(y*y+z*z),2*(x*y-z*w),2*(x*z+y*w)],
 [2*(x*y+z*w),1-2*(x*x+z*z),2*(y*z-x*w)],
 [2*(x*z-y*w),2*(y*z+x*w),1-2*(x*x+y*y)]])@np.diag(node.get('scale',(1,1,1)))
 out[:3,3]=node.get('translation',(0,0,0));return out

def glb_geometry(path):
 raw=path.read_bytes(); assert struct.unpack_from('<4sII',raw)==(b'glTF',2,len(raw))
 n,kind=struct.unpack_from('<II',raw,12);assert kind==0x4e4f534a
 d=json.loads(raw[20:20+n]); bn,bk=struct.unpack_from('<II',raw,20+n);assert bk==0x004e4942
 binary=raw[28+n:28+n+bn]
 def acc(index):
  a=d['accessors'][index]; assert 'sparse' not in a
  v=d['bufferViews'][a['bufferView']];dt=np.dtype({5121:'u1',5123:'<u2',5125:'<u4',5126:'<f4'}[a['componentType']]);k={'SCALAR':1,'VEC2':2,'VEC3':3,'VEC4':4}[a['type']]
  return np.ndarray((a['count'],k),dt,buffer=binary,offset=v.get('byteOffset',0)+a.get('byteOffset',0),strides=(v.get('byteStride',k*dt.itemsize),dt.itemsize)).copy()
 triangles=[]
 def walk(i,parent):
  n=d['nodes'][i]; assert 'skin' not in n
  m=parent@node_matrix(n)
  if 'mesh' in n:
   for q in d['meshes'][n['mesh']]['primitives']:
    assert q.get('mode',4)==4
    p=acc(q['attributes']['POSITION'])@m[:3,:3].T+m[:3,3]
    ids=acc(q['indices']).reshape(-1) if 'indices' in q else np.arange(len(p))
    triangles.append(p[ids.reshape(-1,3)])
  for child in n.get('children',[]):walk(child,m)
 for i in d['scenes'][d.get('scene',0)]['nodes']:walk(i,np.eye(4))
 tri=np.concatenate(triangles);assert np.isfinite(tri).all()
 return tri,{'sha256':sha(raw),'triangle_count':len(tri),'materials':[m.get('name','') for m in d.get('materials',[])],'nodes':[n.get('name','') for n in d.get('nodes',[])]}

def segment_distance(p,a,b):
 v=b-a;return float(np.linalg.norm(p-(a+v*np.clip(np.dot(p-a,v)/np.dot(v,v),0,1))))

def build(source_root,repo_root):
 base=source_root/SOURCE; source_raw=base.read_bytes(); original=json.loads(source_raw); d=copy.deepcopy(original)
 level_raw=(base.parent/'level_design.v1.json').read_bytes();level=json.loads(level_raw)
 heights_raw=(base.parent/d['terrain']['heights']).read_bytes(); heights=np.load(base.parent/d['terrain']['heights'])
 res=d['terrain']['res'];spacing=d['terrain']['spacing'];assert heights.shape==(res,res)
 half=(res-1)*.5
 def height(x,z):
  gx=x/spacing+half;gz=z/spacing+half;i=int(math.floor(gx));j=int(math.floor(gz));assert 0<=i<res-1 and 0<=j<res-1
  a=gx-i;b=gz-j
  return float((heights[j,i]*(1-a)+heights[j,i+1]*a)*(1-b)+(heights[j+1,i]*(1-a)+heights[j+1,i+1]*a)*b)
 roads=level['roads']; segments=[]
 for road in roads:
  pts=road['polyline_xz_m']
  assert pts is not None,road.keys()
  width=road['nominal_width_m']
  for a,b in zip(pts,pts[1:]):segments.append((road['id'],np.array(a),np.array(b),width*.5))
 geometry={};sources={}
 for alias,path in ASSETS.items():
  assert path.startswith('assets/') and all(part not in ('.','..') for part in Path(path).parts)
  tri,meta=glb_geometry(source_root/path);geometry[alias]=tri;sources[alias]={'path':path,**meta}
  if path not in d['assets']:d['assets'].append(path)
 spawn=np.array([1850.,250.]);before_count=len(d['placements']);added=[]
 for group,items in GROUPS.items():
  for alias,x,z,yaw,scale in items:
   tri=geometry[alias];s=np.array([scale]*3 if isinstance(scale,(float,int)) else scale)
   theta=np.radians(yaw);c=np.cos(theta);sn=np.sin(theta);rot=np.array([[c,0,sn],[0,1,0],[-sn,0,c]])
   local=(tri*s)@rot.T;radius=float(np.linalg.norm(local[..., [0,2]],axis=-1).max())
   center=np.array([x,z],dtype=float);spawn_clearance=float(np.linalg.norm(center-spawn)-radius)
   assert spawn_clearance>=24,(group,alias,'spawn',spawn_clearance)
   edge=half*spacing-max(abs(x),abs(z))-radius;assert edge>=24
   clearances=[(rid,segment_distance(center,a,b)-rw-radius) for rid,a,b,rw in segments]
   road_id,road_clearance=min(clearances,key=lambda q:q[1]);assert road_clearance>=8,(group,alias,'road',road_clearance)
   # A conservative AABB radius plus 1.5m gap separates additions from every original placement origin.
   # Original nearby assets are narrow poles and three stones; report nearest original rather than altering it.
   near=min((math.hypot(x-p[1],z-p[3]),i) for i,p in enumerate(original['placements']))
   assert near[0]>radius+2,(group,alias,'original overlap',near,radius)
   # Place mesh's lowest source vertex at its bilinear terrain origin; a 2.5cm sink hides seams.
   y=height(x,z)-float(local[...,1].min())-.025
   placement=[d['assets'].index(ASSETS[alias]),x,round(y,6),z,yaw,scale]; d['placements'].append(placement)
   lo=local.reshape(-1,3).min(0)+[x,y,z];hi=local.reshape(-1,3).max(0)+[x,y,z]
   added.append({'group':group,'alias':alias,'placement_index':len(d['placements'])-1,'placement':placement,'world_bounds':{'min':lo.tolist(),'max':hi.tolist()},'horizontal_radius_m':radius,'spawn_footprint_clearance_m':spawn_clearance,'nearest_road':road_id,'road_edge_footprint_clearance_m':road_clearance,'nearest_original_origin_distance_m':near[0],'nearest_original_index':near[1]})
 assert d['placements'][:before_count]==original['placements']
 for key in original:
  if key not in ('assets','placements'):assert d[key]==original[key]
 for a in d['assets']:assert (source_root/a).is_file(),a
 assert (source_root/d['terrain']['texture']).is_file()
 def count_near(data,radius):
  rows=[p for p in data['placements'] if math.hypot(p[1]-spawn[0],p[3]-spawn[1])<=radius]
  return {'placements':len(rows),'distinct_meshes':len({p[0] for p in rows}),'mesh_counts':dict(sorted(Counter(Path(data['assets'][p[0]]).name for p in rows).items()))}
 manifest_raw=json_bytes(d)
 receipt={'schema':'panzer.winter-approach-revision.v1','revision':REVISION,'source_manifest':str(base),'source_manifest_sha256':sha(source_raw),'revision_manifest_sha256':sha(manifest_raw),'source_root':str(source_root),'seed':d['seed'],'additive_placements':len(added),'total_before':before_count,'total_after':len(d['placements']),'assets_before':len(original['assets']),'assets_after':len(d['assets']),'groups':{k:len(v) for k,v in GROUPS.items()},'near_spawn':{str(r):{'before':count_near(original,r),'after':count_near(d,r)} for r in (50,150,250)},'preserved_byte_exact':{'heights.npy':sha(heights_raw),'level_design.v1.json':sha(level_raw),'features.json':sha((base.parent/'features.json').read_bytes())},'preserved_semantic_fields':[k for k in original if k not in ('assets','placements')],'minimum_spawn_footprint_clearance_m':min(a['spawn_footprint_clearance_m'] for a in added),'minimum_road_edge_footprint_clearance_m':min(a['road_edge_footprint_clearance_m'] for a in added),'all_declared_assets_physically_resolve':True,'added_source_assets':sources,'placements':added,'source_license':'Locally purchased Pizza Doggy packs; integrated owner-private game use. See docs/THIRD_PARTY_ASSETS.md. No standalone model redistribution.','limitations':['CPU geometry and authored-route checks only; runtime procedural overlay remains unchanged and can overlap additions.','Grounded at each origin with preserved bilinear heightfield. Full rigid footprint/terrain contacts require in-game visual check.','No new surveyed historical reconstruction claimed; this is authored environmental art.','Actual game camera, walking/turning and collision review remains a release gate.'],'capture_recommendation':{'position_xz':[1855,250],'look_at_xz':[2050,120],'secondary':'Turn toward east field edge, then west exit; walk toward fence and shelter without entering authored roads.'}}
 outputs={'manifest.v1.json':manifest_raw,'heights.npy':heights_raw,'level_design.v1.json':level_raw,'features.json':(base.parent/'features.json').read_bytes(),'receipt.json':json_bytes(receipt)}
 notes=f'''# Act I winter approach revision\n\nOwner-private additive scene for the isolated shader worktree. **{len(added)} placements in five groups**, using preserved canonical models.\n\nReproduce: `python tools/worldgen/build_winter_approach_revision.py`\n\nVerify byte-identical output: `python tools/worldgen/build_winter_approach_revision.py --check`\n\nSelect with `KESSEL_PLAYTEST_MANIFEST={repo_root/REVISION/'manifest.v1.json'}`. Explicit launcher overrides must remain supported. Root integration owns launcher selection.\n\nTerrain, markers, authored roads, wires, overview, seed and all {before_count} original placements are preserved. NPY, level design and features are copied byte-identical beside the revision, as required by ActManifest's local height path. Asset and terrain texture references remain relative to the canonical `KESSEL_ASSET_ROOT`. No source GLB was changed or copied.\n\nAdded groups: tall/short bare snags near the approach; broken timber boundary with crates/barrels; low ruined field shelter backed by irregular trees; east field edge; west exit remnant. Fixed coordinates and yaw/scale choices avoid uniform scatter. Trees 8 and 9 are source meshes with branching trunks and no leaf meshes, verified in the CPU silhouette contact sheet.\n\nWithin 150 m: {count_near(original,150)['placements']} → {count_near(d,150)['placements']} authored placements; {count_near(original,150)['distinct_meshes']} → {count_near(d,150)['distinct_meshes']} distinct meshes. The separate deterministic runtime story overlay remains unchanged and is excluded from these manifest counts.\n\nConservative added-mesh footprint clearance: at least {receipt['minimum_road_edge_footprint_clearance_m']:.2f} m beyond the authored road edge and {receipt['minimum_spawn_footprint_clearance_m']:.2f} m from spawn. Meshes are grounded at the original bilinear terrain height. Source bounds and placement checks are in receipt.json. A CPU clearance calculation does not establish playability: root must inspect the real camera at (1855,250) toward (2050,120), walk/turn, and check rigid ground contacts and runtime-overlay intersections.\n\nSources are already-local Pizza Doggy game assets; see docs/THIRD_PARTY_ASSETS.md for owner-private game integration terms. No model redistributable or public export is produced.\n'''
 outputs['README.md']=notes.encode()
 return outputs,receipt,geometry

def contact_sheet(geometry,path):
 from PIL import Image,ImageDraw
 image=Image.new('RGB',(1320,600),(224,228,231));draw=ImageDraw.Draw(image)
 for row,alias in enumerate(('snag_tall','snag_short')):
  for col,angle in enumerate((0,45,90)):
   tri=geometry[alias];a=np.radians(angle);u=tri[...,0]*np.cos(a)+tri[...,2]*np.sin(a);v=tri[...,1]
   # Shared meter scale across assets so shorter tree remains visibly distinct.
   xx=col*440+220+u*28; yy=row*300+260-v*28
   for x,y in zip(xx,yy):draw.polygon(list(zip(x.tolist(),y.tolist())),fill=(36,38,39))
   draw.text((col*440+15,row*300+10),f'{ASSETS[alias].split("/")[-1]} | {angle} deg | actual source triangles',fill=(20,20,20))
 draw.text((15,584),'CPU geometry silhouettes, not game shading or art-direction acceptance. Same 28 px/m scale.',fill=(20,20,20))
 import io
 stream=io.BytesIO();image.save(stream,format='PNG');return stream.getvalue()

def main():
 p=argparse.ArgumentParser();p.add_argument('--source-root',type=Path,default=Path('/home/alex/panzer/tank_simulator'));p.add_argument('--repo-root',type=Path,default=Path('/home/alex/panzer/shader-graphics'));p.add_argument('--check',action='store_true');p.add_argument('--contact-sheet',action='store_true',help='Only write optional Pillow CPU silhouette evidence');p.add_argument('--dry-run',action='store_true');args=p.parse_args()
 outputs,receipt,geometry=build(args.source_root,args.repo_root)
 if args.dry_run:
  print(json.dumps({k:receipt[k] for k in ('additive_placements','total_after','assets_after','groups','near_spawn','minimum_spawn_footprint_clearance_m','minimum_road_edge_footprint_clearance_m')},indent=2));return
 target=args.repo_root/REVISION
 paths={target/name:blob for name,blob in outputs.items()}
 if args.contact_sheet:paths={args.repo_root/'captures/astra/winter_approach_source_silhouettes_20260913_v1.png':contact_sheet(geometry,None)}
 for path,blob in paths.items():
  if args.check:assert path.read_bytes()==blob,f'Changed or absent: {path}'
  elif path.exists():assert path.read_bytes()==blob,f'Refusing to overwrite different revision: {path}'
  else:path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(blob)
 print(json.dumps({'status':'verified' if args.check else 'generated','manifest':str(target/'manifest.v1.json'),'added_placements':receipt['additive_placements'],'files':len(paths),'min_road_edge_clearance_m':receipt['minimum_road_edge_footprint_clearance_m']},indent=2))
if __name__=='__main__':main()
