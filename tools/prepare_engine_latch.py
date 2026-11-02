"""CPU Blender worker: split the fused source latch into pedestal/pin and moving clevis.
Authored clearance is measured; source root and all source UVs are retained as provenance.
Outputs candidates only, not a claim of full swept-face or gameplay validation.
"""
import sys,numpy as np,json,copy,hashlib,math
from pathlib import Path
import bpy
from mathutils import Matrix,Vector
sys.path.insert(0,'tools');from fit_crew_steel_helmet import read_glb,accessor;from validate_tiger_hatches import matrix;from prepare_tiger_cooling import make_blender_source,blender_data,boolean,metrics,glb_bytes
def cyl(name,xmin,xmax,r):
 a=np.arange(segments)*2*math.pi/segments+.371;yz=center[1:]+r*np.c_[np.cos(a),np.sin(a)];vs=np.concatenate([np.c_[np.full(segments,x),yz] for x in [xmin,xmax]]);fs=[(i,(i+1)%segments,(i+1)%segments+segments,i+segments) for i in range(segments)]+[tuple(range(segments-1,-1,-1)),tuple(range(segments,segments*2))];m=bpy.data.meshes.new(name);m.from_pydata(vs.tolist(),[],fs);m.update();m.uv_layers.new(name='TEXCOORD_0');o=bpy.data.objects.new(name,m);bpy.context.collection.objects.link(o);return o

def box(name,lo,hi):
 vs=np.array([[x,y,z] for x in [lo[0],hi[0]] for y in [lo[1],hi[1]] for z in [lo[2],hi[2]]]);fs=[(0,1,3,2),(4,6,7,5),(0,4,5,1),(2,3,7,6),(0,2,6,4),(1,5,7,3)];m=bpy.data.meshes.new(name);m.from_pydata(vs.tolist(),[],fs);m.update();m.uv_layers.new(name='TEXCOORD_0');o=bpy.data.objects.new(name,m);bpy.context.collection.objects.link(o);return o


D,B,raw=read_glb('assets/tanks/tiger/baked/tiger_hull.glb');node=next(n for n in D['nodes'] if n.get('name')=='Hull_1068');p=D['meshes'][node['mesh']]['primitives'][0];M=matrix(node);v=accessor(D,B,p['attributes']['POSITION'])@M[:3,:3].T+M[:3,3];n=accessor(D,B,p['attributes']['NORMAL'])@np.linalg.inv(M[:3,:3]);n/=np.linalg.norm(n,axis=1)[:,None];data=dict(vertices=np.c_[v,n,accessor(D,B,p['attributes']['TEXCOORD_0'])],triangles=accessor(D,B,p['indices']).reshape(-1,3),material=copy.deepcopy(D['materials'][p['material']]));center=np.array([0,2.034063,-2.4393255]);segments=256
base=make_blender_source('latch_base',data);arm=make_blender_source('latch_arm',data)
# Reconstruct a functional clevis within the fused source journal: central
# pedestal supports the original pin; two outer arm ears rotate around it.
env=box('base_envelope',[-.26,1.97,-2.67],[-.20,2.0097,-2.405])
column=box('central_pedestal',[-.2361,1.97,center[2]-.018],[-.2251,2.0182,center[2]+.018]);boolean(env,column,'UNION')
pin=cyl('fixed_journal',-.26,-.20,.0191);boolean(env,pin,'UNION');boolean(base,env,'INTERSECT')
low=box('arm_low_clearance',[-.26,1.97,-2.67],[-.20,2.0103,-2.405]);boolean(arm,low,'DIFFERENCE')
bore=cyl('arm_bore',-.26,-.20,.0197);boolean(arm,bore,'DIFFERENCE')
slot=cyl('central_clevis_slot',-.2367,-.2245,.035);boolean(arm,slot,'DIFFERENCE')

files=[]
for name,obj in [('engine_latch_base_candidate',base),('engine_latch_arm_candidate',arm)]:
 import bmesh
 bm=bmesh.new();bm.from_mesh(obj.data);bmesh.ops.remove_doubles(bm,verts=list(bm.verts),dist=.000002);bmesh.ops.triangulate(bm,faces=list(bm.faces));bmesh.ops.dissolve_degenerate(bm,edges=list(bm.edges),dist=.000002);bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(obj.data);bm.free()
 out=blender_data(obj,data['material'])
 if name=='engine_latch_arm_candidate':
  v=out['vertices'][:,:3].copy();q=v[:,[1,2]]-center[1:];rr=np.linalg.norm(q,axis=1);selected=(v[:,2]>=center[2]-.0234)&(rr>.0225);rmax=rr[selected].max();factor=(.0238-.0225)/(rmax-.0225);newr=.0225+(rr[selected]-.0225)*factor;v[selected,1:3]=center[1:]+q[selected]*(newr/rr[selected])[:,None];delta=np.linalg.norm(v-out['vertices'][:,:3],axis=1);attrs=out['vertices'].copy();attrs[:,:3]=v;tri=out['triangles'];expanded=attrs[tri].copy();a=v[tri];nn=np.cross(a[:,1]-a[:,0],a[:,2]-a[:,0]);nn/=np.linalg.norm(nn,axis=1)[:,None];changed=np.any(selected[tri],axis=1);expanded[changed,:,3:6]=nn[changed,None,:];vv,ix=np.unique(expanded.reshape(-1,8).astype(np.float32),axis=0,return_inverse=True);out=dict(vertices=vv.astype(float),triangles=ix.reshape(-1,3),material=out['material']);print('HEAD_DISPLACEMENT_M',float(delta.max()),flush=True);name='engine_latch_arm_clearance_candidate'
 stats=metrics(out);print(name,stats,flush=True);assert stats['volume_m3']>0;assert all(stats[k]==0 for k in ['boundary_edges','nonmanifold_edges','inconsistent_winding_edges','collapsed_edges','degenerate_triangles']);path=Path('artifacts/workshop-source-20261030/'+name+'.glb');path.write_bytes(glb_bytes(name,out));files.append(dict(file=str(path),sha256=hashlib.sha256(path.read_bytes()).hexdigest(),metrics=stats))
before=metrics(data);report=dict(source_root='Hull_1068',source_sha256=hashlib.sha256(raw).hexdigest(),source_metrics=before,axis_hull=[1,0,0],pivot_hull_m=[-.2306,*center[1:].tolist()],pedestal_fixed_top_m=2.0097,pedestal_arm_bottom_m=2.0103,clevis_slot_x_m=[-.2367,-.2245],clevis_slot_radius_m=.035,journal_base_radius_m=.0191,journal_arm_clearance_radius_m=.0197,files=files,removed_fraction=1-sum(f['metrics']['volume_m3'] for f in files)/before['volume_m3'],status='candidate; combined motion probe pending');Path('artifacts/workshop-source-20261030/engine_latch_split_candidate.json').write_text(json.dumps(report,indent=2));print('REMOVED',report['removed_fraction'],flush=True)
