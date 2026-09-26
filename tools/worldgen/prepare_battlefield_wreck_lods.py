"""Run in background Blender. Retain originals and materials; simplify only private wreck copies."""
import bpy
import hashlib
import json
from pathlib import Path
from mathutils import Vector

ROOT = Path('/home/alex/panzer/tank_simulator')
DEST = ROOT/'assets/authoring/battlefield'
DEST.mkdir(parents=True,exist_ok=True)
receipts=[]
for name,target in [('kv1',20000),('pz3',18000)]:
    source=ROOT/'assets/tanks/wrecks'/f'{name}.glb'
    dest=DEST/f'{name}_wreck_detail.glb'
    if dest.exists():
        raise RuntimeError(f'Preserving existing {dest}')
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(source))
    meshes=[o for o in bpy.context.scene.objects if o.type=='MESH']
    before=sum(sum(len(p.vertices)-2 for p in o.data.polygons) for o in meshes)
    ratio=min(1.0,target/before)
    for obj in meshes:
        bpy.context.view_layer.objects.active=obj
        obj.select_set(True)
        mod=obj.modifiers.new('Battlefield detail budget','DECIMATE')
        mod.ratio=ratio
        mod.use_collapse_triangulate=True
        bpy.ops.object.modifier_apply(modifier=mod.name)
        obj.select_set(False)
    after=sum(sum(len(p.vertices)-2 for p in o.data.polygons) for o in meshes)
    corners=[obj.matrix_world@Vector(c) for obj in meshes for c in obj.bound_box]
    lo=[min(c[i] for c in corners) for i in range(3)]
    hi=[max(c[i] for c in corners) for i in range(3)]
    bpy.ops.export_scene.gltf(filepath=str(dest),export_format='GLB',export_yup=True,
        export_animations=False,export_cameras=False,export_lights=False)
    receipts.append({'asset':str(dest.relative_to(ROOT)),'source':str(source.relative_to(ROOT)),
        'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),
        'derived_sha256':hashlib.sha256(dest.read_bytes()).hexdigest(),
        'source_triangles':before,'derived_triangles':after,'ratio':ratio,
        'blender_world_min':lo,'blender_world_max':hi,
        'operations':['Blender collapse decimation; retained materials and UVs','GLB export with no animation'],
        'note':'Static battlefield dressing. Original asset unchanged. Requires visual inspection.'})
(DEST/'wreck-detail.receipt.json').write_text(json.dumps(receipts,indent=2)+'\n')
print('WRECK_DETAIL_RECEIPT',json.dumps(receipts))
