"""CPU Blender worker: bounded engine-cover hinge relief; original GLB remains untouched.
Outputs candidate geometry and measurements under artifacts, never activates gameplay.
Run from repository root with Blender --background --factory-startup --python-exit-code 1.
"""
import sys,numpy as np,json,copy,hashlib,math
from pathlib import Path
import bpy
sys.path.insert(0,'tools')
from fit_crew_steel_helmet import read_glb,accessor
from validate_tiger_hatches import matrix
from prepare_tiger_cooling import make_blender_source,blender_data,boolean,metrics,glb_bytes
D,B,raw=read_glb('assets/tanks/tiger/baked/tiger_hull.glb');node=next(n for n in D['nodes'] if n.get('name')=='Hull_0975');p=D['meshes'][node['mesh']]['primitives'][0];M=matrix(node);v=accessor(D,B,p['attributes']['POSITION'])@M[:3,:3].T+M[:3,3];n=accessor(D,B,p['attributes']['NORMAL'])@np.linalg.inv(M[:3,:3]);n/=np.linalg.norm(n,axis=1)[:,None];data=dict(vertices=np.c_[v,n,accessor(D,B,p['attributes']['TEXCOORD_0'])],triangles=accessor(D,B,p['indices']).reshape(-1,3),material=copy.deepcopy(D['materials'][p['material']]))
obj=make_blender_source('cover_clearance_candidate',data);center=np.array([0,2.00876401746,-2.62762745377]);segments=256

def cyl(name,xmin,xmax,r):
 a=np.arange(segments)*2*math.pi/segments+.371;yz=center[1:]+r*np.c_[np.cos(a),np.sin(a)];vs=np.concatenate([np.c_[np.full(segments,x),yz] for x in [xmin,xmax]]);fs=[(i,(i+1)%segments,(i+1)%segments+segments,i+segments) for i in range(segments)]+[tuple(range(segments-1,-1,-1)),tuple(range(segments,segments*2))];m=bpy.data.meshes.new(name);m.from_pydata(vs.tolist(),[],fs);m.update();m.uv_layers.new(name='TEXCOORD_0');o=bpy.data.objects.new(name,m);bpy.context.collection.objects.link(o);return o

def box(name,lo,hi):
 vs=np.array([[x,y,z] for x in [lo[0],hi[0]] for y in [lo[1],hi[1]] for z in [lo[2],hi[2]]]);fs=[(0,1,3,2),(4,6,7,5),(0,4,5,1),(2,3,7,6),(0,2,6,4),(1,5,7,3)];m=bpy.data.meshes.new(name);m.from_pydata(vs.tolist(),[],fs);m.update();m.uv_layers.new(name='TEXCOORD_0');o=bpy.data.objects.new(name,m);bpy.context.collection.objects.link(o);return o

for side,x0,x1 in [('left',-.4894167,-.3823872),('right',.3843332,.4913627)]:
 bore=cyl(side+'_journal',x0-.00005,x1+.00005,.0160);boolean(obj,bore,'DIFFERENCE')
 # Only relieve the measured lower/rear moving-lug quadrant; source deck is untouched.
 cutter=box(side+'_heel_relief',[x0-.00005,1.94,-2.67],[x1+.00005,center[1],center[2]+.00005])
 keep=cyl(side+'_heel_limit',x0-.0001,x1+.0001,.0541);boolean(cutter,keep,'DIFFERENCE');boolean(obj,cutter,'DIFFERENCE')
# The moving outer knuckle flares overlap the fixed cheeks during rotation.
# Bound all four relief notches strictly to the measured rear knuckle region.
for i,(x0,x1) in enumerate([(-.54,-.4891167),(-.3826872,-.335),(.335,.3846332),(.4910627,.54)]):
 relief=box('knuckle_side_relief_'+str(i),[x0,1.94,-2.67],[x1,2.085,-2.5597]);boolean(obj,relief,'DIFFERENCE')
import bmesh
bm=bmesh.new();bm.from_mesh(obj.data);bmesh.ops.remove_doubles(bm,verts=list(bm.verts),dist=.000002);bmesh.ops.triangulate(bm,faces=list(bm.faces));bmesh.ops.dissolve_degenerate(bm,edges=list(bm.edges),dist=.000002);bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(obj.data);bm.free()
a=blender_data(obj,data['material']);stats=metrics(a);before=metrics(data);print('STATS',stats,flush=True);assert stats['volume_m3']>0
assert all(stats[k]==0 for k in ['boundary_edges','nonmanifold_edges','inconsistent_winding_edges','collapsed_edges','degenerate_triangles'])
path=Path('artifacts/workshop-source-20261030/engine_cover_knuckle_candidate.glb');path.write_bytes(glb_bytes('engine_cover_knuckle_candidate',a));report=dict(status='CANDIDATE ONLY; not deployed or path-certified',source_sha256=hashlib.sha256(raw).hexdigest(),source_root='Hull_0975',source=before,derivative=stats,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),journal_radius_m=.016,source_journal_radius_m=.0155678724281,heel_radius_m=.0541,segments=256,removed_volume_m3=before['volume_m3']-stats['volume_m3'],removed_fraction=1-stats['volume_m3']/before['volume_m3']);Path('artifacts/workshop-source-20261030/engine_cover_knuckle_candidate.json').write_text(json.dumps(report,indent=2));print('CANDIDATE',report['removed_fraction'],flush=True)

