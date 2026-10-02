"""Export the exact source fitting wrongly excluded by the old long/thin cartridge filter.
Run Blender --background <source.blend> --python tools/restore_tiger_rack_fitting.py.
No source asset is saved/edited. The additive GLB uses the existing baked hull frame.
"""
import bpy,json,struct,hashlib
from pathlib import Path
from mathutils import Vector,Matrix
root=Path(__file__).resolve().parents[1]
baked=root/'assets/tanks/tiger/baked'
raw=(baked/'tiger_hull.glb').read_bytes();n=struct.unpack_from('<I',raw,12)[0];g=json.loads(raw[20:20+n])
# The current export retained each source tray as a scene root. Its translation gives the exact
# same hull origin without recomputing a bounding box from a changed export selection.
anchor=next(x for x in g['nodes'] if x.get('name')=='Hull_0559')
assert 'matrix' not in anchor
eng=lambda p:Vector((p.x,p.z,-p.y))
inv=lambda p:Vector((p.x,-p.z,p.y))
origin=inv(eng(bpy.data.objects['Hull_0559'].matrix_world.translation)-Vector(anchor.get('translation',[0,0,0])))
o=bpy.data.objects['Turret_0125'];assert abs(max(o.dimensions)-1.9865743)<1e-5
assert not any(x.get('name')=='Turret_0125' for x in g['nodes'])
copy=o.copy();copy.data=o.data.copy();copy.parent=None
copy.matrix_world=Matrix.Translation(-origin)@o.matrix_world
bpy.context.collection.objects.link(copy)
bpy.ops.object.select_all(action='DESELECT');copy.select_set(True);bpy.context.view_layer.objects.active=copy
# Preserve source identity in the exported root, while the in-memory source keeps its own name.
o.name='SOURCE_NOT_EXPORTED_Turret_0125';copy.name='Turret_0125'
path=baked/'tiger_rack_fitting.glb'
bpy.ops.export_scene.gltf(filepath=str(path),use_selection=True,export_format='GLB',export_yup=True,export_animations=False)
copy.data.calc_loop_triangles()
rows=[]
col=bpy.data.collections['Shells'];legacy=0
for obj in col.all_objects:
 if obj.type!='MESH':continue
 dims=sorted(obj.dimensions)
 if not(dims[2]>.7 and dims[1]<.25):continue
 name='Turret_0125' if obj==o else obj.name
 rows.append({'source_id':legacy,'object':name,'dimensions_m':list(obj.dimensions),'is_cartridge':name!='Turret_0125'})
 legacy+=1
assert len(rows)==65 and rows[32]['object']=='Turret_0125'
receipt={'source_blend':'assets/tanks/tiger/Panzerkampfwagen_VI_Tiger_Ausf.E.blend','source_blend_sha256':hashlib.sha256(Path(bpy.data.filepath).read_bytes()).hexdigest(),'original_hull_sha256':hashlib.sha256(raw).hexdigest(),'restored_object':'Turret_0125','hull_origin_blender_m':list(origin),'source_vertices':len(copy.data.vertices),'source_triangles':len(copy.data.loop_triangles),'source_modifiers':len(copy.modifiers),'attachment':'hull','reason':'source fitting lies over fixed sponson racks; exclude only actual cartridges from static hull','cartridge_count':64,'source_slots':rows}
(baked/'tiger_rack_source.json').write_text(json.dumps(receipt,indent=2)+'\n')
print('RESTORED_RACK_FITTING='+json.dumps({k:v for k,v in receipt.items() if k!='source_slots'}))
