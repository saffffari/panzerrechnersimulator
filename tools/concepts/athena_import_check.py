"""Independently import the emitted GLBs in a fresh Blender scene."""
import bpy, json, math
from pathlib import Path
from mathutils import Vector
ROOT=Path(__file__).resolve().parents[2]
BASE=ROOT/'assets/concepts/primary-accounts-20261107/athena'
checks=[]
for name in ['the-window-solnechnaya','water-under-fire']:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    folder=BASE/name
    source=json.loads((folder/'model.voxels.json').read_text())
    bpy.ops.import_scene.gltf(filepath=str(folder/'model.glb'))
    objects=[o for o in bpy.context.scene.objects if o.type=='MESH']
    positions=[o.matrix_world@v.co for o in objects for v in o.data.vertices]
    bounds=[[min(v[i]for v in positions),max(v[i]for v in positions)]for i in range(3)]
    expected=[[0,n*source['voxel_size_m']]for n in source['dimensions_cells']]
    assert all(abs(bounds[i][j]-expected[i][j])<1e-4 for i in range(3)for j in range(2)),(name,bounds,expected)
    assert {o.name for o in objects}==set(source['parts'])
    assert all(all(math.isfinite(v)for v in p)for p in positions)
    checks.append({'name':name,'passed':True,'importer':'Blender built-in glTF2','bounds_z_up_m':bounds,'objects':len(objects),'triangles':sum(len(o.data.polygons)for o in objects),'runtime_import_tested':False})
(BASE/'blender-import-validation.json').write_text(json.dumps({'blender_version':bpy.app.version_string,'assets':checks},indent=2)+'\n')
print('IMPORTED_AND_CHECKED',json.dumps(checks))
