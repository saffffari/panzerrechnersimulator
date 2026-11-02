"""CPU-only authored detachable hinge/deck cassette, using unchanged original source geometry.
The 0.5mm assembly seam is explicit; both capped partitions remain visible/collidable.
No runtime source is edited and no extraction path is implied by a closed mesh.
"""
import sys,json,numpy as np,hashlib
from pathlib import Path
import bpy,bmesh
sys.path.insert(0,'tools');from prepare_tiger_engine_service import source_data,SHA
from fit_crew_steel_helmet import read_glb
from prepare_tiger_cooling import make_blender_source,blender_data,boolean,metrics,glb_bytes
D,B,raw=read_glb('assets/tanks/tiger/baked/tiger_hull.glb');assert hashlib.sha256(raw).hexdigest()==SHA;data,node=source_data(D,B,'Hull_0708');deck=make_blender_source('fixed_deck',data);cassette=make_blender_source('hinge_cassette',data)
def box(name,lo,hi):
 vs=[[x,y,z] for x in [lo[0],hi[0]] for y in [lo[1],hi[1]] for z in [lo[2],hi[2]]];fs=[(0,1,3,2),(4,6,7,5),(0,4,5,1),(2,3,7,6),(0,2,6,4),(1,5,7,3)];m=bpy.data.meshes.new(name);m.from_pydata(vs,[],fs);m.update();m.uv_layers.new(name='TEXCOORD_0');o=bpy.data.objects.new(name,m);bpy.context.collection.objects.link(o);return o
half=.00025;bounds=[[-.59,1.8,-2.80],[.59,2.2,-2.54]]
outer=box('outer_seam',[-.59-half,1.8,-2.80-half],[.59+half,2.2,-2.54+half])
inner=box('inner_seam',[-.59+half,1.9158394,-2.80+half],[.59-half,2.2,-2.54-half])
# The original rear cover1136 overhangs the strip. Retain its actual roof support footprint
# as a connected rear tongue, rather than cutting off or silently releasing its support.
boolean(outer,box('rear_cover_footprint_outer',[-.07-half,1.8,-3.04],[.46+half,2.2,-2.79]),'UNION')
boolean(inner,box('rear_cover_footprint_inner',[-.07+half,1.9158394,-3.04],[.46-half,2.2,-2.7905]),'UNION')
boolean(deck,outer,'DIFFERENCE');boolean(cassette,inner,'INTERSECT')
files=[];base=Path('artifacts/workshop-source-20261030')
for name,obj in [('engine_cassette_fixed_deck_candidate',deck),('engine_hinge_cassette_candidate',cassette)]:
 bm=bmesh.new();bm.from_mesh(obj.data);bmesh.ops.remove_doubles(bm,verts=list(bm.verts),dist=.000002);bmesh.ops.triangulate(bm,faces=list(bm.faces));bmesh.ops.dissolve_degenerate(bm,edges=list(bm.edges),dist=.000002);bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(obj.data);bm.free();a=blender_data(obj,data['material']);s=metrics(a);assert s['finite_attributes'] and s['volume_m3']>0;assert all(s[k]==0 for k in ['boundary_edges','nonmanifold_edges','inconsistent_winding_edges','collapsed_edges','degenerate_triangles']),(name,s);p=base/(name+'.glb');p.write_bytes(glb_bytes(name,a));files.append(dict(file=p.name,sha256=hashlib.sha256(p.read_bytes()).hexdigest(),metrics=s));print(name,s,flush=True)
before=metrics(data);report=dict(status='authored source-partition candidate; retained cassette supports/retainers and motion checks pending',source_root='Hull_0708',source_node=node,source_sha256=SHA,coordinate_frame='hull metres; identity GLB nodes',partition_bounds_hull_m=bounds,rear_support_tongue_bounds_hull_m=[[-.07,1.8,-3.04],[.46,2.2,-2.79]],retained_attached_roots=['Hull_1136'],seam_width_m=.0005,authored_cassette_underside_y_m=1.9158394,source_sidewall_support_max_y_m=1.9158387318,maximum_underside_relief_m=.000482764,source=before,files=files,removed_volume_m3=before['volume_m3']-sum(f['metrics']['volume_m3'] for f in files),removed_fraction=1-sum(f['metrics']['volume_m3'] for f in files)/before['volume_m3'],cassette_mass_kg_estimate=files[1]['metrics']['volume_m3']*7850,mass_basis='Source closed solid model volume times assumed steel7850kg/m3, not historical measured mass.',existing_lid_required_angle_deg=-100,initial_translation_candidate_hull_m=[0,.25,0],limits=['Authored major-service seam: not a historical Tiger manufacturing claim.','Fixed deck and removed cassette together preserve source surface outside the measured assembly seam; visible and collision meshes must match.','All lid hardware must remain attached to the finite cassette during hoist movement.','No invisible supports. Source-supported visible retainer/ledge geometry still required.'])
(base/'engine_hinge_cassette_candidate.json').write_text(json.dumps(report,indent=2));print('SEAM_REMOVED',report['removed_fraction'],'CASSETTE_KG',report['cassette_mass_kg_estimate'],flush=True)
