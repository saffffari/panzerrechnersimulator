"""CPU-only engine foot/source-floor fit preparation; preserves original assets."""
import sys,json,numpy as np,hashlib
from pathlib import Path
import bpy,bmesh
sys.path.insert(0,'tools');from prepare_tiger_engine_service import source_data,SHA
from fit_crew_steel_helmet import read_glb
from prepare_tiger_cooling import make_blender_source,blender_data,boolean,metrics,glb_bytes
def close_local_fit_boundaries(data):
 # Repair only numerical CSG boundaries in the <=0.71m foot region.
 # First split collinear T-junction edges; then cap bounded newly exposed cut-face loops.
 v=data['vertices'].copy();t=data['triangles'].copy();splits=0;cap_area=0.;cap_triangles=0
 def boundaries():
  _,first,ix=np.unique(np.round(v[:,:3],6),axis=0,return_index=True,return_inverse=True);w=ix[t];edges=np.concatenate([w[:,[0,1]],w[:,[1,2]],w[:,[2,0]]]);unique,back,count=np.unique(np.sort(edges,axis=1),axis=0,return_inverse=True,return_counts=True);where=np.where(count[back]==1)[0];return first,ix,edges[where],where
 for iteration in range(32):
  first,ix,edges,where=boundaries()
  if not len(edges):break
  pts=v[first,:3];assert np.all(pts[np.unique(edges),1]<.71)
  fixed=False
  for (a,b),edge_index in zip(edges,where):
   ab=pts[b]-pts[a];length2=ab@ab
   for q in np.unique(edges):
    if q in (a,b):continue
    u=(pts[q]-pts[a])@ab/length2
    if not .0001<u<.9999 or np.linalg.norm(pts[q]-(pts[a]+u*ab))>2e-6:continue
    tri_index=int(edge_index%len(t));corner=int(edge_index//len(t));old=t[tri_index];aa,bb,cc=old[corner],old[(corner+1)%3],old[(corner+2)%3];attr=v[aa]*(1-u)+v[bb]*u;attr[:3]=pts[q];attr[3:6]/=max(np.linalg.norm(attr[3:6]),1e-15);new=len(v);v=np.vstack([v,attr]);t[tri_index]=[aa,new,cc];t=np.vstack([t,[new,bb,cc]]);splits+=1;fixed=True;break
   if fixed:break
  if not fixed:break
 first,ix,edges,where=boundaries();remaining=[tuple(map(int,e)) for e in edges]
 while remaining:
  a,b=remaining.pop(0);ring=[a,b]
  while ring[-1]!=ring[0]:
   found=next((i for i,e in enumerate(remaining) if e[0]==ring[-1]),None);assert found is not None,('non-loop boundary',ring,remaining);edge=remaining.pop(found);ring.append(edge[1]);assert len(ring)<32
  ring=ring[:-1][::-1];points=v[first[ring],:3];assert np.linalg.norm(points[:,None]-points[None,:],axis=2).max()<.02
  for i in range(1,len(ring)-1):
   corners=v[first[[ring[0],ring[i],ring[i+1]]]].copy();cross=np.cross(corners[1,:3]-corners[0,:3],corners[2,:3]-corners[0,:3]);area=np.linalg.norm(cross)*.5;assert area>1e-13;corners[:,3:6]=cross/(area*2);base=len(v);v=np.vstack([v,corners]);t=np.vstack([t,[base,base+1,base+2]]);cap_area+=area;cap_triangles+=1
 return dict(vertices=v,triangles=t,material=data['material']),dict(t_junction_splits=splits,authored_cut_cap_triangles=cap_triangles,authored_cut_cap_area_m2=cap_area,maximum_cap_loop_extent_limit_m=.02,repair_region_max_y_m=.71)

print('READ_SOURCE',flush=True)
D,B,raw=read_glb('assets/tanks/tiger/baked/tiger_hull.glb');assert hashlib.sha256(raw).hexdigest()==SHA;main,_=source_data(D,B,'Hull_0971');floor,_=source_data(D,B,'Hull_0619');a=main['vertices'][main['triangles'],:3];areas=np.linalg.norm(np.cross(a[:,1]-a[:,0],a[:,2]-a[:,0]),axis=1)*.5;main['triangles']=main['triangles'][areas>1e-13];print('BUILD_ENGINE',flush=True);engine=make_blender_source('engine_foot_fit_candidate',main);print('BUILD_FLOOR',flush=True);support=make_blender_source('actual_floor_support',floor)
def clean(obj):
 bm=bmesh.new();bm.from_mesh(obj.data);bmesh.ops.remove_doubles(bm,verts=list(bm.verts),dist=.000002);bmesh.ops.triangulate(bm,faces=list(bm.faces));bmesh.ops.dissolve_degenerate(bm,edges=list(bm.edges),dist=.000002);bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(obj.data);bm.free()
def box(name,lo,hi):
 vs=[[x,y,z] for x in [lo[0],hi[0]] for y in [lo[1],hi[1]] for z in [lo[2],hi[2]]];fs=[(0,1,3,2),(4,6,7,5),(0,4,5,1),(2,3,7,6),(0,2,6,4),(1,5,7,3)];m=bpy.data.meshes.new(name);m.from_pydata(vs,[],fs);m.update();m.uv_layers.new(name='TEXCOORD_0');o=bpy.data.objects.new(name,m);bpy.context.collection.objects.link(o);return o
print('CLEAN_ENGINE',flush=True)
clean(engine);cleanstats=metrics(blender_data(engine,main['material']));print('CLEAN',cleanstats,flush=True)
boolean(support,box('foot_region',[-.5,.5,-2.7],[.58,.70,-1.28]),'INTERSECT');clean(support)
boolean(engine,support,'DIFFERENCE');clean(engine)
bm=bmesh.new();bm.from_mesh(engine.data);fv=[v for v in bm.verts if v.co.y<.71];fe=[e for e in bm.edges if all(v.co.y<.71 for v in e.verts)];bmesh.ops.remove_doubles(bm,verts=fv,dist=.00001);fe=[e for e in bm.edges if all(v.co.y<.71 for v in e.verts)];fv=[v for v in bm.verts if v.co.y<.71];bmesh.ops.dissolve_degenerate(bm,edges=fe,dist=.00001);fe=[e for e in bm.edges if all(v.co.y<.71 for v in e.verts)];fv=[v for v in bm.verts if v.co.y<.71];bmesh.ops.dissolve_limit(bm,angle_limit=.00001,use_dissolve_boundaries=True,verts=fv,edges=fe);bmesh.ops.triangulate(bm,faces=list(bm.faces));bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(engine.data);bm.free()
data=blender_data(engine,main['material']);a=data['vertices'][data['triangles'],:3];areas=np.linalg.norm(np.cross(a[:,1]-a[:,0],a[:,2]-a[:,0]),axis=1)*.5;data['triangles']=data['triangles'][areas>1e-13];data,repair_receipt=close_local_fit_boundaries(data);stats=metrics(data);print('FIT',stats,flush=True);np.savez_compressed('artifacts/workshop-source-20261030/engine_feet_debug.npz',v=data['vertices'],t=data['triangles']);assert stats['volume_m3']>0;assert all(stats[k]==0 for k in ['boundary_edges','nonmanifold_edges','inconsistent_winding_edges','collapsed_edges','degenerate_triangles'])
base=Path('artifacts/workshop-source-20261030');p=base/'engine_foot_fit_candidate.glb';p.write_bytes(glb_bytes('engine_foot_fit_candidate',data));before=metrics(main);report=dict(status='candidate only; floor-support intersection removed from engine feet, not source floor',source_root='Hull_0971',source_sha256=SHA,original=before,cleaned=cleanstats,derivative=stats,removed_volume_m3=before['volume_m3']-stats['volume_m3'],removed_fraction=1-stats['volume_m3']/before['volume_m3'],support_root='Hull_0619',numerical_boundary_repair=repair_receipt,bounded_cutter_region_hull_m=[[-.5,.5,-2.7],[.58,.70,-1.28]],clearance_m=0.0,limits=['Exact source support contact only; no enlargement or invisible tolerance.','Motion admission and source support contact still required.','Source cleanup drops2zero-area faces and welds/dissolves at2micrometres; post-CSG foot-only cleanup uses10micrometres and recorded bounded cut-face closure.'],sha256=hashlib.sha256(p.read_bytes()).hexdigest());(base/'engine_foot_fit_candidate.json').write_text(json.dumps(report,indent=2));print('REMOVED',report['removed_fraction'],flush=True)
