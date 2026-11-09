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
