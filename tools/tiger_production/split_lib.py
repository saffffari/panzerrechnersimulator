"""Cut one kit part into the pieces that move separately (owner, 2026-11-08: "the latch needs to be
openable and split from the handle object into two objects that properly move").

The kit's CAD parts are single fused solids: a handle and the strap round it share one skin, with
no surface where one passes inside the other. A split therefore leaves both pieces open along the
cut. `close_cuts` closes them the way the hidden surface must have run: two cut rings that face
each other along one axis with the same radius are bridged (a shaft carried on through a clamp, and
the clamp's bore round it); any other ring is filled flat. Named approximation: the bridged surface
is a straight run between the two rings, and a flat fill stands for whatever the fused joint hid.

Import inside Blender:  sys.path.append(<this dir>); import split_lib
"""
import math

import bmesh
import bpy
import numpy as np
from mathutils import Vector


def _rings(bm):
    """Boundary edges gathered into connected rings."""
    edges = [e for e in bm.edges if e.is_boundary]
    seen, rings = set(), []
    for e in edges:
        if e in seen:
            continue
        ring, stack = [], [e]
        seen.add(e)
        while stack:
            x = stack.pop()
            ring.append(x)
            for v in x.verts:
                for y in v.link_edges:
                    if y.is_boundary and y not in seen:
                        seen.add(y)
                        stack.append(y)
        rings.append(ring)
    return rings


def _ring_info(ring):
    pts = np.array([v.co[:] for v in {v for e in ring for v in e.verts}])
    c = pts.mean(0)
    _, s, vt = np.linalg.svd(pts - c)
    r = float(np.linalg.norm(pts - c, axis=1).mean())
    return {'c': c, 'n': vt[2], 'r': r, 'flat': float(s[2] / max(s[0], 1.0e-12)), 'count': len(pts)}


def close_cuts(bm):
    """Bridge facing coaxial rings, fill the rest. Returns (bridged pairs, filled rings, left open)."""
    rings = _rings(bm)
    info = [_ring_info(r) for r in rings]
    used, pairs = set(), []
    order = sorted(((float(np.linalg.norm(info[i]['c'] - info[j]['c'])), i, j)
                    for i in range(len(rings)) for j in range(i + 1, len(rings))))
    for dist, i, j in order:
        if i in used or j in used or dist < 1.0e-4:
            continue
        a, b = info[i], info[j]
        if abs(float(a['n'] @ b['n'])) < 0.95 or abs(a['r'] - b['r']) > 0.2 * max(a['r'], b['r']):
            continue
        d = (b['c'] - a['c']) / dist
        # a ring cut by a curved strap is a saddle, so its fitted normal leans a few degrees:
        # the line between the two centres has to run along both normals, within about 25 degrees
        if min(abs(float(d @ a['n'])), abs(float(d @ b['n']))) < 0.9:
            continue
        used.update((i, j))
        pairs.append((i, j))
    bridged = filled = 0
    for i, j in pairs:
        try:
            bmesh.ops.bridge_loops(bm, edges=rings[i] + rings[j])
            bridged += 1
        except Exception:
            used.discard(i)
            used.discard(j)
    for k, ring in enumerate(rings):
        if k in used:
            continue
        live = [e for e in ring if e.is_valid and e.is_boundary]
        if live:
            got = bmesh.ops.edgeloop_fill(bm, edges=live)
            filled += 1 if got.get('faces') else 0
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    still = sum(1 for e in bm.edges if e.is_boundary)
    return bridged, filled, still


def _finish(me):
    me.update()
    for p in me.polygons:
        p.use_smooth = True
    try:
        me.set_sharp_from_angle(angle=math.radians(30.0))
    except Exception:
        pass


def split(obj, pick, new_name, role_kept, role_new):
    """Move the faces `pick(face, obj)` chooses into a new object `new_name` beside `obj`; close both
    along the cut. Returns the new object and a report."""
    if bpy.data.objects.get(new_name) is not None:
        return bpy.data.objects[new_name], {'skipped': 'already split'}
    if obj.data.users > 1:
        obj.data = obj.data.copy()
    src = bmesh.new()
    src.from_mesh(obj.data)
    src.faces.ensure_lookup_table()
    chosen = {f.index for f in src.faces if pick(f, obj)}
    report = {'faces_total': len(src.faces), 'faces_moved': len(chosen)}
    if not chosen or len(chosen) == len(src.faces):
        src.free()
        report['skipped'] = 'the pick takes none or all of the faces'
        return None, report
    out = []
    for keep_chosen in (False, True):
        bm = src.copy()
        bm.faces.ensure_lookup_table()
        doomed = [f for f in bm.faces if (f.index in chosen) != keep_chosen]
        bmesh.ops.delete(bm, geom=doomed, context='FACES')
        bridged, filled, still = close_cuts(bm)
        out.append((bm, bridged, filled, still))
    src.free()
    new_me = obj.data.copy()
    new_me.name = new_name
    for (bm, bridged, filled, still), me, tag in ((out[0], obj.data, 'kept'), (out[1], new_me, 'new')):
        report[tag] = {'tris': sum(len(f.verts) - 2 for f in bm.faces), 'bridged': bridged, 'filled': filled, 'open_edges': still}
        bm.to_mesh(me)
        bm.free()
        _finish(me)
    new = obj.copy()
    new.name = new_name
    new.data = new_me
    for col in obj.users_collection:
        col.objects.link(new)
    for o, role, tag in ((obj, role_kept, 'kept'), (new, role_new, 'new')):
        o['split_role'] = role
        o['topo_tris'] = report[tag]['tris']
    new['split_from'] = obj.name
    obj['split_into'] = new_name
    return new, report


def by_material(*names):
    def pick(f, obj):
        m = obj.data.materials[f.material_index]
        return m is not None and m.name in names
    return pick


def world_verts(obj):
    co = np.empty(len(obj.data.vertices) * 3)
    obj.data.vertices.foreach_get('co', co)
    m = np.array(obj.matrix_world)
    return co.reshape(-1, 3) @ m[:3, :3].T + m[:3, 3]


def flat_axis(obj, materials=None):
    """The direction most of the part's area faces (both ways counted): the normal of a flat arm
    or plate. World space."""
    m3 = obj.matrix_world.to_3x3()
    best, rows = None, []
    for p in obj.data.polygons:
        if materials and obj.data.materials[p.material_index].name not in materials:
            continue
        rows.append(((m3 @ p.normal).normalized(), p.area))
    rows.sort(key=lambda r: -r[1])
    for n, _ in rows[:12]:
        score = sum(a for k, a in rows if abs(k.dot(n)) > 0.996)
        if best is None or score > best[0]:
            best = (score, n)
    return best[1]


def circle_centre(points, axis):
    """Least-squares circle through `points` projected on the plane across `axis` (world).
    Returns the centre (at the points' mean height along the axis) and the radius."""
    ax = np.array(Vector(axis).normalized())
    u = np.cross(ax, [1.0, 0.0, 0.0])
    if np.linalg.norm(u) < 0.1:
        u = np.cross(ax, [0.0, 1.0, 0.0])
    u /= np.linalg.norm(u)
    v = np.cross(ax, u)
    p = np.asarray(points)
    x, y, h = p @ u, p @ v, p @ ax
    a = np.c_[2.0 * x, 2.0 * y, np.ones(len(p))]
    sol, *_ = np.linalg.lstsq(a, x * x + y * y, rcond=None)
    cx, cy = sol[0], sol[1]
    r = math.sqrt(max(sol[2] + cx * cx + cy * cy, 0.0))
    return Vector(u * cx + v * cy + ax * h.mean()), r


def split_plane(obj, point, normal, new_name, role_kept, role_new):
    """Cut `obj` by the world plane (point, normal). The side the normal points to becomes the new
    object `new_name`; both sides are closed flat on the plane. Named approximation: a fused part's
    real parting line (a stepped jaw, say) is stood for by one plane."""
    if bpy.data.objects.get(new_name) is not None:
        return bpy.data.objects[new_name], {'skipped': 'already split'}
    if obj.data.users > 1:
        obj.data = obj.data.copy()
    inv = obj.matrix_world.inverted()
    co = inv @ Vector(point)
    no = (obj.matrix_world.to_3x3().transposed() @ Vector(normal)).normalized()
    report, made = {}, []
    for tag, inner, outer in (('kept', False, True), ('new', True, False)):
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], dist=1.0e-6, plane_co=co, plane_no=no,
                               clear_inner=inner, clear_outer=outer)
        bridged, filled, still = close_cuts(bm)
        report[tag] = {'tris': sum(len(f.verts) - 2 for f in bm.faces), 'filled': filled, 'bridged': bridged, 'open_edges': still}
        made.append(bm)
    new_me = obj.data.copy()
    new_me.name = new_name
    for bm, me in ((made[0], obj.data), (made[1], new_me)):
        bm.to_mesh(me)
        bm.free()
        _finish(me)
    new = obj.copy()
    new.name = new_name
    new.data = new_me
    for col in obj.users_collection:
        col.objects.link(new)
    for o, role, tag in ((obj, role_kept, 'kept'), (new, role_new, 'new')):
        o['split_role'] = role
        o['topo_tris'] = report[tag]['tris']
    new['split_from'] = obj.name
    obj['split_into'] = new_name
    return new, report


def join(into, piece):
    """Fuse `piece` into `into` (both in the same parent space): weld the seam and drop the pairs of
    flat closing faces that now lie face to face inside. `piece` is removed."""
    bm = bmesh.new()
    bm.from_mesh(into.data)
    rel = into.matrix_world.inverted() @ piece.matrix_world
    tmp = piece.data.copy()
    tmp.transform(rel)
    bm.from_mesh(tmp)
    bpy.data.meshes.remove(tmp)
    bmesh.ops.remove_doubles(bm, verts=bm.verts[:], dist=2.0e-5)
    seen, doomed = {}, []
    for f in bm.faces:
        key = frozenset(v.index for v in f.verts)
        if key in seen:
            doomed += [f, seen[key]]
        else:
            seen[key] = f
    if doomed:
        bmesh.ops.delete(bm, geom=list(set(doomed)), context='FACES')
    # closing faces cut differently on the two sides: any face whose every edge is shared by three
    # or more faces is an inside wall
    inner = [f for f in bm.faces if all(len(e.link_faces) > 2 for e in f.edges)]
    if inner:
        bmesh.ops.delete(bm, geom=inner, context='FACES')
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    report = {'tris': sum(len(f.verts) - 2 for f in bm.faces), 'open_edges': sum(1 for e in bm.edges if e.is_boundary),
              'nonmanifold_edges': sum(1 for e in bm.edges if not e.is_manifold), 'inside_faces_removed': len(set(doomed)) + len(inner)}
    bm.to_mesh(into.data)
    bm.free()
    _finish(into.data)
    into['topo_tris'] = report['tris']
    me = piece.data
    bpy.data.objects.remove(piece, do_unlink=True)
    if me.users == 0:
        bpy.data.meshes.remove(me)
    return report


def score(obj, point, normal):
    """Cut edges into `obj` along the world plane without parting it, so that a closing face made by
    an earlier cut ends where a later cut will meet it."""
    inv = obj.matrix_world.inverted()
    co = inv @ Vector(point)
    no = (obj.matrix_world.to_3x3().transposed() @ Vector(normal)).normalized()
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], dist=1.0e-6, plane_co=co, plane_no=no)
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()


def restore_from(obj, blend_path):
    """Put back `obj`'s mesh as it is in another file (the backup taken before a pass), keeping this
    file's materials. Clears the split marks."""
    name = obj.data.name
    with bpy.data.libraries.load(blend_path, link=False) as (src, dst):
        if name not in src.meshes:
            raise KeyError(name + ' is not in ' + blend_path)
        dst.meshes = [name]
    me = dst.meshes[0]
    for i, m in enumerate(me.materials):
        base = bpy.data.materials.get(m.name.split('.')[0])
        if base is not None and base is not m:
            me.materials[i] = base
    old = obj.data
    obj.data = me
    if old.users == 0:
        bpy.data.meshes.remove(old)
    me.name = name
    for m in list(bpy.data.materials):
        if m.users == 0 and '.' in m.name and bpy.data.materials.get(m.name.split('.')[0]) is not None:
            bpy.data.materials.remove(m)
    for k in ('split_role', 'split_into', 'split_from'):
        if k in obj:
            del obj[k]
    obj['topo_tris'] = sum(len(p.vertices) - 2 for p in me.polygons)
    return me
