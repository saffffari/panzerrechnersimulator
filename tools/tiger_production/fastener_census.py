"""Headless: read the heads (bolts, screws, rivets) off the full CAD of the named parts.

  blender --background tiger_gen2_live.pre-topology-20261108.blend --python fastener_census.py -- OUT.json KEY [KEY ...]

KEY is a piece of an object name ('28A-B69'), or ALL. One entry per mesh (parts sharing a mesh are
read once); rows are in the mesh's own space. See `census` in fasteners.py for the method.
"""
import json
import os
import sys
import time

import bmesh
import bpy

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import fasteners  # noqa: E402

args = sys.argv[sys.argv.index('--') + 1:]
out_path, keys = args[0], args[1:]
out, seen = {}, set()
for o in bpy.data.objects:
    if o.type != 'MESH' or o.data.name in seen:
        continue
    if 'ALL' not in keys and not any(k in o.name for k in keys):
        continue
    if '_WIRE_' in o.name:
        continue
    seen.add(o.data.name)
    began = time.time()
    bm = bmesh.new()
    bm.from_mesh(o.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts[:], dist=2.0e-5)
    rows = fasteners.census(bm)
    for r in rows:
        m = o.data.materials[r['material_index']] if o.data.materials else None
        r['material'] = m.name.split('.')[0] if m else ''
    faces = len(bm.faces)
    bm.free()
    out[o.data.name] = {'object': o.name, 'faces': faces, 'rows': rows}
    kinds = {}
    for r in rows:
        key = '%s %.0f mm' % (r['shape'], r['size'] * 1000.0)
        kinds[key] = kinds.get(key, 0) + 1
    print('[census]', o.name, 'faces', faces, 'heads', len(rows), kinds, '%.1f s' % (time.time() - began), flush=True)
json.dump(out, open(out_path, 'w'))
print('[census] wrote', out_path, 'meshes', len(out), 'heads', sum(len(v['rows']) for v in out.values()), flush=True)
