"""Headless: reduce the kit's three cables from the pre-topology backup and write them to a small
library file, so the open session is not blocked. Interim meshes: the cables are to be rebuilt as
rope curves with end eyes (docs/TIGER_DESTRUCTIBLES_20261108.md, F2).

  blender --background tiger_gen2_live.pre-topology-20261108.blend --python topology_wires.py -- OUT.blend TOL_M
"""
import os
import sys
import time

import bpy

here = os.path.dirname(os.path.abspath(__file__))
exec(open(os.path.join(here, 'topology.py')).read())
argv = sys.argv[sys.argv.index('--') + 1:]
out, tol = argv[0], float(argv[1])
done = []
for o in [o for o in bpy.context.scene.objects if o.type == 'MESH' and '_WIRE_' in o.name]:
    began = time.time()
    r = reduce_mesh(o, tol_m=tol, apply=True, search=7)
    print('[wires]', o.name, r['src_tris'], '->', r['tris'], 'dev', r['dev_mm'], 'mm applied', r['applied'],
          'open edges', r['boundary_edges'], 'non-manifold', r['nonmanifold_edges'], '%.0f s' % (time.time() - began), flush=True)
    done.append(o)
bpy.data.libraries.write(out, set(done), fake_user=True)
print('[wires] wrote', out, flush=True)
