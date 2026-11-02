import sys,json,numpy as np,hashlib
from pathlib import Path
from mathutils.bvhtree import BVHTree
sys.path.insert(0,'tools');from prepare_tiger_cooling import read_output,metrics,glb_bytes
sys.path.insert(0,'artifacts/workshop-source-20261030');import scan_engine_access as E
base=Path('artifacts/workshop-source-20261030');P=np.load(base/'engine_scene_yaw-65.npz');H=P['H'];N=P['N'];T=P['T'];rows=[];rear=-1.7538771587468318;front=2*(-1.74025518)-rear;gap=.00015
for root,name,sgn,x in [('Hull_0932','left',1,-.60617610833),('Hull_0933','right',-1,.60812212388)]:
 d=read_output(base/('engine_'+name+'_lock_pin_candidate.glb'));before=metrics(d);original=d['vertices'].copy();z=d['vertices'][:,2];inside=(z>rear)&(z<front);d['vertices'][:,2]+=np.where(z<=rear,gap,np.where(z>=front,-gap,gap-2*gap*(z-rear)/(front-rear)));slope=1-2*gap/(front-rear);d['vertices'][inside,5]/=slope;d['vertices'][:,3:6]/=np.linalg.norm(d['vertices'][:,3:6],axis=1)[:,None];stats=metrics(d);assert all(stats[k]==0 for k in ['boundary_edges','nonmanifold_edges','inconsistent_winding_edges','collapsed_edges','degenerate_triangles']);p=base/('engine_'+name+'_lock_axial_candidate.glb');p.write_bytes(glb_bytes('engine_'+name+'_lock',d));a=d['vertices'][d['triangles'],:3];sel=N!=root;h=H[sel];n=N[sel];t=T[sel];tree=BVHTree.FromPolygons(h.reshape(-1,3).tolist(),np.arange(len(h)*3).reshape(-1,3).tolist(),all_triangles=True);hits=[]
 for angle in np.arange(0,110.0001,.25):
  M=E.rotation([0,0,1],sgn*angle,[x,2.06130580722,-1.74025518]);v=a@M[:3,:3].T+M[:3,3];local=BVHTree.FromPolygons(v.reshape(-1,3).tolist(),np.arange(len(v)*3).reshape(-1,3).tolist(),all_triangles=True);pairs=local.overlap(tree)
  if pairs:
   counts={}
   for i,j in pairs:counts[str(n[j])]=counts.get(str(n[j]),0)+1
   hits.append(dict(angle=float(angle),count=len(pairs),roots=counts))
 row=dict(name=name,file=p.name,sha256=hashlib.sha256(p.read_bytes()).hexdigest(),before=before,after=stats,authored_end_clearance_m=gap,rear_wing_plane_source_m=rear,front_wing_plane_source_m=front,maximum_vertex_change_m=float(np.linalg.norm(d['vertices'][:,:3]-original[:,:3],axis=1).max()),pin_axial_length_unchanged=True,removed_volume_m3=before['volume_m3']-stats['volume_m3'],static_full_face_step_degrees=.25,hits=hits);rows.append(row);print(name,'hits',len(hits),hits[:5],flush=True)
(base/'engine_side_lock_axial_candidates.json').write_text(json.dumps(rows,indent=2))
