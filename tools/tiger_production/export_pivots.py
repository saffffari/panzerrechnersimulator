"""Export the game master's non-mesh objects (pivots, lights, cameras) and every object's parent (run in Blender).
    TIGER_WORK=<dir> blender --background <master .blend> --python tools/tiger_production/export_pivots.py"""
import bpy, json, os
W = os.environ['TIGER_WORK']
rows = []
for o in bpy.data.objects:
    rows.append(dict(name=o.name, type=o.type, parent=o.parent.name if o.parent else None,
                     parent_type=o.parent_type, parent_bone=o.parent_bone,
                     matrix_world=[list(r) for r in o.matrix_world], empty_display_type=getattr(o, 'empty_display_type', None),
                     empty_display_size=getattr(o, 'empty_display_size', None),
                     data=o.data.name if o.data else None, props={k: str(o[k]) for k in o.keys() if not k.startswith('_')}))
json.dump(rows, open(os.path.join(W, 'master_objects.json'), 'w'))
print('PIVOTS', sum(r['type'] != 'MESH' for r in rows), 'OBJECTS', len(rows))
