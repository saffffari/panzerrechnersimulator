"""Source-preserving CSG rear latch arm; avoid nonlinear deformation of planar faces."""
import sys,json,numpy as np,hashlib,math
from pathlib import Path
import bpy,bmesh
from mathutils.bvhtree import BVHTree
sys.path.insert(0,'tools');from prepare_tiger_cooling import read_output,make_blender_source,blender_data,boolean,metrics,glb_bytes
from prepare_tiger_engine_service import source_data,SHA
from fit_crew_steel_helmet import read_glb
sys.path.insert(0,'artifacts/workshop-source-20261030');import scan_engine_access as E
base=Path('artifacts/workshop-source-20261030');D,B,raw=read_glb('assets/tanks/tiger/baked/tiger_hull.glb');assert hashlib.sha256(raw).hexdigest()==SHA;data,_=source_data(D,B,'Hull_1068');arm=make_blender_source('engine_latch_arm',data);center=np.array([2.034063,-2.4393255]);n=256

def cyl(name,xlo,xhi,r):
 angle=np.arange(n)*2*math.pi/n+.371;yz=center+r*np.c_[np.cos(angle),np.sin(angle)];v=np.concatenate([np.c_[np.full(n,x),yz] for x in [xlo,xhi]]);f=[(i,(i+1)%n,(i+1)%n+n,i+n) for i in range(n)]+[tuple(range(n-1,-1,-1)),tuple(range(n,n*2))];return obj(name,v,f)
def box(name,lo,hi):
 v=[[x,y,z] for x in [lo[0],hi[0]] for y in [lo[1],hi[1]] for z in [lo[2],hi[2]]];f=[(0,1,3,2),(4,6,7,5),(0,4,5,1),(2,3,7,6),(0,2,6,4),(1,5,7,3)];return obj(name,v,f)
def obj(name,v,f):
 m=bpy.data.meshes.new(name);m.from_pydata(np.array(v).tolist(),[],f);m.update();m.uv_layers.new(name='TEXCOORD_0');o=bpy.data.objects.new(name,m);bpy.context.collection.objects.link(o);return o
def boolean(obj,cutter,operation):
 bpy.context.view_layer.objects.active=obj;obj.select_set(True);m=obj.modifiers.new('Bounded source clearance','BOOLEAN');m.operation=operation;m.solver='MANIFOLD';m.object=cutter;bpy.ops.object.modifier_apply(modifier=m.name);obj.select_set(False)
boolean(arm,box('pedestal_gap',[-.26,1.97,-2.67],[-.20,2.0103,-2.405]),'DIFFERENCE')
boolean(arm,cyl('arm_bore',-.26,-.20,.0197),'DIFFERENCE')
boolean(arm,cyl('clevis_slot',-.2367,-.2245,.035),'DIFFERENCE')
# Cut the exterior of the head to its measured clearance envelope instead of warping triangles.
outer=box('head_region',[-.26,1.97,center[1]-.0234],[-.20,2.15,-2.40]);boolean(outer,cyl('head_envelope',-.261,-.199,.0238),'DIFFERENCE');boolean(arm,outer,'DIFFERENCE')
bm=bmesh.new();bm.from_mesh(arm.data);bmesh.ops.remove_doubles(bm,verts=list(bm.verts),dist=.000002);bmesh.ops.triangulate(bm,faces=list(bm.faces));bmesh.ops.dissolve_degenerate(bm,edges=list(bm.edges),dist=.000002);bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(arm.data);bm.free();d=blender_data(arm,data['material']);stats=metrics(d);print(stats,flush=True);assert all(stats[k]==0 for k in ['boundary_edges','nonmanifold_edges','inconsistent_winding_edges','collapsed_edges','degenerate_triangles']);p=base/'engine_latch_arm_csg_candidate.glb';p.write_bytes(glb_bytes('engine_latch_arm',d))
P=np.load(base/'engine_scene_yaw-65.npz');H=P['H'];N=P['N'];T=P['T'];sel=~np.isin(N,['Hull_0975','Hull_1068']);H=H[sel];N=N[sel];T=T[sel]
for name,file in [('cover_leaf','engine_cover_knuckle_candidate'),('latch_base','engine_latch_base_candidate')]:
 dd=read_output(base/(file+'.glb'));aa=dd['vertices'][dd['triangles'],:3];H=np.r_[H,aa];N=np.r_[N,np.array([name]*len(aa))];T=np.r_[T,np.arange(len(aa))]
tree=BVHTree.FromPolygons(H.reshape(-1,3).tolist(),np.arange(len(H)*3).reshape(-1,3).tolist(),all_triangles=True);a=d['vertices'][d['triangles'],:3];hits=[]
for degrees in np.arange(0,90.00001,.25):
 M=E.rotation([1,0,0],degrees,[-.2306,*center]);v=a@M[:3,:3].T+M[:3,3];local=BVHTree.FromPolygons(v.reshape(-1,3).tolist(),np.arange(len(v)*3).reshape(-1,3).tolist(),all_triangles=True);pairs=local.overlap(tree)
 if pairs:
  counts={};w={}
  for i,j in pairs:
   k=str(N[j]);counts[k]=counts.get(k,0)+1
   if k not in w:w[k]=dict(moving_triangle=int(i),fixed_triangle=int(T[j]),moving=v[i].tolist(),fixed=H[j].tolist())
  hits.append(dict(angle=float(degrees),pairs=len(pairs),roots=counts,witness=w))
prior=metrics(read_output(base/'engine_latch_arm_clearance_candidate.glb'));sbase=metrics(read_output(base/'engine_latch_base_candidate.glb'));original=metrics(data);report=dict(description='Direct source CSG head envelope and journal/clevis cuts; no nonlinear vertex deformation. Exterior source material preserved outside recorded cuts.',source_root='Hull_1068',source_sha256=SHA,fixed_base_changed=False,source=original,prior_arm=prior,after=stats,head_outer_radius_m=.0238,head_region_min_z_m=float(center[1]-.0234),bore_radius_m=.0197,base_radius_m=.0191,polygon_apothem_minimum_radial_clearance_m=.0197*math.cos(math.pi/256)-.0191,clevis_x=[-.2367,-.2245],combined_base_and_arm_volume_m3=sbase['volume_m3']+stats['volume_m3'],combined_removed_fraction=1-(sbase['volume_m3']+stats['volume_m3'])/original['volume_m3'],sha256=hashlib.sha256(p.read_bytes()).hexdigest(),static_full_face_step_deg=.25,static_full_face_hits=hits,continuous_sweep_pending=True);(base/'engine_latch_arm_csg_candidate.json').write_text(json.dumps(report,indent=2));print('STATIC_FAILURES',len(hits),hits[:2],flush=True)
