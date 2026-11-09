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


def _fan():
    """25 unit directions over a hemisphere about +Z: the pole and three rings of eight."""
    out = [Vector((0.0, 0.0, 1.0))]
    for k, deg in enumerate((25.0, 50.0, 74.0)):
        a = math.radians(deg)
        for j in range(8):
            b = math.radians(45.0 * j + 22.5 * (k % 2))
            out.append(Vector((math.sin(a) * math.cos(b), math.sin(a) * math.sin(b), math.cos(a))))
    return out


def _tree():
    """One BVH of every visible mesh in world space (scene.ray_cast walks every object for every
    ray and is far too slow for millions of rays). Cached until the next file load."""
    import numpy as np
    from mathutils.bvhtree import BVHTree
    ns = bpy.app.driver_namespace
    if ns.get('material_fix_tree') is not None:
        return ns['material_fix_tree']
    vs, ts, base = [], [], 0
    for o in bpy.data.objects:
        if o.type != 'MESH' or not o.visible_get() or o.name.startswith(('HAND.', 'PLAYER.', 'TOOL.')):
            continue
        me = o.data
        me.calc_loop_triangles()
        if not me.loop_triangles:
            continue
        v = np.empty(len(me.vertices) * 3)
        me.vertices.foreach_get('co', v)
        t = np.empty(len(me.loop_triangles) * 3, dtype=np.int64)
        me.loop_triangles.foreach_get('vertices', t)
        m = np.array(o.matrix_world)
        vs.append(v.reshape(-1, 3) @ m[:3, :3].T + m[:3, 3])
        ts.append(t.reshape(-1, 3) + base)
        base += len(me.vertices)
    tree = BVHTree.FromPolygons(np.concatenate(vs).tolist(), np.concatenate(ts).tolist(), all_triangles=True)
    ns['material_fix_tree'] = tree
    return tree


def _seen(tree, c, n, fan):
    u = n.orthogonal().normalized()
    v = n.cross(u)
    start = c + n * 0.002
    for d in fan:
        if tree.ray_cast(start, (u * d.x + v * d.y + n * d.z).normalized(), REACH)[0] is None:
            return True
    return False


def run_fine(first=0, first_face=0, budget_s=35.0):
    """Finer pass for ivory only (2026-11-09, the owner: "still lots of ivory on the exterior"; it
    sat in seams, hatch rims and the mantlet ring, where fewer than two of five rays got out). A
    face painted ivory is exterior if ANY of 25 rays over its hemisphere leaves the tank, from
    either side of the face (a flipped normal must not hide it), from any object that shares its
    mesh. Named limits: an inside face seen through a vision slit or a gap round the gun goes grey
    too; red oxide keeps the coarser test above, so the engine bay under its grilles stays red.
    Resumable: call again with the returned next and next_face until next == count."""
    began = time.time()
    tree = _tree()
    ivory = INTERIOR[0]
    users = {}
    for o in sorted(bpy.data.objects, key=lambda o: o.name):
        if o.type == 'MESH' and o.visible_get() and not o.name.startswith(('FASTENER.', 'HAND.', 'PLAYER.', 'TOOL.')):
            if any(m is not None and m.name == ivory for m in o.data.materials):
                users.setdefault(o.data.name, []).append(o)
    todo = sorted(users)
    fan = _fan()
    i, k, changed, looked = first, first_face, 0, 0
    while i < len(todo) and time.time() - began < budget_s:
        objs = users[todo[i]]
        me = objs[0].data
        mats = [m.name if m is not None else '' for m in me.materials]
        frames = [(o.matrix_world.copy(), o.matrix_world.to_3x3()) for o in objs]
        grey = None
        polys = me.polygons
        while k < len(polys):
            if k % 256 == 0 and time.time() - began > budget_s:
                break
            p = polys[k]
            k += 1
            if mats[p.material_index] != ivory:
                continue
            looked += 1
            for mw, m3 in frames:
                c, n = mw @ p.center, (m3 @ p.normal).normalized()
                if _seen(tree, c, n, fan) or _seen(tree, c, -n, fan[:9]):
                    if grey is None:
                        grey = _slot(me, EXTERIOR)
                        mats = [m.name if m is not None else '' for m in me.materials]
                    p.material_index = grey
                    changed += 1
                    break
        me.update()
        if k >= len(polys):
            i, k = i + 1, 0
    return {'next': i, 'next_face': k, 'count': len(todo), 'faces_looked_at': looked, 'faces_repainted': changed,
            's': round(time.time() - began, 1)}


def fasteners_fine():
    """The same test for fasteners: one whose ivory faces can be seen from outside takes the
    exterior finish (the road wheel disc colour on a wheel) on every slot that held ivory."""
    tree = _tree()
    ivory, fan, changed = INTERIOR[0], _fan(), 0
    for o in bpy.data.objects:
        if o.type != 'MESH' or not o.name.startswith('FASTENER.') or not o.visible_get():
            continue
        slots = [i for i, s in enumerate(o.material_slots) if s.material is not None and s.material.name == ivory]
        if not slots:
            continue
        mw, m3 = o.matrix_world, o.matrix_world.to_3x3()
        if any(p.material_index in slots and _seen(tree, mw @ p.center, (m3 @ p.normal).normalized(), fan) for p in o.data.polygons):
            to = 'TG_roadwheel_disc_ral7021' if 'ROLLOR' in str(o.get('fastener_host', '')) else EXTERIOR
            for i in slots:
                o.material_slots[i].link = 'OBJECT'
                o.material_slots[i].material = bpy.data.materials[to]
            changed += 1
    return {'fasteners_repainted': changed}


def _tree_ids():
    """The world tree plus, per triangle, which mesh and polygon it came from."""
    import numpy as np
    from mathutils.bvhtree import BVHTree
    ns = bpy.app.driver_namespace
    if ns.get('material_fix_ids') is not None:
        return ns['material_fix_ids']
    vs, ts, meshes, tri_mesh, tri_poly, base = [], [], [], [], [], 0
    lo, hi = np.full(3, 1e9), np.full(3, -1e9)
    for o in bpy.data.objects:
        if o.type != 'MESH' or not o.visible_get() or o.name.startswith(('HAND.', 'PLAYER.', 'TOOL.')):
            continue
        me = o.data
        me.calc_loop_triangles()
        n = len(me.loop_triangles)
        if not n:
            continue
        v = np.empty(len(me.vertices) * 3)
        me.vertices.foreach_get('co', v)
        t = np.empty(n * 3, dtype=np.int64)
        me.loop_triangles.foreach_get('vertices', t)
        pi = np.empty(n, dtype=np.int64)
        me.loop_triangles.foreach_get('polygon_index', pi)
        m = np.array(o.matrix_world)
        w = v.reshape(-1, 3) @ m[:3, :3].T + m[:3, 3]
        if o.get('kit_name', o.name).startswith(('TG_', 'FASTENER.')):
            lo, hi = np.minimum(lo, w.min(0)), np.maximum(hi, w.max(0))
        vs.append(w)
        ts.append(t.reshape(-1, 3) + base)
        tri_mesh.append(np.full(n, len(meshes)))
        tri_poly.append(pi)
        meshes.append(o.name)
        base += len(me.vertices)
    tree = BVHTree.FromPolygons(np.concatenate(vs).tolist(), np.concatenate(ts).tolist(), all_triangles=True)
    ns['material_fix_ids'] = (tree, meshes, np.concatenate(tri_mesh), np.concatenate(tri_poly), lo, hi)
    return ns['material_fix_ids']


def run_views(first=0, views=72, step_m=0.014, budget_s=35.0):
    """What an eye outside the tank can see is exterior (2026-11-09). Parallel rays on a grid of
    step_m are sent at the tank from `views` directions spread over the sky and the horizon; the
    first face each ray meets, if painted ivory, takes the exterior finish (a fastener takes it on
    its ivory slots). This catches what the per-face test cannot: a big face whose middle is
    hidden, and an inside face showing through a gap between kit plates. Named limits: seams
    narrower than the step can be missed; an inside face seen through an open port goes grey.
    Resumable by view: call again with the returned next until next == views."""
    import numpy as np
    began = time.time()
    tree, meshes, tri_mesh, tri_poly, lo, hi = _tree_ids()
    centre, radius = (lo + hi) / 2.0, float(np.linalg.norm(hi - lo)) / 2.0 + 0.2
    ivory = INTERIOR[0]
    hits = {}
    i, rays = first, 0
    golden = math.pi * (3.0 - math.sqrt(5.0))
    while i < views and time.time() - began < budget_s:
        # directions from 12 degrees below the horizon to straight overhead, evenly over that cap
        z = -0.2 + 1.2 * (i + 0.5) / views
        r = math.sqrt(max(0.0, 1.0 - z * z))
        d = Vector((r * math.cos(golden * i), r * math.sin(golden * i), z))    # towards the eye
        u = d.orthogonal().normalized()
        v = d.cross(u)
        corners = np.array([[x, y, zz] for x in (lo[0], hi[0]) for y in (lo[1], hi[1]) for zz in (lo[2], hi[2])]) - centre
        pu, pv = corners @ np.array(u), corners @ np.array(v)
        c = Vector(centre) + d * (radius + 1.0)
        back = -d
        a = pu.min()
        while a <= pu.max():
            b = pv.min()
            while b <= pv.max():
                h = tree.ray_cast(c + u * a + v * b, back, 2.0 * radius + 2.0)
                if h[2] is not None:
                    hits.setdefault(int(tri_mesh[h[2]]), set()).add(int(tri_poly[h[2]]))
                rays += 1
                b += step_m
            a += step_m
        i += 1
    changed = fasteners = 0
    for mi, polys in hits.items():
        o = bpy.data.objects[meshes[mi]]
        me = o.data
        mats = [m.name if m is not None else '' for m in me.materials]
        if ivory not in mats:
            continue
        if o.name.startswith('FASTENER.'):
            slots = [k for k, s in enumerate(o.material_slots) if s.material is not None and s.material.name == ivory]
            if any(me.polygons[p].material_index in slots for p in polys):
                to = 'TG_roadwheel_disc_ral7021' if 'ROLLOR' in str(o.get('fastener_host', '')) else EXTERIOR
                for k in slots:
                    o.material_slots[k].link = 'OBJECT'
                    o.material_slots[k].material = bpy.data.materials[to]
                fasteners += 1
            continue
        grey = None
        for p in polys:
            if mats[me.polygons[p].material_index] == ivory:
                if grey is None:
                    grey = _slot(me, EXTERIOR)
                    mats = [m.name if m is not None else '' for m in me.materials]
                me.polygons[p].material_index = grey
                changed += 1
        me.update()
    return {'next': i, 'views': views, 'rays': rays, 'faces_repainted': changed, 'fasteners_repainted': fasteners,
            's': round(time.time() - began, 1)}
