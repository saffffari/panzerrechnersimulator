"""Package source-derived Tiger engine-access candidates without enabling gameplay.
Original source files are hash-checked and preserved. Existing different output bytes are refused.
Closed-mesh checks and explicit source-path limitations accompany every artifact.
"""
from pathlib import Path
import sys,json,hashlib,copy,numpy as np
ROOT=Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'tools'))
from fit_crew_steel_helmet import read_glb,accessor
from validate_tiger_hatches import matrix
from prepare_tiger_cooling import read_output,metrics,glb_bytes
SOURCE=ROOT/'assets/tanks/tiger/baked/tiger_hull.glb';SHA='51938f43eb90ba321ff452c2be953d06673f7aab670465835144cf8ce88c9a84'
TURRET=ROOT/'assets/tanks/tiger/baked/tiger_turret.glb';TURRET_SHA='20f86e9bacd1487869d42b59e40f86e0548af0a3626e57c250272b21f546e195'
OUT=ROOT/'assets/tanks/tiger/workshop_engine';CAT=ROOT/'catalog/tiger_engine_service.v1.json';STAGE=ROOT/'artifacts/workshop-source-20261030'

def source_data(doc,binary,root):
 i,node=next((i,n) for i,n in enumerate(doc['nodes']) if n.get('name')==root);p=doc['meshes'][node['mesh']]['primitives'][0];m=matrix(node);v=accessor(doc,binary,p['attributes']['POSITION'])@m[:3,:3].T+m[:3,3];n=accessor(doc,binary,p['attributes']['NORMAL'])@np.linalg.inv(m[:3,:3]);n/=np.linalg.norm(n,axis=1)[:,None]
 return dict(vertices=np.c_[v,n,accessor(doc,binary,p['attributes']['TEXCOORD_0'])],triangles=accessor(doc,binary,p['indices']).reshape(-1,3),material=copy.deepcopy(doc['materials'][p['material']])),i

def admit(name,data,provenance):
 stats=metrics(data);assert stats['volume_m3']>0 and stats['finite_attributes'];assert all(stats[k]==0 for k in ['boundary_edges','nonmanifold_edges','inconsistent_winding_edges','collapsed_edges','degenerate_triangles']),(name,stats)
 raw=glb_bytes(name,data);path=OUT/(name+'.glb');assert not path.exists() or path.read_bytes()==raw,'refusing changed derivative overwrite: '+str(path);path.write_bytes(raw)
 return dict(file=path.name,sha256=hashlib.sha256(raw).hexdigest(),metrics=stats,**provenance)

def contact(data,root,node):
 a=data['vertices'][data['triangles'],:3];cross=np.cross(a[:,1]-a[:,0],a[:,2]-a[:,0]);area=np.linalg.norm(cross,axis=1);normal=cross/np.maximum(area[:,None],1e-20);mid=a.mean(1);ok=np.where((normal[:,1]>.75)&(area>1e-7))[0];assert len(ok)
 target=(a.reshape(-1,3).min(0)+a.reshape(-1,3).max(0))/2;target[1]=mid[ok,1].max();i=min(ok,key=lambda j:np.linalg.norm(mid[j]-target))
 return dict(source_root=root,source_node=node,source_triangle=int(i),source_contact_hull_m=mid[i].tolist(),source_normal_hull=normal[i].tolist())

def main():
 if '--verify' in sys.argv:
  report=json.loads(CAT.read_text());assert hashlib.sha256(SOURCE.read_bytes()).hexdigest()==SHA;assert hashlib.sha256(TURRET.read_bytes()).hexdigest()==TURRET_SHA
  for f in report['files']:
   path=OUT/f['file'];assert hashlib.sha256(path.read_bytes()).hexdigest()==f['sha256'];s=metrics(read_output(path));assert all(s[k]==0 for k in ['boundary_edges','nonmanifold_edges','inconsistent_winding_edges','collapsed_edges','degenerate_triangles'])
  print('VERIFIED',len(report['files']),'source-derived engine access assets; service readiness:',report['service_enabled']);return
 doc,binary,raw=read_glb(str(SOURCE));assert hashlib.sha256(raw).hexdigest()==SHA;assert not doc.get('images') and not doc.get('textures');OUT.mkdir(parents=True,exist_ok=True)
 files=[];native={};sources={}
 for root in ['Hull_0895','Hull_0896','Hull_0899','Hull_0999','Hull_0930','Hull_0931','Hull_0932','Hull_0933']:
  data,node=source_data(doc,binary,root);sources[root]=(data,node);name='native_'+root.lower();files.append(admit(name,data,dict(source_root=root,source_node=node,derivative='none; source transform baked to hull metres')));native[root]=name+'.glb'
 for name,staged,root in [('engine_cover_leaf','engine_cover_knuckle_candidate','Hull_0975'),('engine_latch_base','engine_latch_base_candidate','Hull_1068'),('engine_latch_arm','engine_latch_arm_csg_candidate','Hull_1068'),('engine_left_lock','engine_left_lock_axial_candidate','Hull_0932'),('engine_right_lock','engine_right_lock_axial_candidate','Hull_0933')]:
  files.append(admit(name,read_output(STAGE/(staged+'.glb')),dict(source_root=root,derivative_receipt='artifacts/workshop-source-20261030/'+({'engine_cover_leaf':'engine_cover_knuckle_candidate.json','engine_latch_base':'engine_latch_split_candidate.json','engine_latch_arm':'engine_latch_arm_csg_candidate.json','engine_left_lock':'engine_side_lock_axial_candidates.json','engine_right_lock':'engine_side_lock_axial_candidates.json'}[name]))))
 lid_roots=['Hull_0975','Hull_0895','Hull_0896','Hull_0899','Hull_0999','Hull_0930','Hull_0931'];lid_files=['engine_cover_leaf.glb']+[native[n] for n in lid_roots[1:]]+['engine_latch_base.glb'];lid_contact=contact(*sources['Hull_0895'][:1],'Hull_0895',sources['Hull_0895'][1])
 assemblies=[dict(id='lid',role='lid',parent_frame='hull',parent_assembly=None,files=lid_files,source_roots=lid_roots,hinge_hull_m=[0,2.00876401746,-2.62762745377],axis_hull=[1,0,0],closed_angle_deg=0,open_angle_deg=-90,**lid_contact)]
 for name,root,sign,x in [('left_lock','Hull_0932',1,-.60617610833),('right_lock','Hull_0933',-1,.60812212388)]:
  source,node=sources[root];data=read_output(STAGE/('engine_'+name+'_axial_candidate.glb'));assemblies.append(dict(id=name,role=name,parent_frame='hull',parent_assembly='lid',files=['engine_'+name+'.glb'],source_roots=[root],hinge_hull_m=[x,2.06130580722,-1.74025518],axis_hull=[0,0,1],closed_angle_deg=0,open_angle_deg=sign*110,**contact(data,root,node)))
 rear,node=source_data(doc,binary,'Hull_1068');assemblies.append(dict(id='rear_latch',role='rear_latch',parent_frame='hull',parent_assembly='lid',files=['engine_latch_arm.glb'],source_roots=['Hull_1068'],hinge_hull_m=[-.2306,2.034063,-2.4393255],axis_hull=[1,0,0],closed_angle_deg=0,open_angle_deg=90,**contact(rear,'Hull_1068',node)))
 turret_doc,turret_bin,turret_raw=read_glb(str(TURRET));assert hashlib.sha256(turret_raw).hexdigest()==TURRET_SHA
 hardware=json.loads((STAGE/'engine_service_hardware_withdrawal_both_shoes.json').read_text());service_parts=[]
 for row in hardware:
  root=row['root'];data,node=source_data(turret_doc,turret_bin,root);name='native_'+root.lower()
  entry=admit(name,data,dict(source_root=root,source_node=node,parent_frame='turret',derivative='none; source transform baked to turret metres'));files.append(entry)
  first_delta=np.array(row['withdrawal'][0]['delta_turret_m']);axis=first_delta/np.linalg.norm(first_delta)
  part_id='carrier_plate' if root=='Turret_0132' else 'lower_retainer'
  service_parts.append(dict(id=part_id,source_root=root,source_node=node,file=entry['file'],parent_frame='turret',
   source_triangle=row['contact']['source_triangle'],source_contact_turret_m=row['contact']['contact_turret_m'],source_normal_turret=row['contact']['normal_turret'],
   source_mount_axis_turret=row['source_axis'],withdrawal_axis_turret=axis.tolist(),withdrawal_path_turret_m=[step['delta_turret_m'] for step in row['withdrawal']],
   mass_kg_estimate=entry['metrics']['volume_m3']*7850,mass_estimate_basis='Closed source solid volume multiplied by assumed steel density7850kg/m3; not a historical measured mass.',
   required_absent_source_roots=['Turret_0128','Turret_0147'] if root=='Turret_0132' else ['Turret_0147'],
   identity_retained=True,source_geometry_modified=False,
   withdrawal_validation='All source-vertex straight translation paths clear against current turret source with both spare shoes0128/0147 absent; full-face admission and ordered removal gameplay still required.',
   procedure_scope='Component-level release and measured withdrawal; no modeled thread-pitch or fastener mechanics claim.'))
 split=json.loads((STAGE/'engine_latch_split_candidate.json').read_text());head=json.loads((STAGE/'latch_head_clearance_candidate.json').read_text())
 final_latch_volume=next(f['metrics']['volume_m3'] for f in files if f['file']=='engine_latch_base.glb')+next(f['metrics']['volume_m3'] for f in files if f['file']=='engine_latch_arm.glb')
 report=dict(
  schema='panzer.tiger-engine-service.v1',asset_root='assets/tanks/tiger/workshop_engine/',service_enabled=False,
  source_hull='assets/tanks/tiger/baked/tiger_hull.glb',source_hull_sha256=SHA,
  source_turret='assets/tanks/tiger/baked/tiger_turret.glb',source_turret_sha256=TURRET_SHA,
  coordinate_frame='Assembly meshes use original hull metres. Service-part meshes use original turret metres. Every GLB node transform is identity. Lid children multiply parent hull pose, lid hinge transform, then child hinge transform. Turret-to-hull is translation(0,.524,.019) times Y-yaw rotation.',
  original_source_modified=False,removed_roots=dict(hull=lid_roots+['Hull_0932','Hull_0933','Hull_1068'],turret=['Turret_0132','Turret_0153']),
  fixed_replacements=[],assemblies=assemblies,service_parts=service_parts,files=files,
  unused_source_copies=['native_hull_0932.glb','native_hull_0933.glb'],
  prerequisites=[
   'Engine stopped; vehicle stationary.',
   'Physically remove and retain right spare shoesTurret_0128(front) andTurret_0147(rear), then carrier plateTurret_0132 and lower wing retainerTurret_0153. Never hide these source roots without equivalent finite native objects.',
   'Release both side locks and rear latch before raising lid.',
   'Candidate turret window64-68degrees right: runtime yaw-68 to-64degrees. Test actual source full-face swept admission before allowing motion.',
   'This source kit requires more removable hardware than the surveyed real-Tiger procedure; keep the discrepancy explicit.'
  ],
  sweep_validation=dict(
   method='CPU source-vertex trajectory rays,1degree rotation intervals; not a full-face or continuous solid sweep.',
   side_locks='All derivative vertices including the full length of the9.40mm-radius moving pins: zero hits over0 to+110 and0 to-110degrees against actual closed hull and fixed side blocks.',
   rear_latch='1461 moving vertices,0-90degrees,zero source hits against corrected lid/base and surrounding hull.',
   combined='11344 unique vertices of corrected leaf, both handles, attached covers, blocks, preopened locks, new latch base and preopened arm checked at yaw-64,-65,-66,-67,-68degrees. Only16 separating initial fitted-seam contacts under0.3micrometre; no other vertex-path hits. Actual0132/0153/0147 absent;0128 remained present for this lid check.',
   service_parts='0132:190 vertices,160mm opposite source stud axis.0153:675 vertices,60mm up then150mm outboard. Zero vertex-trajectory hits with both shoes0128/0147 absent.',
   runtime_full_face_admission_pending=True,
   receipts=['artifacts/workshop-source-20261030/engine_side_lock_motion_probe.json','artifacts/workshop-source-20261030/latch_head_clearance_motion_probe.json','artifacts/workshop-source-20261030/lid_service_removed_prerequisites_probe.json','artifacts/workshop-source-20261030/engine_service_hardware_withdrawal_both_shoes.json']),
  candidate_turret_yaw_window_deg=[-68,-64],
  authored_changes=dict(lid=json.loads((STAGE/'engine_cover_knuckle_candidate.json').read_text()),latch_split=split,latch_head=head,
   latch_head_csg=json.loads((STAGE/'engine_latch_arm_csg_candidate.json').read_text()),
   latch_final_combined_volume_m3=final_latch_volume,latch_final_removed_fraction=1-final_latch_volume/split['source_metrics']['volume_m3'],
   side_locks=json.loads((STAGE/'engine_side_lock_candidates.json').read_text()),
   side_lock_axial_clearance=json.loads((STAGE/'engine_side_lock_axial_candidates.json').read_text()),
   carrier_hardware='Native geometry preserved. Candidate relief removing38-82percent of carrier plate volume was rejected; physical removal is required.'),
  limits=[
   'Source preparation only. No engine exchange, component repair or playable opening claimed. service_enabled remains false pending runtime full-face admission and gameplay proof.',
   'Main engineHull0971 and its14-source-root assembly remain in original hull; no unverified lift is admitted.',
   'The original latch was a fused model part; its split journal/clevis and head reshaping are authored reconstructions with measured source-volume changes, not untouched historical geometry.',
   'The2original native side-lock copies are retained only for provenance; active moving assemblies use the pin-clearance derivatives. Do not render both.',
   'No new texture images: exact source material dictionaries and source UVs preserved.'
  ])
 CAT.write_text(json.dumps(report,indent=2)+'\n');print('PACKAGED',len(files),str(CAT))
if __name__=='__main__':main()
