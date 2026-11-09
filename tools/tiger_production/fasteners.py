"""Rivets and other small heads moulded into a part become separate fastener objects (owner,
2026-11-08: the turret bin's rivets "we can honestly convert to simple cubes or remove entirely";
"lot of screws on this model that will need to be properly integrated as screw objects").

A head is found on the reduced mesh as a connected patch of small faces, closed by one ring of
edges onto larger host faces, round in plan, standing proud of the host. The patch is cut out, the
ring filled flat, and an object is placed there: one shared cube mesh, its +Z along the host's
normal, scaled to the head's plan and height, parented to the host and kept in the FASTENERS
collection with `fastener_kind`, `fastener_radius_m` and `fastener_height_m`. Named
approximation: the cube stands for a domed head; the host's surface under it is flat.

Run inside Blender:  exec(open(PATH).read());  heads_to_objects(obj, kind='rivet')
"""
import math

import bmesh
import bpy
from mathutils import Matrix, Vector

SMALL_FACE_M2 = 1.5e-4
PLANAR_DEG = 0.5


def _collection():
    col = bpy.data.collections.get('FASTENERS')
    if col is None:
        col = bpy.data.collections.new('FASTENERS')
        bpy.context.scene.collection.children.link(col)
    return col


def _cube_mesh():
    me = bpy.data.meshes.get('FASTENER.cube')
    if me is None:
        me = bpy.data.meshes.new('FASTENER.cube')
        bm = bmesh.new()
        bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation((0.0, 0.0, 0.5)))
        bm.to_mesh(me)
        bm.free()
    return me


def find_heads(bm, d_min=0.010, d_max=0.026, round_tol=0.3):
    """Patches of small faces that read as a round head. Returns dicts with the patch's faces, its
    ring of boundary edges, base centre, outward normal, plan radius and height (object space)."""
    bm.faces.ensure_lookup_table()
    small = {f.index for f in bm.faces if f.calc_area() < SMALL_FACE_M2}
    seen, heads, rejected = set(), [], 0
    for start in small:
        if start in seen:
            continue
        stack, patch = [start], []
        seen.add(start)
        while stack:
            j = stack.pop()
            patch.append(j)
            for e in bm.faces[j].edges:
                for g in e.link_faces:
                    if g.index in small and g.index not in seen:
                        seen.add(g.index)
                        stack.append(g.index)
        in_patch = set(patch)
        ring, hosts = [], []
        for j in patch:
            for e in bm.faces[j].edges:
                outside = [g for g in e.link_faces if g.index not in in_patch]
                if outside:
                    ring.append(e)
                    hosts.extend(outside)
        if not ring or not hosts:
            rejected += 1
            continue
        ring_verts = {v for e in ring for v in e.verts}
        # one closed ring: every ring vertex meets exactly two ring edges
        count = {}
        for e in ring:
            for v in e.verts:
                count[v] = count.get(v, 0) + 1
        if any(c != 2 for c in count.values()):
            rejected += 1
            continue
        base = sum((v.co for v in ring_verts), Vector()) / len(ring_verts)
        normal = sum((h.normal * h.calc_area() for h in hosts), Vector())
        if normal.length < 1.0e-9:
            rejected += 1
            continue
        normal.normalize()
        radii = [((v.co - base) - normal * (v.co - base).dot(normal)).length for v in ring_verts]
        radius = sum(radii) / len(radii)
        spread = (max(radii) - min(radii)) / max(radius, 1.0e-9)
        patch_verts = {v for j in patch for v in bm.faces[j].verts}
        height = max((v.co - base).dot(normal) for v in patch_verts)
        low = min((v.co - base).dot(normal) for v in patch_verts)
        if not (d_min <= 2.0 * radius <= d_max) or spread > round_tol or height < 0.0008 or low < -0.001:
            rejected += 1
            continue
        heads.append({'faces': patch, 'ring': ring, 'base': base, 'normal': normal, 'radius': radius, 'height': height})
    return heads, rejected


def heads_to_objects(obj, kind='rivet', mode='cube', d_min=0.010, d_max=0.026):
    """Cut the round heads out of `obj` and (mode 'cube') stand a fastener object on each; mode
    'remove' only cuts them out. Returns counts and the triangle totals before and after."""
    me = obj.data
    bm = bmesh.new()
    bm.from_mesh(me)
    before = sum(len(f.verts) - 2 for f in bm.faces)
    heads, rejected = find_heads(bm, d_min, d_max)
    placed = [(h['base'].copy(), h['normal'].copy(), h['radius'], h['height']) for h in heads]
    doomed = {bm.faces[j] for h in heads for j in h['faces']}
    rings = [e for h in heads for e in h['ring']]
    bmesh.ops.delete(bm, geom=list(doomed), context='FACES')
    rings = [e for e in rings if e.is_valid]
    bmesh.ops.edgeloop_fill(bm, edges=rings)
    bmesh.ops.dissolve_limit(bm, angle_limit=math.radians(PLANAR_DEG), verts=bm.verts[:], edges=bm.edges[:],
                             use_dissolve_boundaries=False, delimit={'MATERIAL'})
    loose = [v for v in bm.verts if not v.link_faces]
    if loose:
        bmesh.ops.delete(bm, geom=loose, context='VERTS')
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    after = sum(len(f.verts) - 2 for f in bm.faces)
    open_edges = sum(1 for e in bm.edges if e.is_boundary)
    nonmanifold = sum(1 for e in bm.edges if not e.is_manifold)
    bm.to_mesh(me)
    bm.free()
    me.update()
    for p in me.polygons:
        p.use_smooth = True
    try:
        me.set_sharp_from_angle(angle=math.radians(30.0))
    except Exception:
        pass
    made = 0
    if mode == 'cube':
        col, cube = _collection(), _cube_mesh()
        stem = obj.name.split('_')[1] if '_' in obj.name else obj.name
        for i, (base, normal, radius, height) in enumerate(placed):
            ob = bpy.data.objects.new('FASTENER.%s.%s.%03d' % (kind, stem, i), cube)
            col.objects.link(ob)
            ob.parent = obj
            rot = normal.to_track_quat('Z', 'Y').to_matrix().to_4x4()
            side = radius * 1.6
            ob.matrix_parent_inverse = Matrix.Identity(4)
            ob.matrix_local = Matrix.Translation(base) @ rot @ Matrix.Diagonal((side, side, max(height, 0.002), 1.0))
            ob['fastener_kind'] = kind
            ob['fastener_radius_m'] = radius
            ob['fastener_height_m'] = height
            ob['fastener_host'] = obj.name
            if me.materials:
                ob.color = (0.2, 0.2, 0.2, 1.0)
            made += 1
    obj['fasteners_cut'] = len(placed)
    if obj.get('topo_tris') is not None:
        obj['topo_tris'] = after
    return {'object': obj.name, 'heads': len(placed), 'rejected_patches': rejected, 'objects_made': made,
            'tris_before': before, 'tris_after': after, 'open_edges': open_edges, 'nonmanifold_edges': nonmanifold}


# ---------------------------------------------------------------------------------------------
# Screw objects from the full CAD (owner, 2026-11-08: "lot of screws on this model that will need
# to be properly integrated as screw objects").
#
# `find_heads` above reads the reduced mesh, where a 1 mm limit can turn an 11 mm round head into a
# hexagon, so it cannot tell a bolt from a rivet. `census` reads the full CAD mesh instead: a head
# is a small island of surface that meets its host along one closed concave edge ring and stands
# proud of it (a washer under a head is part of the island). Its plan outline gives the shape: six
# corners is a hex head, four a square, anything else round. `apply_rows` then cuts each head out
# of the reduced mesh, fills the ring, and stands a fastener object there: its origin on the seat,
# local +Z out of the host (the way it comes out), scaled to the measured head.
#
# Named approximations: the object is a plain prism for the head, with no shank or thread; the
# thread size is a guess from the across-flats by the DIN 931 table (`fastener_thread_guess`),
# since the kit models no shank; whether a round head is a rivet or a screw is the caller's call
# per part and is recorded as `fastener_kind`.

DIN_AF_MM = {7: 'M4', 8: 'M5', 10: 'M6', 13: 'M8', 17: 'M10', 19: 'M12', 22: 'M14', 24: 'M16', 27: 'M18', 30: 'M20',
             32: 'M22', 36: 'M24', 41: 'M27', 46: 'M30'}


def _hull_corners(xy, tol_deg=14.0):
    """Corner count of the convex hull of 2-D points, runs of nearly collinear points merged."""
    pts = sorted(set((round(float(x), 6), round(float(y), 6)) for x, y in xy))
    if len(pts) < 3:
        return len(pts), pts, pts

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    lower, upper = [], []
    for p in pts:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], p) <= 0:
            lower.pop()
        lower.append(p)
    for p in reversed(pts):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], p) <= 0:
            upper.pop()
        upper.append(p)
    hull = lower[:-1] + upper[:-1]
    keep = []
    n = len(hull)
    for i in range(n):
        a, b, c = hull[i - 1], hull[i], hull[(i + 1) % n]
        u = Vector((b[0] - a[0], b[1] - a[1]))
        v = Vector((c[0] - b[0], c[1] - b[1]))
        if u.length < 1.0e-7 or v.length < 1.0e-7:
            continue
        if math.degrees(u.angle(v)) > tol_deg:
            keep.append(b)
    return len(keep), keep, hull


def census(bm, d_min=0.006, d_max=0.045, min_height=0.0015):
    """Heads on a full-detail mesh. Rows (object space): base, normal, ring radius, height, shape
    ('hex', 'square', 'round'), head size (across flats for hex and square, diameter for round)."""
    bm.faces.ensure_lookup_table()
    bm.normal_update()
    nf = len(bm.faces)
    cent = [f.calc_center_median() for f in bm.faces]
    concave = set()
    for e in bm.edges:
        if len(e.link_faces) != 2:
            continue
        a, b = e.link_faces
        if a.normal.dot(b.normal) > 0.94:
            continue
        mid = (e.verts[0].co + e.verts[1].co) * 0.5
        if a.normal.dot(cent[b.index] - mid) > 1.0e-7 and b.normal.dot(cent[a.index] - mid) > 1.0e-7:
            concave.add(e.index)
    region = [-1] * nf
    regions = []
    for f in bm.faces:
        if region[f.index] != -1:
            continue
        k = len(regions)
        region[f.index] = k
        stack, members = [f], []
        while stack:
            x = stack.pop()
            members.append(x.index)
            for e in x.edges:
                if e.index in concave or len(e.link_faces) != 2:
                    continue
                for g in e.link_faces:
                    if region[g.index] == -1:
                        region[g.index] = k
                        stack.append(g)
        regions.append(members)
    limit = d_max * 1.6
    small = []
    for members in regions:
        lo = Vector((1e9, 1e9, 1e9))
        hi = -lo
        for j in members:
            for v in bm.faces[j].verts:
                for i in range(3):
                    lo[i] = min(lo[i], v.co[i])
                    hi[i] = max(hi[i], v.co[i])
        small.append((hi - lo).length < limit)
    # islands: small regions joined across concave edges
    island = {}
    rows = []
    for k, members in enumerate(regions):
        if not small[k] or k in island:
            continue
        group, stack = [], [k]
        island[k] = k
        while stack:
            r = stack.pop()
            group.append(r)
            for j in regions[r]:
                for e in bm.faces[j].edges:
                    for g in e.link_faces:
                        q = region[g.index]
                        if small[q] and q not in island:
                            island[q] = k
                            stack.append(q)
        inside = {j for r in group for j in regions[r]}
        ring, hosts = [], []
        ok = True
        for j in inside:
            for e in bm.faces[j].edges:
                if len(e.link_faces) != 2:
                    ok = False
                    continue
                out = [g for g in e.link_faces if g.index not in inside]
                if out:
                    ring.append(e)
                    hosts.extend(out)
        if not ok or not ring:
            continue
        count = {}
        for e in ring:
            for v in e.verts:
                count[v] = count.get(v, 0) + 1
        if any(c != 2 for c in count.values()):
            continue
        ring_verts = list(count)
        base = sum((v.co for v in ring_verts), Vector()) / len(ring_verts)
        normal = sum((h.normal * h.calc_area() for h in hosts), Vector())
        if normal.length < 1.0e-12:
            continue
        normal.normalize()
        verts = {v for j in inside for v in bm.faces[j].verts}
        hs = [(v.co - base).dot(normal) for v in verts]
        height, low = max(hs), min(hs)
        radii = [((v.co - base) - normal * (v.co - base).dot(normal)).length for v in ring_verts]
        radius = sum(radii) / len(radii)
        if height < min_height or low < -0.001 or not (d_min <= 2.0 * radius <= d_max):
            continue
        if (max(radii) - min(radii)) / radius > 0.45:
            continue
        u = normal.orthogonal().normalized()
        w = normal.cross(u)
        top = [v.co - base for v in verts if (v.co - base).dot(normal) > 0.5 * height]
        corners, pts, _ = _hull_corners([(p.dot(u), p.dot(w)) for p in top])
        r_top = sum(math.hypot(x, y) for x, y in pts) / max(len(pts), 1)
        if corners == 6:
            shape, size = 'hex', 2.0 * r_top * math.cos(math.radians(30.0))
        elif corners == 4:
            shape, size = 'square', 2.0 * r_top * math.cos(math.radians(45.0))
        else:
            # a round head's size is its widest plan diameter anywhere above the seat
            proud = [v.co - base for v in verts if (v.co - base).dot(normal) > 0.0003]
            shape = 'round'
            size = 2.0 * max(((p - normal * p.dot(normal)).length for p in proud), default=radius)
        mats = {}
        for j in inside:
            mats[bm.faces[j].material_index] = mats.get(bm.faces[j].material_index, 0) + 1
        rows.append({'base': list(base), 'normal': list(normal), 'ring_radius': radius, 'height': height, 'shape': shape,
                     'size': size, 'corners': corners, 'material_index': max(mats, key=mats.get), 'faces': len(inside)})
    return rows


def _shape_mesh(shape):
    name = 'FASTENER.' + shape
    me = bpy.data.meshes.get(name)
    if me is not None:
        return me
    if shape == 'cube':
        return _cube_mesh()
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    if shape == 'hex':
        # unit across flats: circumradius 1 / (2 cos 30)
        bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=0.5 / math.cos(math.radians(30.0)),
                              radius2=0.5 / math.cos(math.radians(30.0)), depth=1.0, matrix=Matrix.Translation((0.0, 0.0, 0.5)))
    elif shape == 'square':
        bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation((0.0, 0.0, 0.5)))
    else:
        bmesh.ops.create_cone(bm, cap_ends=True, segments=8, radius1=0.5, radius2=0.5, depth=1.0,
                              matrix=Matrix.Translation((0.0, 0.0, 0.5)))
    bm.to_mesh(me)
    bm.free()
    return me


def thread_guess(across_flats_m):
    mm = across_flats_m * 1000.0
    af = min(DIN_AF_MM, key=lambda k: abs(k - mm))
    return DIN_AF_MM[af] if abs(af - mm) <= 1.6 else ''


def apply_rows(obj, rows, kind_of, set_name='', cut=True):
    """Cut the heads in `rows` (object space, from `census` on the full CAD of the same part) out
    of `obj`'s reduced mesh and stand a fastener object on each. `kind_of(row)` returns
    (kind, removable) or None to leave that head moulded in. With cut=False the mesh is left
    alone and only the objects are placed (a second object sharing an already cut mesh)."""
    me = obj.data
    todo = [(r, kind_of(r)) for r in rows]
    todo = [(r, k) for r, k in todo if k is not None]
    report = {'object': obj.name, 'rows': len(rows), 'wanted': len(todo), 'cut': 0, 'not_found': 0, 'ring_open': 0}
    placed = []
    if cut:
        bm = bmesh.new()
        bm.from_mesh(me)
        report['tris_before'] = sum(len(f.verts) - 2 for f in bm.faces)
        for r, k in todo:
            base, normal = Vector(r['base']), Vector(r['normal'])
            reach = r['ring_radius'] + 0.0012
            inside = []
            for f in bm.faces:
                good = True
                for v in f.verts:
                    d = v.co - base
                    h = d.dot(normal)
                    if h < -0.0012 or h > r['height'] + 0.0015 or (d - normal * h).length > reach:
                        good = False
                        break
                if good:
                    inside.append(f)
            if not inside:
                report['not_found'] += 1
                placed.append((r, k))  # the head was reduced away or never there: the object still marks it
                continue
            keep = set(inside)
            ring = [e for f in inside for e in f.edges if any(g not in keep for g in e.link_faces) or len(e.link_faces) != 2]
            ring = list(set(ring))
            count = {}
            for e in ring:
                for v in e.verts:
                    count[v] = count.get(v, 0) + 1
            if not ring or any(c != 2 for c in count.values()):
                report['ring_open'] += 1
                continue
            bmesh.ops.delete(bm, geom=inside, context='FACES')
            ring = [e for e in ring if e.is_valid]
            bmesh.ops.edgeloop_fill(bm, edges=ring)
            report['cut'] += 1
            placed.append((r, k))
        bmesh.ops.dissolve_limit(bm, angle_limit=math.radians(PLANAR_DEG), verts=bm.verts[:], edges=bm.edges[:],
                                 use_dissolve_boundaries=False, delimit={'MATERIAL'})
        loose = [v for v in bm.verts if not v.link_faces]
        if loose:
            bmesh.ops.delete(bm, geom=loose, context='VERTS')
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
        report['tris_after'] = sum(len(f.verts) - 2 for f in bm.faces)
        report['open_edges'] = sum(1 for e in bm.edges if e.is_boundary)
        bm.to_mesh(me)
        bm.free()
        me.update()
        for p in me.polygons:
            p.use_smooth = True
        try:
            me.set_sharp_from_angle(angle=math.radians(30.0))
        except Exception:
            pass
        if obj.get('topo_tris') is not None:
            obj['topo_tris'] = report['tris_after']
    else:
        placed = todo
    col = _collection()
    stem = obj.name.replace('TG_RM5010-', '').split('_')[0]
    made = 0
    for i, (r, (kind, removable)) in enumerate(placed):
        shape = r['shape'] if kind != 'rivet' else 'cube'
        ob = bpy.data.objects.new('FASTENER.%s.%s.%03d' % (kind, stem, i), _shape_mesh(shape))
        col.objects.link(ob)
        ob.parent = obj
        ob.matrix_parent_inverse = Matrix.Identity(4)
        normal = Vector(r['normal'])
        rot = normal.to_track_quat('Z', 'Y').to_matrix().to_4x4()
        size = max(r['size'], 0.002)
        ob.matrix_local = Matrix.Translation(Vector(r['base'])) @ rot @ Matrix.Diagonal((size, size, max(r['height'], 0.002), 1.0))
        ob['fastener_kind'] = kind
        ob['fastener_shape'] = r['shape']
        ob['fastener_removable'] = bool(removable)
        ob['fastener_size_m'] = r['size']
        ob['fastener_radius_m'] = r['ring_radius']
        ob['fastener_height_m'] = r['height']
        ob['fastener_host'] = obj.name
        if set_name:
            ob['fastener_set'] = set_name
        if r['shape'] == 'hex':
            guess = thread_guess(r['size'])
            if guess:
                ob['fastener_thread_guess'] = guess
        ob.color = (0.2, 0.2, 0.2, 1.0)
        made += 1
    report['objects_made'] = made
    obj['fasteners_cut'] = int(obj.get('fasteners_cut', 0)) + report['cut']
    return report
