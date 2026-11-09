"""Second look at the paint split on the gen2 rig file: a face the air grid called interior but
which can be seen from outside is exterior.

materials.py decides inside from outside by probing an air grid taken from the July game master
(its shell seals; the kit's own does not). The kit's plates do not sit exactly where the game
master's do, so some outside faces probe into the master's solid and take the nearest air, which
can be the inside: ivory shards on a grey hull. This pass casts five rays from each face painted
as interior (ivory, red oxide) or blued gun steel: along its normal and four tilted 35 degrees. If
two or more leave the tank without hitting anything, the face is repainted with the exterior
finish. Named limits: a face seen from outside only through an open hatch or the engine grilles
also counts as outside; a mesh shared by several objects is judged from its first object.

    import material_fix; material_fix.run(first=0, budget_s=35)    # repeat from the returned index
"""
import math
import time

import bpy
from mathutils import Vector

INTERIOR = ('TG_paint_elfenbein_ral1001', 'TG_primer_red_oxide_ral8012')
EXTERIOR = 'TG_paint_dunkelgrau_ral7021'
SWAP = {'TG_gun_steel_blued': 'TG_gun_steel_painted'}
REACH = 40.0


def _slot(me, name):
    for i, m in enumerate(me.materials):
        if m is not None and m.name == name:
            return i
    me.materials.append(bpy.data.materials[name])
    return len(me.materials) - 1


def run(first=0, budget_s=35.0):
    sc = bpy.context.scene
    dg = bpy.context.evaluated_depsgraph_get()
    began = time.time()
    seen, todo = set(), []
    for o in sorted(bpy.data.objects, key=lambda o: o.name):
        if o.type != 'MESH' or o.name.startswith(('FASTENER.', 'HAND.', 'PLAYER.', 'TOOL.')) or o.data.name in seen:
            continue
        seen.add(o.data.name)
        names = {m.name for m in o.data.materials if m is not None}
        if names & set(INTERIOR) or (names & set(SWAP) and 'MG34' not in o.name and '_GUN' not in o.name):
            todo.append(o)
    i, changed, looked = first, 0, 0
    tilt = math.radians(35.0)
    while i < len(todo) and time.time() - began < budget_s:
        o = todo[i]
        me = o.data
        m3 = o.matrix_world.to_3x3()
        me.calc_loop_triangles()
        first_tri = {}
        for t in me.loop_triangles:
            first_tri.setdefault(t.polygon_index, t)
        mats = [m.name if m is not None else '' for m in me.materials]
        gun = 'MG34' not in o.name and '_GUN' not in o.name
        for p in me.polygons:
            name = mats[p.material_index]
            to = EXTERIOR if name in INTERIOR else (SWAP.get(name) if gun else None)
            if to is None:
                continue
            looked += 1
            t = first_tri[p.index]
            c = o.matrix_world @ ((me.vertices[t.vertices[0]].co + me.vertices[t.vertices[1]].co + me.vertices[t.vertices[2]].co) / 3.0)
            n = (m3 @ p.normal).normalized()
            u = n.orthogonal().normalized()
            v = n.cross(u)
            free = 0
            for d in (n, (n + u * math.tan(tilt)).normalized(), (n - u * math.tan(tilt)).normalized(), (n + v * math.tan(tilt)).normalized(), (n - v * math.tan(tilt)).normalized()):
                hit = sc.ray_cast(dg, c + n * 0.003, d, distance=REACH)[0]
                if not hit:
                    free += 1
            if free >= 2:
                p.material_index = _slot(me, to)
                changed += 1
        me.update()
        i += 1
    return {'next': i, 'count': len(todo), 'faces_looked_at': looked, 'faces_repainted': changed, 's': round(time.time() - began, 1)}
