"""CPU Blender source preparation of the modeled rear driveline joint; no gameplay activation.
Original Hull_0671 remains untouched. The 0.5 mm joint split and short sleeve socket are
explicit authored service clearances within the actual source neck, not hidden collision edits.
"""
import sys,json,numpy as np,hashlib,math
from pathlib import Path
import bpy,bmesh
sys.path.insert(0,'tools');from prepare_tiger_engine_service import source_data,SHA
from fit_crew_steel_helmet import read_glb
from prepare_tiger_cooling import make_blender_source,blender_data,boolean,metrics,glb_bytes
ROOT=Path(__file__).resolve().parents[1];STAGE=ROOT/'artifacts/workshop-source-20261030'
D,B,raw=read_glb(str(ROOT/'assets/tanks/tiger/baked/tiger_hull.glb'));assert hashlib.sha256(raw).hexdigest()==SHA
data,node=source_data(D,B,'Hull_0671');center=[0,.9715509365643697];boundary=-1.160350;half_gap=.00025

def box(name,lo,hi):
 vs=[[x,y,z] for x in [lo[0],hi[0]] for y in [lo[1],hi[1]] for z in [lo[2],hi[2]]];fs=[(0,1,3,2),(4,6,7,5),(0,4,5,1),(2,3,7,6),(0,2,6,4),(1,5,7,3)];m=bpy.data.meshes.new(name);m.from_pydata(vs,[],fs);m.update();m.uv_layers.new(name='TEXCOORD_0');o=bpy.data.objects.new(name,m);bpy.context.collection.objects.link(o);return o

def cylinder(name,z0,z1,r):
 count=256;a=np.arange(count)*2*math.pi/count+.371;ring=np.c_[r*np.cos(a),center[1]+r*np.sin(a)];vs=np.concatenate([np.c_[ring,np.full(count,z)] for z in [z0,z1]]);fs=[(i,(i+1)%count,(i+1)%count+count,i+count) for i in range(count)]+[tuple(range(count-1,-1,-1)),tuple(range(count,count*2))];m=bpy.data.meshes.new(name);m.from_pydata(vs.tolist(),[],fs);m.update();m.uv_layers.new(name='TEXCOORD_0');o=bpy.data.objects.new(name,m);bpy.context.collection.objects.link(o);return o

shaft=make_blender_source('engine_fixed_driveshaft',data);coupler=make_blender_source('engine_rear_coupling',data)
boolean(shaft,box('fixed_side',[-.2,.7,boundary+half_gap],[.2,1.2,1.3]),'INTERSECT')
boolean(coupler,box('coupling_side',[-.2,.7,-1.5],[.2,1.2,boundary-half_gap]),'INTERSECT')
boolean(coupler,cylinder('short_source_neck_socket',boundary-.029,boundary+.02,.0375),'DIFFERENCE')
files=[]
for name,obj in [('engine_fixed_driveshaft_candidate',shaft),('engine_rear_coupling_candidate',coupler)]:
 bm=bmesh.new();bm.from_mesh(obj.data);bmesh.ops.remove_doubles(bm,verts=list(bm.verts),dist=.000002);bmesh.ops.triangulate(bm,faces=list(bm.faces));bmesh.ops.dissolve_degenerate(bm,edges=list(bm.edges),dist=.000002);bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(obj.data);bm.free();a=blender_data(obj,data['material']);s=metrics(a);assert s['finite_attributes'] and s['volume_m3']>0;assert all(s[k]==0 for k in ['boundary_edges','nonmanifold_edges','inconsistent_winding_edges','collapsed_edges','degenerate_triangles']),(name,s);path=STAGE/(name+'.glb');path.write_bytes(glb_bytes(name,a));files.append(dict(file=path.name,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),metrics=s));print(name,s,flush=True)
before=metrics(data);after=sum(f['metrics']['volume_m3'] for f in files)
report=dict(status='source preparation candidate; no admitted removal path yet',source_root='Hull_0671',source_node=node,source_sha256=SHA,cad_label='TG_RM5010-25K74_ROLLAR',coordinate_frame='hull metres; identity node transforms',axis_hull=[0,0,1],centerline_xy_hull_m=center,source_joint_bounds_z_m=[-1.3744089304544787,boundary],neck_boundary_z_m=boundary,authored_joint_gap_m=half_gap*2,source_neck_radius_m=.04086580436,source_fixed_shaft_radius_m=.03697382493,authored_socket_radius_m=.0375,authored_socket_z_m=[boundary-.029,boundary-half_gap],minimum_radial_clearance_m=.0375*math.cos(math.pi/256)-.03697382493,source=before,files=files,removed_volume_m3=before['volume_m3']-after,removed_fraction=1-after/before['volume_m3'],limits=['Short sleeve socket and fitted joint clearance are authored reconstructions within the actual modeled neck; no historical telescoping-joint claim.','Joint spider and source exterior retained; no bore through the spider.','The finite coupling can be axially released and remain sleeve-supported; driveline torque is disconnected while the engine may idle. Complete removal needs separate physical path validation.','Original source remains untouched; visible and collision derivatives must be identical.'])
(STAGE/'engine_rear_coupling_candidate.json').write_text(json.dumps(report,indent=2));print('REMOVED',report['removed_fraction'],flush=True)
