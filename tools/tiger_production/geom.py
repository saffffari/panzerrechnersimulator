import bpy, numpy as np
def mesh_stats(o):
    """World-space volume, volume centroid, principal extents (PCA on vertices) of a mesh object."""
    me = o.data
    n = len(me.vertices)
    if n == 0 or len(me.polygons) == 0:
        return None
    co = np.empty(n*3, np.float64); me.vertices.foreach_get('co', co); co = co.reshape(-1, 3)
    M = np.array(o.matrix_world, dtype=np.float64)
    w = co @ M[:3, :3].T + M[:3, 3]
    me.calc_loop_triangles()
    nt = len(me.loop_triangles)
    tri = np.empty(nt*3, np.int64); me.loop_triangles.foreach_get('vertices', tri); tri = tri.reshape(-1, 3)
    a, b, c = w[tri[:, 0]], w[tri[:, 1]], w[tri[:, 2]]
    v6 = np.einsum('ij,ij->i', a, np.cross(b, c))
    vol = v6.sum()/6.0
    if abs(vol) > 1e-12:
        cen = ((a+b+c)*v6[:, None]).sum(0)/(24.0*vol)
    else:
        cen = w.mean(0)
    area = 0.5*np.linalg.norm(np.cross(b-a, c-a), axis=1).sum()
    q = w - w.mean(0)
    try:
        _, _, vt = np.linalg.svd(q[::max(1, len(q)//20000)], full_matrices=False)
        proj = q @ vt.T
        ext = (proj.max(0)-proj.min(0)).tolist()
        axes = vt.tolist()
    except Exception:
        ext, axes = [0, 0, 0], []
    lo, hi = w.min(0), w.max(0)
    return dict(vol=float(abs(vol)), area=float(area), cen=cen.tolist(), ext=ext, axes=axes,
                bmin=lo.tolist(), bmax=hi.tolist(), tris=int(nt))
