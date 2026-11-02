"""Admit a measured vehicle-left fuel-filler service derivative; originals remain unchanged.
The CAD tank is a solid lump. Split its fused inner cap and bore a shallow receiving well;
all new cut faces are explicitly authored geometry, not a surveyed hollow tank interior.
A098.25mm-radius deck opening clears the097.30mm-radius source armored plug.
CPU Blender exact booleans only. No rendering, remeshing or decimation.
"""
from pathlib import Path
import sys,json,hashlib,subprocess,numpy as np
ROOT=Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'tools'))
from prepare_tiger_engine_service import source_data
from prepare_tiger_cooling import make_blender_source,boolean,blender_data,glb_bytes,metrics,read_output
from fit_crew_steel_helmet import read_glb
SOURCE=ROOT/'assets/tanks/tiger/baked/tiger_hull.glb'
SHA='51938f43eb90ba321ff452c2be953d06673f7aab670465835144cf8ce88c9a84'
OUT=ROOT/'assets/tanks/tiger/workshop_fuel';CAT=ROOT/'catalog/tiger_fuel_service.v1.json'
CENTER=np.array([1.14813453,1.934,-1.33648619]);RADIUS=.045;BOTTOM=1.805

def write(name,data):
 s=metrics(data);assert s['volume_m3']>0 and s['finite_attributes'];assert all(s[k]==0 for k in ['boundary_edges','nonmanifold_edges','inconsistent_winding_edges','collapsed_edges','degenerate_triangles']),(name,s)
 raw=glb_bytes(name,data);p=OUT/(name+'.glb');assert not p.exists() or p.read_bytes()==raw,'refusing changed admitted derivative '+str(p);p.write_bytes(raw)
 return dict(file=p.name,sha256=hashlib.sha256(raw).hexdigest(),metrics=s)

def main():
 if '--verify' in sys.argv:
  c=json.loads(CAT.read_text());assert hashlib.sha256(SOURCE.read_bytes()).hexdigest()==SHA
  for f in c['files']:assert hashlib.sha256((OUT/f['file']).read_bytes()).hexdigest()==f['sha256']
  print('VERIFIED fuel service source and',len(c['files']),'derivatives');return
 if '--blender' not in sys.argv:
  subprocess.run([r'C:\Program Files\Blender Foundation\Blender 5.1\blender.exe','--background','--factory-startup','--python',str(Path(__file__).resolve()),'--','--blender'],check=True);return
 import bpy
 from mathutils import Vector
 D,B,raw=read_glb(str(SOURCE));assert hashlib.sha256(raw).hexdigest()==SHA
 OUT.mkdir(parents=True,exist_ok=True)
 outer,_=source_data(D,B,'Hull_0875');tank,_=source_data(D,B,'Hull_0902');deck,_=source_data(D,B,'Hull_0708')
 # This horizontal split lies just below the source inner-cap disc (bottom1.937231m).
 bpy.ops.mesh.primitive_cube_add(size=2,location=(CENTER[0],2.434,CENTER[2]));cut=bpy.context.object;cut.scale=(.2,.5,.2);bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
 cap=make_blender_source('inner source cap',tank);body=make_blender_source('source fuel tank',tank)
 boolean(cap,cut,'INTERSECT');boolean(body,cut,'DIFFERENCE');bpy.data.objects.remove(cut,do_unlink=True)
 # The source tank has no hollow interior. A visible blind receiving neck exposes its
 # access without inventing a full internal shell or changing remote tank surfaces.
 bpy.ops.mesh.primitive_cylinder_add(vertices=128,radius=RADIUS,depth=2.10-BOTTOM,location=(CENTER[0],(2.10+BOTTOM)/2,CENTER[2]),rotation=(np.pi/2,0,0));well=bpy.context.object
 boolean(body,well,'DIFFERENCE');bpy.data.objects.remove(well,do_unlink=True)
 import bmesh
 for obj in [cap,body]:
  tol=0.00001 if obj==cap else 0.0000004
  bm=bmesh.new();bm.from_mesh(obj.data);bmesh.ops.triangulate(bm,faces=list(bm.faces));bmesh.ops.remove_doubles(bm,verts=list(bm.verts),dist=tol);bmesh.ops.dissolve_degenerate(bm,dist=tol,edges=list(bm.edges));boundary=[e for e in bm.edges if e.is_boundary];print('BOUNDARY',obj.name,len(boundary),sum(e.calc_length() for e in boundary))
  if boundary:
   assert sum(e.calc_length() for e in boundary)<.02,'unexpected source seam opening';bmesh.ops.holes_fill(bm,edges=boundary,sides=0)
  bm.to_mesh(obj.data);bm.free();obj.data.update()
 cap_data=blender_data(cap,tank['material']);body_data=blender_data(body,tank['material'])
 grille=make_blender_source('source deck with plug clearance',deck)
 bpy.ops.mesh.primitive_cylinder_add(vertices=256,radius=.09825,depth=2.20-2.0259,location=(1.14618853,(2.20+2.0259)/2,-1.34911114),rotation=(np.pi/2,0,0));clearance=bpy.context.object
 boolean(grille,clearance,'DIFFERENCE');bpy.data.objects.remove(clearance,do_unlink=True)
 # The model's lower throat is only105mm across, but the eccentric inner cap needs
 #138.4mm clearance about the outer plug axis. Make that passage visibly140.6mm.
 bpy.ops.mesh.primitive_cylinder_add(vertices=256,radius=.0703,depth=.20,location=(1.14618853,2.0,-1.34911114),rotation=(np.pi/2,0,0));throat=bpy.context.object
 boolean(grille,throat,'DIFFERENCE');bpy.data.objects.remove(throat,do_unlink=True)
 bm=bmesh.new();bm.from_mesh(grille.data);bmesh.ops.triangulate(bm,faces=list(bm.faces));bmesh.ops.remove_doubles(bm,verts=list(bm.verts),dist=.000001);bmesh.ops.dissolve_degenerate(bm,dist=.000001,edges=list(bm.edges));bm.to_mesh(grille.data);bm.free();grille.data.update()
 grille_data=blender_data(grille,deck['material'])
 files=[write('left_outer_plug',outer),write('left_inner_cap',cap_data),write('left_tank_open_neck',body_data),write('deck_filler_clearance_v3',grille_data)]
 source_s=metrics(tank);s=metrics(body_data);cs=metrics(cap_data)
 report=dict(schema='panzer.tiger-fuel-service.v1',source=str(SOURCE.relative_to(ROOT)).replace('\\','/'),source_sha256=SHA,original_modified=False,
 removed_roots=['Hull_0875','Hull_0902','Hull_0708'],asset_root='assets/tanks/tiger/workshop_fuel/',files=files,
 receiver_center_hull_m=CENTER.tolist(),receiver_radius_m=RADIUS,receiver_bottom_hull_m=BOTTOM,
 outer_center_hull_m=[1.14618853,2.0340619,-1.34911114],inner_center_hull_m=[CENTER[0],1.9434,CENTER[2]],
 source_deck_metrics=metrics(deck),deck_clearance_removed_m3=metrics(deck)['volume_m3']-metrics(grille_data)['volume_m3'],outer_bore_radius_m=.09825,lower_throat_radius_m=.0703,source_tank_metrics=source_s,removed_well_volume_m3=source_s['volume_m3']-s['volume_m3']-cs['volume_m3'],
 source_caps='Outer armored plug is untouched source geometry. Inner cap split at y1.934m from fused source fuel tank.',
 authored_changes='128-sided45mm-radius blind receiving well to y1.805m; underside split closure; cap cleanup tolerance10micrometres, tank0.4micrometres; nine original/boolean seam edges totaling4.671mm closed. No hidden hole or invisible refill target. Exact derivative triangles must be rendered and used for source contacts.',
 limits=['Only vehicle-left filler is serviceable in this slice; right source filler remains unchanged.',
 'Tank groups are treated as connected with their valves open; no separate fuel valve or reserve selection simulation.',
 'Existing gameplay capacity540L remains unchanged; February1943 chassis description states534L. No historical capacity recalibration is claimed.',
 'Receiving well is a source-derived gameplay reconstruction, not a full fuel-tank interior, liquid-volume simulation or measured neck bore.',
 'Cap removal/return follows a guided physical path; thread pitch, detached-cap free dynamics and seal leakage are not simulated.'],
 references=[dict(title='1943 Tiger chassis draft D656/21 printed39-40',url='https://bushmakow.com/wp-content/uploads/2024/05/155.-D-656_21-TIGER-compressed_bushmakow.pdf'),dict(title='Original Tiger131 upper fuel tank survey by DavidByrden',url='https://tiger1.info/EN/Upper-fuel-tanks.html')])
 CAT.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8',newline='\n');print('ADMITTED',json.dumps(dict(files=len(files),well_volume_m3=report['removed_well_volume_m3'])))
if __name__=='__main__':main()
