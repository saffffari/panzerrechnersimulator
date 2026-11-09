"""Headless: the centreline of each kit cable as a guide spline (owner, 2026-11-08: "add a spline
to blender model to guide rope sim starting point"). Reads the pre-topology backup, whose cables
are the full CAD geometry, and writes one curve per cable to a small library file.

Method, named plainly: the cable's own mesh edges are the graph (so two passes of rope that touch
are not joined); the largest connected shell is walked by shortest edge path from one end to the
other; the shell's vertices are binned by that path length every BIN_M and each bin's centroid is
a point of the line; the line is smoothed over SMOOTH bins (a single strand of a laid rope winds
round the true centre). An end eye, whose two legs lie at equal path length, comes out as the line
through the eye's middle. The rope's radius is the median distance of the shell's vertices from
the line.

  blender --background tiger_gen2_live.pre-topology-20261108.blend --python cable_guides.py -- OUT.blend
"""
import heapq
import sys
import time

import bpy
import bmesh
import numpy as np

BIN_M = 0.03
SMOOTH = 5


def csr(n, edges):
    both = np.concatenate([edges, edges[:, ::-1]])
    order = np.argsort(both[:, 0], kind='stable')
    both = both[order]
    indptr = np.zeros(n + 1, dtype=np.int64)
    np.add.at(indptr, both[:, 0] + 1, 1)
    return np.cumsum(indptr), both[:, 1].copy()


def components(n, indptr, indices):
    label = np.full(n, -1, dtype=np.int32)
    sizes = []
    for seed in range(n):
        if label[seed] != -1:
            continue
        k = len(sizes)
        label[seed] = k
        stack = [seed]
        count = 0
        while stack:
            u = stack.pop()
            count += 1
            for v in indices[indptr[u]:indptr[u + 1]].tolist():
                if label[v] == -1:
                    label[v] = k
                    stack.append(v)
        sizes.append(count)
    return label, sizes


def dijkstra(seed, co, indptr, indices):
    dist = np.full(len(co), np.inf)
    dist[seed] = 0.0
    heap = [(0.0, seed)]
    while heap:
        d, u = heapq.heappop(heap)
        if d > dist[u]:
            continue
        nb = indices[indptr[u]:indptr[u + 1]]
        if len(nb) == 0:
            continue
        nd = d + np.linalg.norm(co[nb] - co[u], axis=1)
        for v, w in zip(nb.tolist(), nd.tolist()):
            if w < dist[v]:
                dist[v] = w
                heapq.heappush(heap, (w, v))
    return dist


def guide(obj):
    began = time.time()
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=2.0e-5)
    bm.verts.ensure_lookup_table()
    n = len(bm.verts)
    co = np.array([v.co[:] for v in bm.verts], dtype=np.float64)
    edges = np.array([(e.verts[0].index, e.verts[1].index) for e in bm.edges], dtype=np.int64)
    bm.free()
    indptr, indices = csr(n, edges)
    label, sizes = components(n, indptr, indices)
    big = int(np.argmax(sizes))
    members = np.nonzero(label == big)[0]
    d0 = dijkstra(int(members[0]), co, indptr, indices)
    a = int(members[np.argmax(d0[members])])
    da = dijkstra(a, co, indptr, indices)
    length_path = float(da[members].max())
    bins = np.floor(da[members] / BIN_M).astype(np.int64)
    nb = int(bins.max()) + 1
    pts = np.zeros((nb, 3))
    cnt = np.zeros(nb)
    np.add.at(pts, bins, co[members])
    np.add.at(cnt, bins, 1.0)
    keep = cnt > 0
    pts = pts[keep] / cnt[keep][:, None]
    # moving average, ends held
    sm = pts.copy()
    h = SMOOTH // 2
    for i in range(len(pts)):
        lo, hi = max(0, i - h), min(len(pts), i + h + 1)
        sm[i] = pts[lo:hi].mean(axis=0)
    sm[0], sm[-1] = pts[0], pts[-1]
    seg = np.linalg.norm(np.diff(sm, axis=0), axis=1)
    # radius: every vertex of the whole mesh against its bin's line point
    centre = sm[np.minimum(np.floor(da[members] / BIN_M).astype(np.int64), len(sm) - 1)] if len(sm) == nb else None
    radius = float(np.median(np.linalg.norm(co[members] - centre, axis=1))) if centre is not None else 0.0
    cu = bpy.data.curves.new('GUIDE.' + obj.name.split('_')[1] + '_cable', 'CURVE')
    cu.dimensions = '3D'
    sp = cu.splines.new('POLY')
    sp.points.add(len(sm) - 1)
    for p, c in zip(sp.points, sm):
        p.co = (c[0], c[1], c[2], 1.0)
    ob = bpy.data.objects.new(cu.name, cu)
    ob.matrix_world = obj.matrix_world.copy()
    ob['source_object'] = obj.name
    ob['length_m'] = float(seg.sum())
    ob['edge_path_length_m'] = length_path
    ob['radius_m'] = radius
    ob['shells'] = len(sizes)
    ob['shell_share'] = float(sizes[big]) / float(n)
    ob['method'] = 'centroids of the largest shell binned by edge-path length every %.0f mm, smoothed over %d bins' % (BIN_M * 1000.0, SMOOTH)
    print('[guides]', obj.name, 'verts', n, 'shells', len(sizes), 'largest share %.3f' % (sizes[big] / n),
          'line %.2f m' % seg.sum(), 'edge path %.2f m' % length_path, 'radius %.1f mm' % (radius * 1000.0),
          'points', len(sm), '%.0f s' % (time.time() - began), flush=True)
    return ob


out = sys.argv[sys.argv.index('--') + 1]
made = [guide(o) for o in bpy.context.scene.objects if o.type == 'MESH' and '_WIRE_' in o.name]
bpy.data.libraries.write(out, set(made), fake_user=True)
print('[guides] wrote', out, flush=True)
