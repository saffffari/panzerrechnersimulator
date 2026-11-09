"""Topology pass for the Tiger gen2 master (tiger_gen2_live.blend), 8 November 2026.

The kit CAD arrives as a dense tessellation: every face's vertices split from its neighbours',
flat faces cut into many triangles, cylinders and fillets at a pitch far below a game pixel. The
parts are watertight once welded. This module reduces a part in three steps and measures what
each step moved:

1. weld: merge vertices closer than WELD_M (the import's split vertices; no shape change);
2. planar: dissolve edges between faces that meet within PLANAR_DEG (exact for flat faces, a
   bounded facet error on curved ones), keeping material borders;
3. collapse: edge-collapse decimation, the ratio searched so that the two-sided distance between
   the result and the welded original stays within a tolerance in metres.

The deviation is measured, not assumed (CLAUDE.md, second principle: a simplification made from
the source geometry carries a measured error bound): the result's vertices and face centres
against the original's surface, and a sample of the original's vertices against the result's.
Each part records `topo_src_tris`, `topo_tris`, `topo_tol_m` and `topo_dev_m` as custom properties.

Run inside Blender:  exec(open(PATH).read());  report = reduce_objects([...], tol_m=0.001)
"""
import math
import time

import bmesh
import bpy
from mathutils.bvhtree import BVHTree

WELD_M = 2.0e-5
PLANAR_DEG = 0.5
SHARP_DEG = 30.0
SAMPLE = 20000


def _tris(bm):
    return sum(len(f.verts) - 2 for f in bm.faces)


def _bvh(bm):
    bm.verts.ensure_lookup_table()
    bm.faces.ensure_lookup_table()
    return BVHTree.FromBMesh(bm, epsilon=0.0)


def _deviation(src_bm, src_bvh, new_bm):
    """Two-sided distance (m): the new surface's vertices and face centres to the source surface,
    and a sample of the source's vertices to the new surface."""
    worst = 0.0
    for v in new_bm.verts:
        hit = src_bvh.find_nearest(v.co)
        if hit[0] is not None:
            worst = max(worst, hit[3])
    # Face interiors are sampled at the centroids of the faces' triangles: the median centre of a
    # dissolved face can lie off it (a ring's centre is in its hole).
    tri = new_bm.copy()
    bmesh.ops.triangulate(tri, faces=tri.faces[:])
    for f in tri.faces:
        hit = src_bvh.find_nearest(f.calc_center_median())
        if hit[0] is not None:
            worst = max(worst, hit[3])
    new_bvh = _bvh(tri)
    n = len(src_bm.verts)
    step = max(1, n // SAMPLE)
    for i in range(0, n, step):
        hit = new_bvh.find_nearest(src_bm.verts[i].co)
        if hit[0] is not None:
            worst = max(worst, hit[3])
    tri.free()
    return worst


def _welded_source(me):
    bm = bmesh.new()
    bm.from_mesh(me)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=WELD_M)
    degenerate = [f for f in bm.faces if f.calc_area() < 1.0e-12]
    if degenerate:
        bmesh.ops.delete(bm, geom=degenerate, context='FACES')
    return bm


def _planar(bm, deg):
    bmesh.ops.dissolve_limit(bm, angle_limit=math.radians(deg), verts=bm.verts[:], edges=bm.edges[:],
                             use_dissolve_boundaries=False, delimit={'MATERIAL'})


def _collapse(obj, bm_in, ratio):
    """Edge-collapse `bm_in` at `ratio` through a Decimate modifier on a scratch object; returns a
    new bmesh (triangles where collapsed)."""
    me = bpy.data.meshes.new('topo_scratch')
    bm_in.to_mesh(me)
    for m in obj.data.materials:
        me.materials.append(m)
    tmp = bpy.data.objects.new('topo_scratch', me)
    bpy.context.scene.collection.objects.link(tmp)
    mod = tmp.modifiers.new('d', 'DECIMATE')
    mod.decimate_type = 'COLLAPSE'
    mod.ratio = ratio
    mod.use_collapse_triangulate = True
    dg = bpy.context.evaluated_depsgraph_get()
    ev = tmp.evaluated_get(dg)
    out_me = bpy.data.meshes.new_from_object(ev)
    out = bmesh.new()
    out.from_mesh(out_me)
    bpy.data.meshes.remove(out_me)
    bpy.data.objects.remove(tmp)
    bpy.data.meshes.remove(me)
    return out


def reduce_mesh(obj, tol_m=0.001, apply=True, search=7):
    """Reduce one object's mesh within `tol_m`. Returns the record; writes the mesh when `apply`."""
    began = time.time()
    me = obj.data
    src = _welded_source(me)
    src_tris = sum(len(p.vertices) - 2 for p in me.polygons)
    src_bvh = _bvh(src)
    # A fixed angle moves a long plate further than a small part (0.5 degrees over 3 m is 26 mm),
    # so the planar step falls back to finer angles, and to none, until it is inside the tolerance.
    work, planar_dev = None, float('inf')
    for deg in (PLANAR_DEG, PLANAR_DEG / 5.0, PLANAR_DEG / 25.0, 0.0):
        if work is not None:
            work.free()
        work = src.copy()
        if deg > 0.0:
            _planar(work, deg)
        planar_dev = _deviation(src, src_bvh, work) if deg > 0.0 else 0.0
        if planar_dev <= tol_m:
            break
    planar_tris = _tris(work)
    best, best_dev, best_ratio = work, planar_dev, 1.0
    if planar_dev <= tol_m:
        # Search the collapse ratio downward while the deviation stays inside the tolerance.
        lo, hi = 0.0, 1.0
        base = work.copy()
        bmesh.ops.triangulate(base, faces=base.faces[:])
        for _ in range(search):
            mid = (lo + hi) * 0.5
            if mid * _tris(base) < 12:
                lo = mid
                continue
            cand = _collapse(obj, base, mid)
            dev = _deviation(src, src_bvh, cand)
            if dev <= tol_m and len(cand.faces) > 3:
                if best is not work:
                    best.free()
                best, best_dev, best_ratio = cand, dev, mid
                hi = mid
            else:
                cand.free()
                lo = mid
        base.free()
        if best is not work:
            # Flat regions the collapse left as triangle fans go back to single faces, where that
            # stays inside the tolerance.
            merged = best.copy()
            _planar(merged, PLANAR_DEG)
            merged_dev = _deviation(src, src_bvh, merged)
            if merged_dev <= tol_m:
                best.free()
                best, best_dev = merged, merged_dev
            else:
                merged.free()
    out_tris = _tris(best)
    record = {'object': obj.name, 'src_tris': src_tris, 'welded_tris': _tris(src), 'planar_tris': planar_tris,
              'planar_dev_mm': round(planar_dev * 1000.0, 4), 'tris': out_tris, 'ratio': round(best_ratio, 4),
              'dev_mm': round(best_dev * 1000.0, 4), 'tol_mm': tol_m * 1000.0,
              'boundary_edges': sum(1 for e in best.edges if e.is_boundary),
              'nonmanifold_edges': sum(1 for e in best.edges if not e.is_manifold),
              'seconds': 0.0}
    if apply and best_dev <= tol_m:
        best.normal_update()
        best.to_mesh(me)
        me.update()
        if me.has_custom_normals:
            with bpy.context.temp_override(object=obj, active_object=obj, selected_objects=[obj]):
                try:
                    bpy.ops.mesh.customdata_custom_splitnormals_clear()
                except Exception:
                    pass
        for p in me.polygons:
            p.use_smooth = True
        try:
            me.set_sharp_from_angle(angle=math.radians(SHARP_DEG))
        except Exception:
            pass
        obj['topo_src_tris'] = src_tris
        obj['topo_tris'] = out_tris
        obj['topo_tol_m'] = tol_m
        obj['topo_dev_m'] = best_dev
        record['applied'] = True
    else:
        record['applied'] = False
    if best is not work:
        best.free()
    work.free()
    src.free()
    record['seconds'] = round(time.time() - began, 2)
    return record


def reduce_objects(names, tol_m=0.001, apply=True, budget_s=None):
    """Reduce the named objects; stops when `budget_s` is spent and reports what is left."""
    began = time.time()
    done, left = [], []
    for i, name in enumerate(names):
        if budget_s is not None and time.time() - began > budget_s:
            left = names[i:]
            break
        obj = bpy.data.objects.get(name)
        if obj is None or obj.type != 'MESH' or obj.get('topo_tris') is not None:
            continue
        done.append(reduce_mesh(obj, tol_m=tol_m, apply=apply))
    return {'done': done, 'left': left, 'src_tris': sum(r['src_tris'] for r in done),
            'tris': sum(r['tris'] for r in done), 'worst_dev_mm': max([r['dev_mm'] for r in done] or [0.0]),
            'seconds': round(time.time() - began, 1)}


# ── The whole model: identical parts share one reduced mesh ──────────────────────────────────────

def _mesh_hash(me):
    import hashlib
    import numpy as np
    co = np.empty(len(me.vertices) * 3, dtype=np.float32)
    me.vertices.foreach_get('co', co)
    return '%d:%s' % (len(me.vertices), hashlib.md5(np.round(co, 5).tobytes()).hexdigest()[:16])


def plan(skip_words=('_WIRE_',)):
    """Group the scene's unreduced mesh objects by identical local geometry (the kit's repeated
    parts: track links, rounds, wheels). Returns groups, heaviest first: (hash, names, tris each)."""
    groups = {}
    for o in bpy.context.scene.objects:
        if o.type != 'MESH' or o.get('topo_tris') is not None or any(w in o.name for w in skip_words):
            continue
        h = o.get('topo_hash')
        if h is None:
            h = _mesh_hash(o.data)
            o['topo_hash'] = h
        groups.setdefault(h, []).append(o.name)
    out = []
    for h, names in groups.items():
        me = bpy.data.objects[names[0]].data
        out.append((h, sorted(names), sum(len(p.vertices) - 2 for p in me.polygons)))
    out.sort(key=lambda g: -g[2] * len(g[1]))
    return out


def run(budget_s=120.0, tol_m=0.001, skip_words=('_WIRE_',), max_tris=1500000):
    """Reduce duplicate groups, heaviest first, until `budget_s` is spent. One mesh per group: the
    first object is reduced and the others take its mesh data. Parts over `max_tris` are left for a
    call of their own. Returns totals and what is left."""
    import contextlib
    import io
    began = time.time()
    with contextlib.redirect_stdout(io.StringIO()):
        groups = plan(skip_words)
        done, skipped = [], []
        src_total = out_total = 0
        worst = 0.0
        left = 0
        for h, names, tris in groups:
            if tris > max_tris:
                skipped.append((names[0], tris))
                continue
            if time.time() - began > budget_s:
                left += 1
                continue
            first = bpy.data.objects[names[0]]
            rec = reduce_mesh(first, tol_m=tol_m, apply=True)
            if not rec['applied']:
                skipped.append((names[0], tris))
                continue
            for n in names[1:]:
                o = bpy.data.objects[n]
                old = o.data
                o.data = first.data
                for k in ('topo_src_tris', 'topo_tris', 'topo_tol_m', 'topo_dev_m'):
                    o[k] = first[k]
                if old.users == 0:
                    bpy.data.meshes.remove(old)
            src_total += rec['src_tris'] * len(names)
            out_total += rec['tris'] * len(names)
            worst = max(worst, rec['dev_mm'])
            done.append((names[0][10:34], len(names), rec['src_tris'], rec['tris'], rec['dev_mm']))
    scene_tris = 0
    for o in bpy.context.scene.objects:
        if o.type == 'MESH':
            scene_tris += sum(len(p.vertices) - 2 for p in o.data.polygons)
    return {'groups_done': len(done), 'groups_left': left, 'skipped': skipped[:12], 'src_tris': src_total, 'tris': out_total,
            'worst_dev_mm': worst, 'scene_tris_now': scene_tris, 'top': sorted(done, key=lambda d: -d[1] * d[2])[:14],
            'seconds': round(time.time() - began, 1)}
