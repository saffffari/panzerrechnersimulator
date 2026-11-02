"""CPU source-preserving side-lock PIN clearance, not a drilled moving lever.
Outward original radial normals establish that the 9.73mm cylinder is a pin.
A 9.40mm radius retains the pin and fits inside the mating24-sided9.73mm bore.
"""
import sys,numpy as np,json,hashlib
from pathlib import Path
sys.path.insert(0,'tools');from prepare_tiger_engine_service import source_data;from fit_crew_steel_helmet import read_glb;from prepare_tiger_cooling import metrics,glb_bytes
D,B,raw=read_glb('assets/tanks/tiger/baked/tiger_hull.glb');rows=[]
for name,root,x in [('engine_left_lock','Hull_0932',-.60617610833),('engine_right_lock','Hull_0933',.60812212388)]:
 data,node=source_data(D,B,root);v=data['vertices'][:,:3].copy();q=v[:,:2]-[x,2.06130580722];r=np.linalg.norm(q,axis=1);s=r<.0105;v[s,:2]=[x,2.06130580722]+q[s]*(.0094/.00972996);delta=np.linalg.norm(v-data['vertices'][:,:3],axis=1);attrs=data['vertices'].copy();attrs[:,:3]=v;t=data['triangles'];a=v[t];nn=np.cross(a[:,1]-a[:,0],a[:,2]-a[:,0]);nn/=np.linalg.norm(nn,axis=1)[:,None];expanded=attrs[t].copy();modified=np.any(s[t],axis=1);expanded[modified,:,3:6]=nn[modified,None,:];vv,ii=np.unique(expanded.reshape(-1,8).astype(np.float32),axis=0,return_inverse=True);out=dict(vertices=vv.astype(float),triangles=ii.reshape(-1,3),material=data['material']);stats=metrics(out);before=metrics(data);assert all(stats[k]==0 for k in ['boundary_edges','nonmanifold_edges','inconsistent_winding_edges','collapsed_edges','degenerate_triangles']),(name,stats);p=Path('artifacts/workshop-source-20261030/'+name+'_pin_candidate.glb');p.write_bytes(glb_bytes(name,out));rows.append(dict(name=name,root=root,node=node,source_metrics=before,metrics=stats,source_pin_radius_m=.00972996,derivative_pin_radius_m=.0094,bore_apothem_clearance_lower_bound_m=.0097299*np.cos(np.pi/24)-.0094001,maximum_vertex_displacement_m=float(delta.max()),removed_volume_m3=before['volume_m3']-stats['volume_m3'],sha256=hashlib.sha256(p.read_bytes()).hexdigest()));print(name,stats,flush=True)
Path('artifacts/workshop-source-20261030/engine_side_lock_candidates.json').write_text(json.dumps(rows,indent=2))
