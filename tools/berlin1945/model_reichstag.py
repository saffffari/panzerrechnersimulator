"""The Reichstag as it stood on 2 May 1945, for the Berlin 1945 map.

    blender --background --python tools/berlin1945/model_reichstag.py -- OUT_DIR [PREVIEW_PREFIX]

Writes OUT_DIR/reichstag_1945.glb and reichstag_1945.colliders.json (bl_kit.py: frames, materials,
collision). Origin: the centre of the building's footprint at ground level; +X east, +Y north.

Sources (full list with URLs: docs/BERLIN_REICHSTAG_1945.md):
- Footprint: OpenStreetMap relation 2201742 (the Wallot outer walls survived the 1961-71 and
  1995-99 rebuilds): 137.8 m north-south over the corner pavilions, 104 m east-west over the west
  portico and the east portal, main walls at x = +-43 m, corner pavilions 18 x 21 m; the
  1928 Berlin aerial survey (Geoportal Berlin, luftbild_1928_04) for the two courtyards either
  side of the domed hall and the dome's square plan.
- Elevations: the IWM BU 8573 west front (3 June 1945), Khaldei's west-north-west view (May 1945),
  IWM BU 8922 south front (12 July 1945): rusticated arcaded ground storey, a giant order over two
  window storeys, balustrade; six Corinthian columns on the west portico under a pediment, the
  Begas Germania group above it; four corner towers whose caps were removed in 1941 for 2 cm flak
  platforms (flat tops in every 1945 photograph); the dome's bare steel ribs, glass gone.
- Heights: corner towers 46 m with their caps (de.wikipedia, Reichstagsgebaeude), so about 42 m
  flat-topped; the dome "knapp 75 m" to its top; the rest scaled from the photographs (eaves
  about 25 m), an estimate recorded as such.
- 1945 state: windows bricked up in 1939 with small firing slits (BU 8573, Bundesarchiv 204-008),
  brick blast walls in front of the portico, the inscription's letters shot away: BU 8573 reads
  "DEM  EUTSCHEN  OLKE" (the D of DEUTSCHEN and the V of VOLKE gone), soot streaks, shell scars,
  parapet and cornice pieces knocked off, the lantern's frame bent.
Not modelled: the interior (the burnt plenary hall, the rooms fought through on 30 April - 2 May);
its collision is the solid mass of the wings behind the bricked-up windows.
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import bl_kit as K  # noqa: E402
from mathutils import Matrix  # noqa: E402

h01 = K.h01

# Levels (m above ground).
Z_PLINTH = 1.1
Z_BASE = 8.4          # rusticated ground storey top
Z_STRING = 9.2        # string course top
Z_ORDER = 22.2        # top of the giant order's capitals
Z_ENT = 25.2          # top of the entablature
Z_PAR = 26.8          # top of the balustrade
Z_ROOF = 25.0
Z_PODIUM = 7.4        # portico floor
# Footprint (OSM relation 2201742, see the module doc).
X_WALL = 43.3         # west/east main walls
N_WALL = 64.9         # north/south main walls
PAV_X0, PAV_X1 = 28.4, 47.2   # corner pavilion extents (abs)
PAV_N0, PAV_N1 = 47.7, 68.9
TOWER = (32.2, 44.4, 51.6, 63.8)   # tower shaft x0, x1, n0, n1 (abs)
Z_TOWER = 41.2
COURT = (18.0, 24.0, 44.5)    # courtyard half-width x, inner n, outer n


def bay_windows(m, axis, plane, out, u0, u1, nbays, key, pilasters=True, ground_arch=True, mat='sandstone'):
    """One stretch of main facade: wall from plane (outer face) inward, `out` = +1/-1 the outward
    normal sign. Ground storey arcaded windows, two window storeys in a giant order. All windows
    bricked up with a firing slit (1939); some scarred."""
    t = 1.4
    p_out, p_in = (plane, plane-out*t) if out > 0 else (plane, plane+t)
    p0, p1 = min(p_out, p_in), max(p_out, p_in)
    bw = (u1-u0)/nbays
    gw, nw, uw = 2.5, 2.2, 2.0
    openings = []
    for b in range(nbays):
        c = u0+bw*(b+0.5)
        openings.append((c-gw/2, c+gw/2, 1.9, 5.6+gw/2))
        openings.append((c-nw/2, c+nw/2, 10.4, 15.4))
        openings.append((c-uw/2, c+uw/2, 17.2, 20.6))
    # the wall proper, with rectangular holes (the round ground-floor heads are filled below)
    m.wall(axis, p0, p1, u0, u1, 0.0, Z_ENT, openings, mat, solid=False)
    # collision: the wall with its windows bricked up is solid
    if axis == 'x':
        m.solid(p0, p1, u0, u1, 0.0, Z_ENT)
    else:
        m.solid(u0, u1, p0, p1, 0.0, Z_ENT)
    face = p_out
    for b in range(nbays):
        c = u0+bw*(b+0.5)
        k = f'{key}.{b}'
        # ground storey round head: the opening's top part filled as an arch
        m.arch_fill(axis, p0, p1, c-gw/2, c+gw/2, 5.6, mat)
        for (w, z0, z1, kind) in ((gw, 1.9, 5.6+gw/2, 'g'), (nw, 10.4, 15.4, 'n'), (uw, 17.2, 20.6, 'u')):
            # brick infill set back 0.35 m, with a firing slit
            back0 = face-out*0.35
            back1 = face-out*(t-0.05)
            slit_z = z0+1.15 if kind != 'u' else z0+1.0
            sw = 0.30
            infill_z1 = z1 if kind != 'g' else 5.6+gw/2
            scar = h01(k+kind) < 0.06
            if scar:
                # a shell has blown part of the infill out; the hole stays shallow (the rooms behind
                # are not modelled)
                m.wall(axis, min(back0, back1), max(back0, back1), c-w/2, c+w/2, z0, infill_z1,
                       [(c-w*0.32, c+w*0.28, z0+0.6, z0+2.4)], 'brick_infill')
                m.wbox(axis, min(face-out*(t-0.06), face-out*(t-0.3)), max(face-out*(t-0.06), face-out*(t-0.3)),
                       c-w*0.32, c+w*0.28, z0+0.6, z0+2.4, 'soot')
            else:
                m.wall(axis, min(back0, back1), max(back0, back1), c-w/2, c+w/2, z0, infill_z1,
                       [(c-sw/2, c+sw/2, slit_z, slit_z+0.36)], 'brick_infill')
                m.wbox(axis, min(face-out*(t-0.06), face-out*(t-0.25)), max(face-out*(t-0.06), face-out*(t-0.25)),
                       c-sw/2, c+sw/2, slit_z, slit_z+0.36, 'plaster_dark')
            if kind == 'n':
                # aedicule: a pediment over the piano nobile window on consoles
                pa, pb = sorted((face, face+out*0.35))
                m.wbox(axis, pa, pb, c-w/2-0.35, c+w/2+0.35, z1, z1+0.45, mat)
                if axis == 'x':
                    m.gable(pa, pb, c-w/2-0.45, c+w/2+0.45, z1+0.45, 0.9, 'x', mat)
                else:
                    m.gable(c-w/2-0.45, c+w/2+0.45, pa, pb, z1+0.45, 0.9, 'y', mat)
            # soot from the fires above some openings
            if h01(k+kind+'s') < 0.35:
                sa, sb = sorted((face, face+out*0.03))
                m.wbox(axis, sa, sb, c-w*0.35, c+w*0.35, z1+0.1, z1+1.4+2.2*h01(k+kind+'h'), 'soot')
    # rusticated plinth and the string course
    pa, pb = sorted((face, face+out*0.35))
    m.wbox(axis, pa, pb, u0, u1, 0.0, Z_PLINTH, 'granite', solid=False)
    pa, pb = sorted((face, face+out*0.55))
    m.wbox(axis, pa, pb, u0, u1, Z_BASE, Z_STRING, mat)
    # rustication grooves read as bands
    for z in (2.3, 3.6, 4.9, 6.2, 7.4):
        pa, pb = sorted((face, face+out*0.05))
        m.wbox(axis, pa, pb, u0, u1, z, z+0.12, 'sandstone_dark')
    # giant pilasters between the bays
    if pilasters:
        for b in range(nbays+1):
            c = u0+bw*b
            pa, pb = sorted((face, face+out*0.45))
            m.wbox(axis, pa, pb, c-0.55, c+0.55, Z_STRING, Z_ORDER-1.0, mat)
            pa, pb = sorted((face, face+out*0.6))
            m.wbox(axis, pa, pb, c-0.75, c+0.75, Z_ORDER-1.0, Z_ORDER, 'sandstone_light')
    entablature(m, axis, face, out, u0, u1, key)


def entablature(m, axis, face, out, u0, u1, key, z0=Z_ORDER, z1=Z_ENT, par=Z_PAR, balustrade=True):
    pa, pb = sorted((face, face+out*0.35))
    m.wbox(axis, pa, pb, u0, u1, z0, z0+1.2, 'sandstone')          # architrave
    pa, pb = sorted((face, face+out*0.25))
    m.wbox(axis, pa, pb, u0, u1, z0+1.2, z1-0.7, 'sandstone')     # frieze
    # the cornice, with pieces shot off
    seg = 3.0
    u = u0
    while u < u1-1e-6:
        e = min(u+seg, u1)
        if h01(f'{key}.c{u:.1f}') > 0.10:
            pa, pb = sorted((face, face+out*1.0))
            m.wbox(axis, pa, pb, u, e, z1-0.7, z1, 'sandstone_light')
        u = e
    if balustrade:
        u = u0
        while u < u1-1e-6:
            e = min(u+2.4, u1)
            if h01(f'{key}.b{u:.1f}') > 0.14:
                pa, pb = sorted((face-out*0.1, face-out*0.6))
                m.wbox(axis, pa, pb, u, e, z1, z1+0.25, 'sandstone')
                for k in range(4):
                    a = u+(e-u)*(k+0.5)/4
                    m.wbox(axis, pa, pb, a-0.12, a+0.12, z1+0.25, par-0.25, 'sandstone')
                m.wbox(axis, pa, pb, u, e, par-0.25, par, 'sandstone')
            u = e


def roof_parapet_inner(m, axis, face, out, u0, u1):
    """The wall's mass from the facade inward under the roof (collision only)."""
    pass


def pavilion(m, sx, sy, key):
    """A corner pavilion with its tower. sx, sy = +-1 the quadrant."""
    def X(a):
        return sx*a

    def Y(a):
        return sy*a
    x0, x1 = sorted((X(PAV_X0), X(PAV_X1)))
    y0, y1 = sorted((Y(PAV_N0), Y(PAV_N1)))
    # the pavilion's mass, slightly higher than the wings
    m.solid(x0, x1, y0, y1, 0.0, Z_ENT+0.6)
    # visual mass inside the two articulated outer faces
    ix0, ix1 = (x0, x1-1.45) if sx > 0 else (x0+1.45, x1)
    iy0, iy1 = (y0, y1-1.45) if sy > 0 else (y0+1.45, y1)
    m.box(ix0, ix1, iy0, iy1, 0.0, Z_ENT+0.6, 'sandstone')
    # facade articulation on the two outer faces: three bays each, bricked windows
    ox = X(PAV_X1)
    oy = Y(PAV_N1)
    bay_windows(m, 'x', ox, sx, min(Y(PAV_N0), Y(PAV_N1)), max(Y(PAV_N0), Y(PAV_N1)), 3, key+'.px')
    bay_windows(m, 'y', oy, sy, min(X(PAV_X0), X(PAV_X1)), max(X(PAV_X0), X(PAV_X1)), 3, key+'.py')
    # corner quoins (rusticated pilasters at the pavilion's outer corner)
    m.box(X(PAV_X1)-sx*0.0, X(PAV_X1)+sx*0.5, Y(PAV_N1)-sy*2.0, Y(PAV_N1)+sy*0.5, 0, Z_ENT, 'sandstone_light')
    # the tower: a square shaft, a top storey of three arched openings a side, cornice, a flat
    # platform (the 1941 flak conversion) with a parapet
    tx0, tx1 = sorted((X(TOWER[0]), X(TOWER[1])))
    ty0, ty1 = sorted((Y(TOWER[2]), Y(TOWER[3])))
    zt0 = Z_PAR
    m.box(tx0, tx1, ty0, ty1, Z_ENT, 33.0, 'sandstone', solid=True)
    # top storey: corner piers and arches on each face (open belfry-like openings)
    w = tx1-tx0
    zs, za = 33.0, 37.4
    m.solid(tx0, tx1, ty0, ty1, zs, Z_TOWER+1.2)
    for face_axis, plane, sgn, ua, ub in (('x', tx0, -1, ty0, ty1), ('x', tx1, 1, ty0, ty1),
                                          ('y', ty0, -1, tx0, tx1), ('y', ty1, 1, tx0, tx1)):
        p0, p1 = sorted((plane, plane-sgn*1.2))
        ops = []
        for k in range(3):
            c = ua+(ub-ua)*(k+0.5)/3
            ops.append((c-1.35, c+1.35, zs+0.8, za))
        m.wall(face_axis, p0, p1, ua, ub, zs, Z_TOWER, ops, 'sandstone')
        for k in range(3):
            c = ua+(ub-ua)*(k+0.5)/3
            m.arch_fill(face_axis, p0, p1, c-1.35, c+1.35, za-1.35, 'sandstone')
        # the openings' backs: the tower's inner stair core (dark, sooted)
        q0, q1 = sorted((plane-sgn*1.2, plane-sgn*2.6))
        m.wbox(face_axis, q0, q1, ua+1.0, ub-1.0, zs, za, 'soot')
        # cornice and platform parapet
        c0, c1 = sorted((plane, plane+sgn*0.9))
        m.wbox(face_axis, c0, c1, ua-0.9, ub+0.9, Z_TOWER-0.8, Z_TOWER, 'sandstone_light')
        d0, d1 = sorted((plane, plane-sgn*0.5))
        u = ua
        while u < ub-1e-6:
            e = min(u+2.0, ub)
            if h01(f'{key}.tp{face_axis}{plane:.1f}{u:.1f}') > 0.2:
                m.wbox(face_axis, d0, d1, u, e, Z_TOWER, Z_TOWER+1.2, 'sandstone')
            u = e
        # the shaft's corner pilasters and a cornice at the order's height
        m.wbox(face_axis, c0, c1, ua-0.4, ua+1.2, Z_ENT, zs, 'sandstone_light')
        m.wbox(face_axis, c0, c1, ub-1.2, ub+0.4, Z_ENT, zs, 'sandstone_light')
        e0, e1 = sorted((plane, plane+sgn*0.6))
        m.wbox(face_axis, e0, e1, ua, ub, zs-0.6, zs, 'sandstone_light')
    # platform slab (concrete, 1941)
    m.box(tx0+1.0, tx1-1.0, ty0+1.0, ty1-1.0, Z_TOWER-0.6, Z_TOWER, 'render_grey')
    # soot on the tower faces from the fires (BU 8573 shows the towers streaked)
    for k in range(3):
        if h01(f'{key}.ts{k}') < 0.6:
            a = tx0+w*(0.2+0.25*k)
            if sx > 0:
                m.box(tx1, tx1+0.03, ty0+2+k*2.5, ty0+3.2+k*2.5, 27.5, 32.0, 'soot')
            else:
                m.box(tx0-0.03, tx0, ty0+2+k*2.5, ty0+3.2+k*2.5, 27.5, 32.0, 'soot')
    # allegorical figures: stone figure blocks at the tower foot corners (decoration)
    for cx in (tx0-0.8, tx1+0.8):
        for cy in (ty0-0.8, ty1+0.8):
            if h01(f'{key}.fig{cx:.1f}{cy:.1f}') > 0.25:
                figure(m, cx, cy, Z_PAR, 2.6, 'sandstone_dark')


def figure(m, cx, cy, z0, h, mat):
    """A crude standing figure on a plinth (silhouette only; no collision)."""
    m.box(cx-0.5, cx+0.5, cy-0.5, cy+0.5, z0, z0+0.5, mat, part='statues')
    m.box(cx-0.33, cx+0.33, cy-0.25, cy+0.25, z0+0.5, z0+0.5+h*0.55, mat, part='statues')
    m.box(cx-0.42, cx+0.42, cy-0.28, cy+0.28, z0+0.5+h*0.55, z0+0.5+h*0.8, mat, part='statues')
    m.box(cx-0.16, cx+0.16, cy-0.16, cy+0.16, z0+0.5+h*0.8, z0+0.5+h, mat, part='statues')


def rider(m, cx, cy, z0, scale, mat, facing='x'):
    """An equestrian group's silhouette: horse and rider (decoration)."""
    s = scale
    def B(a0, a1, b0, b1, c0, c1):
        if facing == 'x':
            m.box(cx+a0*s, cx+a1*s, cy+b0*s, cy+b1*s, z0+c0*s, z0+c1*s, mat, part='statues')
        else:
            m.box(cx+b0*s, cx+b1*s, cy+a0*s, cy+a1*s, z0+c0*s, z0+c1*s, mat, part='statues')
    B(-1.6, 1.6, -0.9, 0.9, 0.0, 0.5)      # plinth
    B(-1.1, 1.0, -0.35, 0.35, 1.4, 2.2)    # barrel
    for a in (-0.9, 0.8):
        for b in (-0.25, 0.25):
            B(a-0.1, a+0.1, b-0.08, b+0.08, 0.5, 1.4)
    B(0.8, 1.2, -0.2, 0.2, 2.0, 2.9)       # neck
    B(1.0, 1.6, -0.15, 0.15, 2.6, 3.0)     # head
    B(-0.3, 0.3, -0.25, 0.25, 2.2, 3.4)    # rider
    B(-0.15, 0.15, -0.15, 0.15, 3.4, 3.8)


def build():
    m = K.Model('reichstag_1945', 'tools/berlin1945/model_reichstag.py')
    # ── main walls between the pavilions (west/east: x = +-43.3; north/south: n = +-64.9)
    for sx in (-1, 1):
        for sy in (-1, 1):
            # west/east wall from the portal projection to the pavilion
            a, b = (19.8, PAV_N0) if True else None
            bay_windows(m, 'x', sx*X_WALL, sx, *sorted((sy*a, sy*b)), 5, f'w{sx}{sy}')
            # the short return by the portico/portal (n 16..19.8 at x = 45.1)
            bay_windows(m, 'x', sx*45.1, sx, *sorted((sy*16.0, sy*19.8)), 1, f'r{sx}{sy}', pilasters=False)
            m.solid(*sorted((sx*43.3, sx*45.1)), *sorted((sy*16.0, sy*19.8)), 0, Z_ENT)
            # north/south wall from the pavilion to the central projection
            bay_windows(m, 'y', sy*N_WALL, sy, *sorted((sx*PAV_X0, sx*9.0)), 3, f'n{sx}{sy}')
            pavilion(m, sx, sy, f'p{sx}{sy}')
    # north and south central projections: four engaged columns and an attic
    for sy in (-1, 1):
        face = sy*66.1
        m.solid(-9.0, 9.0, *sorted((sy*N_WALL, face)), 0, Z_ENT)
        bay_windows(m, 'y', face, sy, -9.0, 9.0, 3, f'c{sy}', pilasters=False)
        for k in range(4):
            c = -6.75+4.5*k
            yy = face+sy*0.7
            m.cyl(c, yy, Z_STRING, Z_ORDER-1.0, 0.75, 0.66, 'sandstone', seg=12)
            m.box(c-0.95, c+0.95, yy-0.95, yy+0.95, Z_ORDER-1.0, Z_ORDER, 'sandstone_light')
            m.box(c-0.9, c+0.9, yy-0.9, yy+0.9, Z_BASE, Z_STRING, 'sandstone')
        m.box(-9.6, 9.6, *sorted((face, face+sy*1.6)), Z_ORDER, Z_ENT, 'sandstone')
        m.box(-8.0, 8.0, *sorted((face-sy*0.2, face-sy*2.4)), Z_ENT, Z_ENT+3.4, 'sandstone', solid=True)  # attic
        # 1945: the attic's top edge is broken on one side
        m.box(-8.0, -3.0+6*h01(f'att{sy}'), *sorted((face-sy*0.2, face-sy*2.4)), Z_ENT+3.4, Z_ENT+4.2, 'sandstone')
    # ── the building's mass behind the facades (collision; the rooms are not modelled)
    cx, ci, co = COURT
    for sy in (-1, 1):
        # wings between the courts and the north/south facades
        m.solid(-X_WALL+1.0, X_WALL-1.0, *sorted((sy*co, sy*(N_WALL-1.0))), 0, Z_ROOF)
        # wings west and east of each court
        m.solid(-X_WALL+1.0, -cx, *sorted((sy*ci, sy*co)), 0, Z_ROOF)
        m.solid(cx, X_WALL-1.0, *sorted((sy*ci, sy*co)), 0, Z_ROOF)
    m.solid(-X_WALL+1.0, X_WALL-1.0, -ci, ci, 0, Z_ROOF)          # the domed hall block
    # roofs: zinc over the wings, open over the two courts
    for sy in (-1, 1):
        m.box(-X_WALL+0.2, X_WALL-0.2, *sorted((sy*co, sy*(N_WALL-0.4))), Z_ROOF-0.4, Z_ROOF, 'zinc_roof')
        m.box(-X_WALL+0.2, -cx, *sorted((sy*ci, sy*co)), Z_ROOF-0.4, Z_ROOF, 'zinc_roof')
        m.box(cx, X_WALL-0.2, *sorted((sy*ci, sy*co)), Z_ROOF-0.4, Z_ROOF, 'zinc_roof')
        # the court walls: plain rendered walls with window rows (seen only from above)
        for (axis, plane, out, ua, ub) in (('x', -cx, 1, *sorted((sy*ci, sy*co))), ('x', cx, -1, *sorted((sy*ci, sy*co))),
                                           ('y', sy*ci, sy, -cx, cx), ('y', sy*co, -sy, -cx, cx)):
            ops = []
            n = max(int((ub-ua)/4.0), 1)
            for k in range(n):
                c = ua+(ub-ua)*(k+0.5)/n
                for z0 in (2.0, 9.5, 14.5, 19.0):
                    ops.append((c-0.9, c+0.9, z0, z0+3.0))
            p0, p1 = sorted((plane, plane-out*0.8))
            m.wall(axis, p0, p1, ua, ub, 0.0, Z_ROOF, ops, 'render_grey', solid=False)
            q0, q1 = sorted((plane-out*0.8, plane-out*1.0))
            m.wbox(axis, q0, q1, ua, ub, 0.0, Z_ROOF, 'plaster_dark')
        # court floors
        m.box(-cx, cx, *sorted((sy*ci, sy*co)), 0.0, 0.15, 'granite')
    m.box(-X_WALL+0.2, X_WALL-0.2, -ci, ci, Z_ROOF-0.4, Z_ROOF, 'zinc_roof')
    # ── the west portico (six Corinthian columns, pediment, inscription) and the stair
    x_front = -52.1
    m.box(x_front, -43.3, -15.7, 16.0, 0, Z_PODIUM, 'sandstone', solid=True)     # podium
    # the stair (24 m wide) from the square up to the portico floor
    steps = 26
    run = 13.0
    for k in range(steps):
        z1 = Z_PODIUM*(k+1)/steps
        xa = x_front-run+run*k/steps
        m.box(xa, x_front, -12.0, 12.0, z1-Z_PODIUM/steps, z1, 'granite', solid=False)
    m.solid(x_front-run, x_front, -12.0, 12.0, 0.0, Z_PODIUM*0.5)
    m.solid(x_front-run*0.5, x_front, -12.0, 12.0, Z_PODIUM*0.5, Z_PODIUM)
    m.bounded.append(('portico stair', round(Z_PODIUM*0.25, 3)))
    # stair cheek walls
    for y in (-12.8, 12.8):
        m.box(x_front-run, x_front, y-0.8, y+0.8, 0, Z_PODIUM+0.8, 'sandstone', solid=True)
    # the wall behind the columns (the portal), doors bricked up
    m.box(-46.5, -43.3, -15.7, 16.0, Z_PODIUM, Z_ENT, 'sandstone', solid=True)
    for k in range(3):
        c = -8.1+8.1*k
        m.box(-46.8, -46.5, c-1.6, c+1.6, Z_PODIUM, Z_PODIUM+6.0, 'brick_infill')
        m.box(-46.9, -46.8, c-0.15, c+0.15, Z_PODIUM+1.3, Z_PODIUM+1.66, 'plaster_dark')
        m.box(-46.9, -46.5, c-1.9, c+1.9, Z_PODIUM+6.0, Z_PODIUM+6.5, 'sandstone_light')
    # side walls of the portico (antae)
    for y0, y1 in ((-15.7, -13.9), (14.2, 16.0)):
        m.box(x_front+0.4, -46.5, y0, y1, Z_PODIUM, Z_ORDER, 'sandstone', solid=True)
    # columns: front row of six, a second row of four behind (BU 8573)
    col_y = [-13.5+5.4*k for k in range(6)]
    for y in col_y:
        for x, r in ((-50.4, 0.86), ):
            m.box(x-1.05, x+1.05, y-1.05, y+1.05, Z_PODIUM, Z_PODIUM+0.6, 'sandstone_light')
            m.cyl(x, y, Z_PODIUM+0.6, Z_ORDER-1.6, r, r*0.86, 'sandstone', seg=16, solid=True)
            m.cyl(x, y, Z_ORDER-1.6, Z_ORDER, r*0.86, r*1.25, 'sandstone_light', seg=12)
            m.box(x-1.15, x+1.15, y-1.15, y+1.15, Z_ORDER-0.25, Z_ORDER, 'sandstone_light')
            # shot scars: dark patches on some shafts
            if h01(f'col{y}') < 0.7:
                zz = Z_PODIUM+2+8*h01(f'col{y}z')
                m.box(x-r-0.02, x-r+0.05, y-0.35, y+0.35, zz, zz+1.2, 'sandstone_dark')
    for y in col_y[1:-1]:
        x = -47.7
        m.cyl(x, y, Z_PODIUM+0.6, Z_ORDER-1.6, 0.8, 0.7, 'sandstone', seg=12, solid=True)
        m.cyl(x, y, Z_ORDER-1.6, Z_ORDER, 0.7, 1.0, 'sandstone_light', seg=10)
    # entablature over the portico (west face carries the inscription)
    m.box(x_front-0.3, -45.0, -16.0, 16.3, Z_ORDER, Z_ORDER+1.3, 'sandstone')
    m.box(x_front-0.1, -45.0, -16.0, 16.3, Z_ORDER+1.3, Z_ENT-0.7, 'sandstone')
    m.box(x_front-1.0, -45.0, -16.6, 16.9, Z_ENT-0.7, Z_ENT, 'sandstone_light')
    # "DEM DEUTSCHEN VOLKE", 60 cm letters (Behrens/Loevy, 1916), as BU 8573 shows it in 1945:
    # the D of DEUTSCHEN and the V of VOLKE shot away.
    place = Matrix(((0, 0, -1, x_front-0.12), (-1, 0, 0, 0.15), (0, 1, 0, Z_ORDER+1.48), (0, 0, 0, 1)))
    m.text('DEM   EUTSCHEN   OLKE', 0.78, 'bronze', place, depth=0.08)
    # pediment, its tympanum relief as a recessed field with figure blocks
    m.gable(x_front-1.0, -45.0, -16.6, 16.9, Z_ENT, 6.4, 'x', 'sandstone', solid=True)
    m.gable(x_front-1.1, x_front-0.9, -14.4, 14.7, Z_ENT+0.5, 5.0, 'x', 'sandstone_dark')
    # the tympanum relief (Germania among the estates): low figure masses, not a survey of it
    for k in range(17):
        y = -12.0+1.5*k
        hgt = (4.4*(1-abs(y)/15.5))*(0.55+0.45*h01(f'tym{k}'))
        if hgt > 0.5:
            m.box(x_front-1.5+0.2*h01(f'tymd{k}'), x_front-1.05, y-0.5-0.3*h01(f'tymw{k}'), y+0.5,
                  Z_ENT+0.55, Z_ENT+0.55+hgt, 'sandstone', part='statues')
            m.box(x_front-1.6, x_front-1.3, y-0.2, y+0.2, Z_ENT+0.55+hgt*0.75, Z_ENT+0.55+hgt*0.95, 'sandstone', part='statues')
    # the portal's raised attic behind the pediment and the Germania group on it (Begas)
    m.box(-47.0, -41.0, -10.0, 10.0, Z_ENT, Z_ENT+6.6, 'sandstone', solid=True)
    m.box(-47.4, -40.6, -10.4, 10.4, Z_ENT+6.6, Z_ENT+7.2, 'sandstone_light')
    rider(m, -44.0, 0.0, Z_ENT+7.2, 1.55, 'bronze')
    for y in (-8.0, 8.0):
        figure(m, -44.0, y, Z_ENT+7.2, 2.6, 'bronze')
    # 1945: brick blast walls in front of the portico's base (BU 8573)
    for y0, y1 in ((-14.0, -4.5), (4.5, 14.0)):
        m.box(x_front-run-2.4, x_front-run-1.9, y0, y1, 0, 4.2, 'brick', solid=True)
    m.box(x_front-run-2.4, x_front-run+1.0, -14.5, -14.0, 0, 4.2, 'brick', solid=True)
    m.box(x_front-run-2.4, x_front-run+1.0, 14.0, 14.5, 0, 4.2, 'brick', solid=True)
    # ── the east portal: a projection with four engaged columns and an attic; the two copper
    # heralds on horseback (Maison, 1896) at its roof corners
    m.solid(43.3, 52.1, -14.2, 13.3, 0, Z_ENT)
    m.box(43.3, 50.6, -14.2, 13.3, 0, Z_ENT, 'sandstone')
    bay_windows(m, 'x', 52.1, 1, -14.2, 13.3, 3, 'east', pilasters=False)
    for k in range(4):
        y = -11.6+7.6*k
        m.cyl(52.8, y, Z_STRING, Z_ORDER-1.0, 0.8, 0.7, 'sandstone', seg=12, solid=True)
        m.box(51.9, 53.8, y-0.95, y+0.95, Z_ORDER-1.0, Z_ORDER, 'sandstone_light')
    m.box(52.1, 54.0, -14.6, 13.7, Z_ORDER, Z_ENT, 'sandstone')
    m.box(44.0, 51.0, -11.0, 10.0, Z_ENT, Z_ENT+5.0, 'sandstone', solid=True)
    for y in (-12.6, 11.8):
        rider(m, 50.4, y, Z_ENT, 1.5, 'copper_patina', facing='x')
    # ── the dome: masonry drum, four corner turrets, the bare steel ribs, the lantern frame
    dome(m)
    return m


def octagon(h, c):
    """Octagon of half-size h with corner cut c, as a closed list of (x, y)."""
    return [(h, -h+c), (h, h-c), (h-c, h), (-h+c, h), (-h, h-c), (-h, -h+c), (-h+c, -h), (h-c, -h)]


def dome(m):
    H, C = 17.0, 5.5
    z_drum0, z_drum1 = Z_ROOF, 37.6
    # the drum: an octagonal masonry ring with windows (the glass behind them gone)
    pts = octagon(H, C)
    m.prism(pts, z_drum0, z_drum0+1.0, 'sandstone', solid=True, part='dome drum', excess=round(C/2, 3))
    for i in range(8):
        a, b = pts[i], pts[(i+1) % 8]
        L = math.hypot(b[0]-a[0], b[1]-a[1])
        dx, dy = (b[0]-a[0])/L, (b[1]-a[1])/L
        nx, ny = dy, -dx                       # outward for a counter-clockwise list
        n = 3 if L > 10 else 1
        for k in range(n):
            t0, t1 = (k+0.2)/n, (k+0.8)/n
            # piers between drum windows
        for k in range(n+1):
            t = k/n
            px, py = a[0]+dx*L*t, a[1]+dy*L*t
            m.prism([(px-0.9*dx-0.0*nx, py-0.9*dy), (px+0.9*dx, py+0.9*dy),
                     (px+0.9*dx-1.4*nx, py+0.9*dy-1.4*ny), (px-0.9*dx-1.4*nx, py-0.9*dy-1.4*ny)],
                    z_drum0+1.0, z_drum1, 'sandstone')
        # sill and lintel bands
        m.prism([(a[0], a[1]), (b[0], b[1]), (b[0]-1.2*nx, b[1]-1.2*ny), (a[0]-1.2*nx, a[1]-1.2*ny)],
                z_drum1-1.6, z_drum1, 'sandstone_light')
        m.prism([(a[0], a[1]), (b[0], b[1]), (b[0]-1.2*nx, b[1]-1.2*ny), (a[0]-1.2*nx, a[1]-1.2*ny)],
                z_drum0+1.0, z_drum0+2.6, 'sandstone')
    m.solid(-H+C*0.5, H-C*0.5, -H+C*0.5, H-C*0.5, z_drum0, z_drum1)
    # corner turrets on the drum's diagonals, with their stone crowns (BU 8573, Khaldei)
    for sx in (-1, 1):
        for sy in (-1, 1):
            cx, cy = sx*(H-C/2+0.6), sy*(H-C/2+0.6)
            m.box(cx-2.2, cx+2.2, cy-2.2, cy+2.2, Z_ROOF, z_drum1+3.2, 'sandstone', solid=True)
            m.box(cx-2.6, cx+2.6, cy-2.6, cy+2.6, z_drum1+3.2, z_drum1+3.8, 'sandstone_light')
            m.box(cx-1.6, cx+1.6, cy-1.6, cy+1.6, z_drum1+3.8, z_drum1+6.4, 'sandstone')
            m.cyl(cx, cy, z_drum1+6.4, z_drum1+7.6, 1.5, 1.1, 'sandstone_light', seg=8)
            m.cyl(cx, cy, z_drum1+7.6, z_drum1+9.6, 0.9, 0.5, 'sandstone_dark', seg=8)   # crown
            m.cyl(cx, cy, z_drum1+9.6, z_drum1+10.4, 0.3, 0.05, 'sandstone_dark', seg=6)
    # the cupola's steel frame: meridian ribs and ring purlins on a domical octagon
    z0, rise = z_drum1, 17.5
    h_top = 4.0
    rings = 10
    def ring_pts(s):
        h = H-0.8-(H-0.8-h_top)*(1-math.cos(s*math.pi/2))
        c = C*h/(H-0.8)
        z = z0+rise*math.sin(s*math.pi/2)
        return [(x, y, z) for x, y in octagon(h, c)]
    def perim(pts, f):
        """Point at fraction f of the octagon's perimeter."""
        segs = [(pts[i], pts[(i+1) % 8]) for i in range(8)]
        lens = [math.dist(a[:2], b[:2]) for a, b in segs]
        tot = sum(lens)
        d = f*tot
        for (a, b), L in zip(segs, lens):
            if d <= L:
                t = d/L
                return tuple(a[k]+(b[k]-a[k])*t for k in range(3))
            d -= L
        return segs[-1][1]
    meridians = [i/40 for i in range(40)]
    levels = [ring_pts(k/rings) for k in range(rings+1)]
    for mi, f in enumerate(meridians):
        for k in range(rings):
            if h01(f'mer{mi}.{k}') < 0.07 and k > 2:
                continue                     # a rib section blown out
            m.beam(perim(levels[k], f), perim(levels[k+1], f), 0.32, 0.45, 'steel')
    for k in range(1, rings+1):
        for mi in range(40):
            if h01(f'ring{k}.{mi}') < 0.10 and k not in (rings,):
                continue
            m.beam(perim(levels[k], meridians[mi]), perim(levels[k], (mi+1)/40 if mi < 39 else 0.0), 0.22, 0.3, 'steel')
    # base ring
    for mi in range(40):
        m.beam(perim(levels[0], meridians[mi]), perim(levels[0], (mi+1)/40 if mi < 39 else 0.0), 0.5, 0.6, 'steel')
    # the lantern: an octagonal frame, bent and partly missing (BU 8573)
    zl0 = z0+rise
    lp = [(x, y) for x, y in octagon(h_top, h_top*0.35)]
    for i, (x, y) in enumerate(lp):
        if h01(f'lan{i}') < 0.25:
            continue
        lean = 0.6 if i in (2, 3) else 0.0
        m.beam((x, y, zl0), (x*1.0+lean, y, zl0+6.0), 0.3, 0.3, 'steel')
    for z in (zl0+3.0, zl0+6.0):
        for i in range(8):
            if h01(f'lr{z}{i}') < 0.2:
                continue
            a, b = lp[i], lp[(i+1) % 8]
            m.beam((a[0], a[1], z), (b[0], b[1], z), 0.22, 0.22, 'steel')
    # the crown's frame over the lantern
    for i in range(0, 8, 2):
        x, y = lp[i]
        m.beam((x, y, zl0+6.0), (x*0.25, y*0.25, zl0+11.5), 0.25, 0.25, 'steel')
    m.beam((0, 0, zl0+11.0), (0, 0, z_drum1+rise+15.5), 0.3, 0.3, 'steel')
    m.no_collision.add('dome steel ribs and lantern frame')


def main():
    a = K.args()
    out = a[0] if a else 'assets/worlds/berlin1945'
    prev = a[1] if len(a) > 1 else None
    K.clear_scene()
    m = build()
    cams = None
    if prev:
        cams = (prev, [
            ('west_bu8573', (-150.0, -12.0, 1.7), (-45.0, 0.0, 24.0), 35),
            ('wnw_khaldei', (-120.0, 75.0, 1.6), (-20.0, 10.0, 22.0), 28),
            ('south_bu8922', (25.0, -230.0, 2.0), (0.0, 0.0, 26.0), 45),
            ('aerial', (-260.0, -220.0, 210.0), (0.0, 0.0, 10.0), 40),
        ])
    m.export(out, cams)


if __name__ == '__main__':
    main()
