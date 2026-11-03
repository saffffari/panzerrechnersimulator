"""Shell buildings for the Berlin 1945 map: the burnt-out, shelled and still-roofed blocks of the
Dorotheenstadt, Pariser Platz, the Alsenviertel remnant and the landmark ruins (Kroll, the
Ministry of the Interior). Used by model_city_blocks.py and model_landmarks.py.

A building is a rectangle of perimeter walls with window openings on every storey. Conditions
(the 1945 states in the photographs, docs/BERLIN_REICHSTAG_1945.md):
- 'gutted': roof and floors burnt out (the 1943-45 raids and the April fighting); the walls stand,
  every window open to the sky, soot over the openings, the wall heads ragged;
- 'collapsed': gutted, and one corner section down to the lower storeys;
- 'roofed': the roof on (a slate mansard or a flat roof), the windows dark glass or boarded,
  a few blown in; the interior is not modelled and its collision is solid;
- 'roofed_damaged': roofed with holes in the roof and some bays burnt.
Collision: a shell's walls are their own pieces (piers, sills, lintels), so windows stay open to
fire and sight; a roofed building is solid inside its walls.
"""
import math

import bl_kit as K

h01 = K.h01


def facade_openings(u0, u1, z_levels, bay, w_frac, key, ground_shop=False, arch=False):
    """Window grid of one face: returns openings [(a0, a1, b0, b1)] and their bay centres."""
    n = max(int(round((u1-u0)/bay)), 1)
    bw = (u1-u0)/n
    ops = []
    for b in range(n):
        c = u0+bw*(b+0.5)
        for li, (z0, h) in enumerate(z_levels):
            if li == 0 and ground_shop:
                w = bw*0.72
                ops.append((c-w/2, c+w/2, z0+0.3, z0+h-0.9))
            else:
                w = bw*w_frac
                ops.append((c-w/2, c+w/2, z0+0.95, z0+h-0.75))
    return ops, n, bw


def shell(m, x0, x1, y0, y1, storeys, cond, key, storey_h=4.0, ground_h=4.8, bay=3.6, w_frac=0.42,
          wall_mat='plaster', trim_mat='plaster_dark', t=0.75, faces=('n', 's', 'e', 'w'), party=(),
          roof='mansard', shop=True, cornice=True, collapse_side=None):
    """One building. faces: which faces carry windows; party: blind fire walls (brick)."""
    levels = [(0.0, ground_h)]+[(ground_h+storey_h*k, storey_h) for k in range(storeys-1)]
    top = ground_h+storey_h*(storeys-1)
    roofed = cond in ('roofed', 'roofed_damaged')
    sides = {
        'w': ('x', x0, -1, y0, y1), 'e': ('x', x1, 1, y0, y1),
        's': ('y', y0, -1, x0, x1), 'n': ('y', y1, 1, x0, x1),
    }
    # the collapsed section: a span of one face (and its corner) brought down to 1-2 storeys
    cside = collapse_side or ['n', 's', 'e', 'w'][int(h01(key+'cs')*4)]
    for f, (axis, plane, out, ua, ub) in sides.items():
        p0, p1 = sorted((plane, plane-out*t))
        fkey = f'{key}.{f}'
        if f in party:
            # a blind fire wall (brick), ragged top when gutted
            ztop = top+(0.0 if roofed else -storey_h*0.4*h01(fkey+'pt'))
            m.wall(axis, p0, p1, ua, ub, 0.0, ztop, [], 'brick')
            continue
        ops, n, bw = facade_openings(ua, ub, levels, bay, w_frac, fkey, ground_shop=shop and f in faces[:1])
        if f not in faces:
            ops = [o for o in ops if h01(f'{fkey}{o[0]:.1f}{o[2]:.1f}') < 0.6]
        if roofed:
            m.wall(axis, p0, p1, ua, ub, 0.0, top, ops, wall_mat)
            for (a0, a1, b0, b1) in ops:
                k = f'{fkey}{a0:.1f}{b0:.1f}'
                g0, g1 = sorted((plane-out*0.35, plane-out*0.40))
                if cond == 'roofed_damaged' and h01(k+'burn') < 0.25:
                    gb0, gb1 = sorted((plane-out*(t-0.02), plane-out*(t+0.01)))
                    m.wbox(axis, gb0, gb1, a0, a1, b0, b1, 'soot')
                    m.plate(axis, plane, out, a0+0.1, a1-0.1, b1, b1+1.2, 'soot')
                elif h01(k+'board') < 0.2:
                    m.wbox(axis, g0, g1, a0, a1, b0, b1, 'timber_charred')
                else:
                    m.wbox(axis, g0, g1, a0, a1, b0, b1, 'glass')
            continue
        # a shell: the wall's top steps down along burnt bays; the collapsed corner section
        prof = []
        for b in range(n):
            a, e = ua+bw*b, ua+bw*(b+1)
            drop = 0.0
            r = h01(f'{fkey}.top{b}')
            if r < 0.25:
                drop = storey_h*(0.3+0.7*h01(f'{fkey}.td{b}'))
            if cond == 'collapsed' and f == cside:
                frac = (b+0.5)/n
                if frac > 0.62:
                    drop = max(drop, top-(ground_h+storey_h*(0.2+1.2*h01(f'{fkey}.cd{b}'))))
            prof.append((a, e, max(top-drop, ground_h*0.6)))
        for (a, e, zt) in prof:
            seg_ops = [(max(o[0], a), min(o[1], e), o[2], min(o[3], zt)) for o in ops if o[1] > a and o[0] < e and o[2] < zt-0.4]
            m.wall(axis, p0, p1, a, e, 0.0, zt, seg_ops, wall_mat)
            # soot above the openings
            for (a0, a1, b0, b1) in seg_ops:
                if b1 < zt-0.5 and h01(f'{fkey}{a0:.1f}{b0:.1f}s') < 0.55:
                    m.plate(axis, plane, out, a0+0.1, a1-0.1, b1, min(b1+0.8+1.6*h01(f'{fkey}{a0:.1f}h'), zt), 'soot')
        # window surrounds and string courses (decoration)
        if cornice:
            for (z0, h) in levels[1:]:
                s0, s1 = sorted((plane, plane+out*0.18))
                m.wbox(axis, s0, s1, ua, ub, z0-0.05, z0+0.18, trim_mat)
    if cornice:
        for f, (axis, plane, out, ua, ub) in sides.items():
            if f in party:
                continue
            u = ua
            while u < ub-1e-6:
                e = min(u+3.0, ub)
                if roofed or h01(f'{key}{f}corn{u:.1f}') > 0.45:
                    s0, s1 = sorted((plane, plane+out*0.7))
                    m.wbox(axis, s0, s1, u, e, top-0.6, top, trim_mat)
                u = e
    if roofed:
        m.solid(x0+t, x1-t, y0+t, y1-t, 0.0, top)
        if roof == 'mansard':
            r0 = 2.6
            m.prism([(x0-0.2, y0-0.2), (x1+0.2, y0-0.2), (x1+0.2, y1+0.2), (x0-0.2, y1+0.2)], top, top+0.3, 'slate')
            # the steep lower slope as a frustum: four trapezoid slabs
            zm = top+3.6
            xi0, xi1, yi0, yi1 = x0+r0*0.4, x1-r0*0.4, y0+r0*0.4, y1-r0*0.4
            _frustum(m, (x0, x1, y0, y1), (xi0, xi1, yi0, yi1), top+0.3, zm, 'slate')
            xt0, xt1, yt0, yt1 = xi0+r0, xi1-r0, yi0+r0, yi1-r0
            if xt1 > xt0 and yt1 > yt0:
                _frustum(m, (xi0, xi1, yi0, yi1), (xt0, xt1, yt0, yt1), zm, zm+1.4, 'zinc_roof')
            m.solid(xi0, xi1, yi0, yi1, top, zm)
            m.bounded.append(('mansard roof', round(r0*0.4, 3)))
            # dormers on the faces with windows
            for f, (axis, plane, out, ua, ub) in sides.items():
                if f in party or f not in faces:
                    continue
                n = max(int((ub-ua)/bay), 1)
                for b in range(0, n, 2):
                    c = ua+(ub-ua)*(b+0.5)/n
                    pi = plane-out*(r0*0.4+0.2)
                    d0, d1 = sorted((pi, pi-out*1.6))
                    if cond == 'roofed_damaged' and h01(f'{key}{f}dorm{b}') < 0.3:
                        continue
                    m.wbox(axis, d0, d1, c-0.8, c+0.8, top+0.6, top+2.6, 'zinc_roof')
            if cond == 'roofed_damaged':
                # roof holes: burnt-through patches show the charred rafters
                for k in range(3):
                    hx = x0+(x1-x0)*(0.2+0.6*h01(f'{key}rh{k}'))
                    hy = y0+(y1-y0)*(0.3+0.4*h01(f'{key}rhy{k}'))
                    m.box(hx-2.0, hx+2.0, hy-1.5, hy+1.5, zm-0.2, zm+0.05, 'timber_charred')
        else:
            m.box(x0, x1, y0, y1, top-0.3, top+0.2, 'zinc_roof')
    else:
        # inside the shell: rubble of the floors on the ground (low), charred joist stubs
        m.box(x0+t, x1-t, y0+t, y1-t, 0.0, 0.6+0.5*h01(key+'rub'), 'rubble')
        for k in range(int((x1-x0)/4)):
            if h01(f'{key}.j{k}') < 0.35:
                x = x0+t+2.0+k*4.0
                z = ground_h+storey_h*int(h01(f'{key}.jz{k}')*max(storeys-2, 1))
                m.beam((x, y0+t, z), (x+0.2, y0+t+2.0+3.0*h01(f'{key}.jl{k}'), z-0.8), 0.25, 0.3, 'timber_charred')
    return top


def _frustum(m, lo, hi, z0, z1, mat):
    """Four sloping slabs from rectangle lo at z0 to rectangle hi at z1, plus a top cap."""
    (a0, a1, b0, b1), (c0, c1, d0, d1) = lo, hi
    bm = m.bm(mat)
    v = [bm.verts.new(p) for p in ((a0, b0, z0), (a1, b0, z0), (a1, b1, z0), (a0, b1, z0),
                                   (c0, d0, z1), (c1, d0, z1), (c1, d1, z1), (c0, d1, z1))]
    for f in ((0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7), (4, 5, 6, 7), (3, 2, 1, 0)):
        bm.faces.new([v[i] for i in f])
