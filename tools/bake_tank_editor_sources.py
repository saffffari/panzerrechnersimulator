#!/usr/bin/env python3
"""Lossless triangle/object OBJ -> static GLB for archived full-interior editor tanks.

python3 tools/bake_tank_editor_sources.py --vehicle tiger_ii
Only Python/NumPy; no Blender import validation, decimation, inferred rig or GPU rendering.
"""
from __future__ import annotations
import argparse
from array import array
from io import BytesIO
import hashlib
import json
from pathlib import Path
import struct
import zipfile
import numpy as np

SPECS = {
 'tiger_ii': {'archive':'pzkpfw-vi-tiger-2-with-full-interior.zip','label':'Tiger II','linear':[[0,0,-1],[0,-1,0],[-1,0,0]],'offset':[.04,-.19,.39]},
 'is2': {'archive':'is-2-with-full-interior.zip','label':'IS-2','linear':[[-.5,0,0],[0,.5,0],[0,0,-.5]],'offset':[-.06706825,-.096057,-.64426175]},
 'panzer_iv': {'archive':'pzkpfw-iv-ausf-g-with-full-interior.zip','label':'Panzer IV Ausf. G','linear':[[0,0,-1],[0,-1,0],[-1,0,0]],'offset':[-.1269725,.208103,1.5002635]},
}
def digest(data):return hashlib.sha256(data).hexdigest()

def main():
 parser=argparse.ArgumentParser(description=__doc__)
 parser.add_argument('--vehicle',choices=sorted(SPECS),required=True)
 parser.add_argument('--source-root',type=Path,default=Path('/home/alex/panzer/tank_simulator/source_assets/legacy-imports/asset-warehouse-20260831/incoming/2026-08-31-new-assets'))
 parser.add_argument('--output-root',type=Path,default=Path(__file__).resolve().parents[1]/'assets/tanks/editor')
 opts=parser.parse_args();spec=SPECS[opts.vehicle]
 archive=opts.source_root/spec['archive'];outer_bytes=archive.read_bytes()
 with zipfile.ZipFile(BytesIO(outer_bytes)) as outer:
  nested_name=next(n for n in outer.namelist() if n.endswith('.zip'));nested_bytes=outer.read(nested_name)
 with zipfile.ZipFile(BytesIO(nested_bytes)) as nested:
  names=nested.namelist();source_name=next(n for n in names if n.endswith('.obj'));obj_bytes=nested.read(source_name)
 assert names==[source_name],f'Extra source resources need review: {names}'
 assert b'\nmtllib ' not in obj_bytes and b'\nusemtl ' not in obj_bytes
 positions=array('f');normals=array('f');uvs=array('f');source_faces=0;source_names=[]
 for line in BytesIO(obj_bytes):
  if line.startswith(b'v '):positions.extend(map(float,line.split()[1:4]))
  elif line.startswith(b'vn '):normals.extend(map(float,line.split()[1:4]))
  elif line.startswith(b'vt '):uvs.extend(map(float,line.split()[1:3]))
  elif line.startswith(b'o '):source_names.append(line[2:].strip().decode())
  elif line.startswith(b'f '):
   assert len(line.split())==4,'Non-triangular source needs review';source_faces+=1
 p=np.asarray(positions).reshape(-1,3);n=np.asarray(normals).reshape(-1,3);uv=np.asarray(uvs).reshape(-1,2)
 raw_min=p.min(0).tolist();raw_max=p.max(0).tolist();linear=np.array(spec['linear']);offset=np.array(spec['offset'])
 game_p=np.asarray(p@linear.T+offset,dtype='<f4');normal_matrix=np.linalg.inv(linear).T
 game_n=n@normal_matrix.T;length=np.linalg.norm(game_n,axis=1);nonzero=length>1e-12;game_n[nonzero]/=length[nonzero,None];game_n=np.asarray(game_n,dtype='<f4')
 game_uv=np.asarray(uv*np.array([1.,-1.])+np.array([0.,1.]),dtype='<f4') # OBJ bottom-left -> glTF top-left convention.
 doc={'asset':{'version':'2.0','generator':'Panzer lossless source OBJ baker v1'},'scene':0,'scenes':[{'nodes':[]}],
  'nodes':[],'meshes':[],'materials':[{'name':'Provisional neutral matte metal - source has no materials','pbrMetallicRoughness':{'baseColorFactor':[.18,.20,.22,1.],'metallicFactor':.25,'roughnessFactor':.82}}],
  'accessors':[],'bufferViews':[],'buffers':[]}
 binary=bytearray();records=[];object_name=None;tuples=[];index_map={};indices=array('I');face_count=0
 def accessor(a,kind,component,bounds=False,target=34962):
  while len(binary)%4:binary.append(0)
  start=len(binary);binary.extend(a.tobytes());view=len(doc['bufferViews']);doc['bufferViews'].append({'buffer':0,'byteOffset':start,'byteLength':len(binary)-start,'target':target})
  item={'bufferView':view,'componentType':component,'count':len(a),'type':kind}
  if bounds:item.update(min=a.min(0).tolist(),max=a.max(0).tolist())
  idx=len(doc['accessors']);doc['accessors'].append(item);return idx
 def flush():
  nonlocal tuples,index_map,indices
  if object_name is None:return
  assert len(indices)>0,object_name
  refs=np.asarray(tuples,dtype=np.int32)
  pos=game_p[refs[:,0]];norm=game_n[refs[:,2]];tex=game_uv[refs[:,1]]
  ip=np.asarray(indices,dtype='<u4')
  primitive={'attributes':{'POSITION':accessor(pos,'VEC3',5126,True),'NORMAL':accessor(norm,'VEC3',5126),'TEXCOORD_0':accessor(tex,'VEC2',5126)},'indices':accessor(ip,'SCALAR',5125,target=34963),'material':0,'mode':4}
  mesh_index=len(doc['meshes']);doc['meshes'].append({'name':object_name,'primitives':[primitive]})
  doc['scenes'][0]['nodes'].append(len(doc['nodes']));doc['nodes'].append({'name':object_name,'mesh':mesh_index,'extras':{'source_object_name':object_name,'editor_static_source':True}})
  records.append({'name':object_name,'split_vertices':len(pos),'triangles':len(ip)//3,'game_bounds_min':pos.min(0).tolist(),'game_bounds_max':pos.max(0).tolist()})
  tuples=[];index_map={};indices=array('I')
 for line in BytesIO(obj_bytes):
  if line.startswith(b'o '):flush();object_name=line[2:].strip().decode()
  elif line.startswith(b'f '):
   for token in line.split()[1:]:
    if token not in index_map:
     ref=tuple(int(v)-1 for v in token.split(b'/'));assert len(ref)==3 and min(ref)>=0,'Non-full/negative OBJ indices need review'
     assert ref[0]<len(game_p) and ref[1]<len(game_uv) and ref[2]<len(game_n)
     index_map[token]=len(tuples);tuples.append(ref)
    indices.append(index_map[token])
   face_count+=1
 flush();assert face_count==source_faces and [r['name'] for r in records]==source_names
 doc['buffers']=[{'byteLength':len(binary)}]
 encoded=json.dumps(doc,separators=(',',':')).encode();encoded+=b' '*((-len(encoded))%4);binary.extend(b'\0'*((-len(binary))%4))
 header=struct.pack('<III',0x46546c67,2,12+8+len(encoded)+8+len(binary))
 out=opts.output_root/opts.vehicle;out.mkdir(parents=True,exist_ok=True);glb=out/'source_full.glb'
 with glb.open('wb') as f:f.write(header);f.write(struct.pack('<II',len(encoded),0x4e4f534a));f.write(encoded);f.write(struct.pack('<II',len(binary),0x004e4942));f.write(binary)
 matrix=np.eye(4);matrix[:3,:3]=linear;matrix[:3,3]=offset
 receipt={'schema':'panzer.tank-editor-source-bake','version':1,'vehicle_id':opts.vehicle,'label':spec['label'],
  'source_archive':str(archive),'archive_sha256':digest(outer_bytes),'nested_archive':nested_name,'nested_archive_sha256':digest(nested_bytes),'source_object_file':source_name,'obj_sha256':digest(obj_bytes),
  'source_generator':'ImageToStl.com (OBJ header)','source_author':'Not supplied in archived files','source_license':'Not supplied in archived files; local authoring source, redistribution status unverified',
  'source_materials':False,'source_textures':False,'source_hierarchy':False,'source_pivots':False,'source_bounds_min':raw_min,'source_bounds_max':raw_max,
  'source_to_game_matrix_rows':matrix.tolist(),'game_frame':'+X tank-left, +Y up, +Z forward, ground y=0',
  'frame_evidence':'CPU vertex projections confirm Tiger II/Panzer IV up=-sourceY and forward=-sourceX; IS-2 up=sourceY and forward=-sourceZ. X/Z centering uses vertex bounds in lowest 0.8 m support band.',
  'unit_scale':.5 if opts.vehicle=='is2' else 1.,'unit_evidence':'Uniform IS-2 0.5 scale gives 9.75 m length instead of 19.51 m. Others already meter-sized. Provisional physical normalization, not source units or a calibrated blueprint.',
  'game_bounds_min':game_p.min(0).tolist(),'game_bounds_max':game_p.max(0).tolist(),'material':{'status':'provisional editor display, not authored paint','linear_rgb':[.18,.20,.22],'metallic':.25,'roughness':.82},
  'geometry_preserved':True,'object_count':len(records),'triangles':source_faces,'source_vertices':len(p),'source_normals':len(n),'source_uvs':len(uv),'uv_transform':'(u,1-v) OBJ to glTF convention','objects':records,
  'articulation':'Unconfigured static source; OBJ object splits preserved; no inferred joints','playable':False,'output':str(glb),'glb_sha256':digest(glb.read_bytes()),
  'bake_method':'Direct OBJ index triplets to glTF, preserving every source triangle. Blender OBJ import removed triangles and was rejected before export.'}
 (out/'source_manifest.v1.json').write_text(json.dumps(receipt,indent=2)+'\n')
 (out/'SOURCE.md').write_text(f'''# {spec['label']} static editor source\n\nComplete source geometry from `{archive}` → `{nested_name}` → `{source_name}`.\n\nThe archive supplies anonymous OBJ objects, normals and UV coordinates, but no material library, textures, semantic part names, pivots, hierarchy, author or license record. The matte neutral metal is explicitly provisional. Every source triangle and object split is preserved by direct OBJ-to-GLB conversion. This is an inspection/editor model, not a playable tank rig.\n\n`source_manifest.v1.json` records hashes, exact normalization matrix, bounds and the physical-scale limitation. Rebuild with `python3 tools/bake_tank_editor_sources.py --vehicle {opts.vehicle}` from the shader checkout.\n''')
 print('EDITOR_SOURCE_READY',json.dumps({k:receipt[k] for k in ['vehicle_id','object_count','triangles','game_bounds_min','game_bounds_max','output']}),flush=True)
if __name__=='__main__':main()
