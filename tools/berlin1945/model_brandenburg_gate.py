"""The Brandenburg Gate and its two gatehouses as they stood in May 1945.

    blender --background --python tools/berlin1945/model_brandenburg_gate.py -- OUT_DIR [PREVIEW_PREFIX]

Writes OUT_DIR/brandenburg_gate_1945.glb and its colliders (bl_kit.py). Model frame: +X along the
passages toward Pariser Platz and Unter den Linden (the gate's east face), +Y along the colonnade
to the north, origin at ground level in the middle of the central passage, halfway between the
column rows. The map turns it to Unter den Linden's bearing (83.7 deg, OSM).

Sources (docs/BERLIN_REICHSTAG_1945.md): Langhans 1788-91; width 62.5 m over the gatehouses,
depth 11 m, height 20.3 m without the Quadriga and about 26 m with it (de.wikipedia); twelve
fluted Doric columns, six a side, 13.5 m high, 1.73 m at the foot; five passages, the middle one
5.65 m and the others 3.80 m; OSM way 518071791 (the gate, 34.7 m along the colonnade) and ways
518071762 / 375247938 (the gatehouses, about 19 x 13 m, east of the gate's ends). 1945 state:
Thurgood's west view (AIP, 1945) with the shell hole in the attic; Donath's two 1945 views
(Bundesarchiv 183-M1015-331 and -334): the Quadriga a wreck, the gatehouses burnt out with
their Doric fronts and east pediments standing; Khaldei's May 1945 view from the top of the
gate, one horse's head and neck still on the group.
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import bl_kit as K  # noqa: E402

h01 = K.h01

Z_STYL = 0.6
COL_H = 13.5
Z_CAP = Z_STYL+COL_H          # 14.1
Z_ARCH = Z_CAP+1.4            # architrave top
Z_FRIEZE = Z_ARCH+1.5
Z_CORN = Z_FRIEZE+0.65        # 17.65
Z_ATTIC1 = 19.0
Z_ATTIC2 = 20.3
COL_X = 4.63
COL_Y = [-14.75, -9.22, -3.69, 3.69, 9.22, 14.75]
END_Y = 17.4
R0, R1 = 1.73/2, 1.45/2


def doric(m, x, y, z0, h, r0, r1, key, seg=16, mat='sandstone'):
    m.cyl(x, y, z0, z0+h-0.75, r0, r1, mat, seg=seg, solid=True, part='column')
    m.cyl(x, y, z0+h-0.75, z0+h-0.3, r1, r1*1.32, 'sandstone_light', seg=seg)
    m.box(x-r1*1.45, x+r1*1.45, y-r1*1.45, y+r1*1.45, z0+h-0.3, z0+h, 'sandstone_light')
    # flutes read as thin dark ribs on the shaft; bullet and splinter scars as darker patches
    for k in range(0, seg, 2):
        a = 2*math.pi*(k+0.5)/seg
        m.beam((x+r0*0.99*math.cos(a), y+r0*0.99*math.sin(a), z0+0.1),
               (x+r1*0.99*math.cos(a), y+r1*0.99*math.sin(a), z0+h-0.8), 0.07, 0.05, 'sandstone_dark')
    for k in range(3):
        if h01(f'{key}.scar{k}') < 0.65:
            a = 2*math.pi*h01(f'{key}.sa{k}')
            zz = z0+1.0+(h-3)*h01(f'{key}.sz{k}')
            rr = r0-(r0-r1)*(zz-z0)/h
            m.box(x+rr*math.cos(a)-0.25, x+rr*math.cos(a)+0.25, y+rr*math.sin(a)-0.25, y+rr*math.sin(a)+0.25,
                  zz, zz+0.4+0.6*h01(f'{key}.sh{k}'), 'sandstone_dark')


def gate(m):
    # plinths under the column rows and the walls; the passages are roadway
    for y in COL_Y:
        for x in (-COL_X, COL_X):
            m.box(x-1.2, x+1.2, y-1.2, y+1.2, 0, Z_STYL, 'granite', solid=True)
            doric(m, x, y, Z_STYL, COL_H, R0, R1, f'c{x:.0f}{y:.1f}')
    # the walls between the passages, from front to back row
    for y in COL_Y[1:-1]:
        m.box(-COL_X+0.5, COL_X-0.5, y-0.72, y+0.72, 0, Z_CAP, 'sandstone', solid=True)
        m.box(-COL_X+0.4, COL_X-0.4, y-0.9, y+0.9, 0, Z_STYL+0.4, 'granite', solid=True)
        # a plain impost band in the passages
        m.box(-COL_X+0.6, COL_X-0.6, y-0.82, y+0.82, Z_CAP-1.2, Z_CAP-0.9, 'sandstone_light')
    # the end blocks behind the outer columns
    for s in (-1, 1):
        y0, y1 = sorted((s*14.0, s*END_Y))
        m.box(-COL_X+0.2, COL_X-0.2, y0, y1, 0, Z_CAP, 'sandstone', solid=True)
        # an engaged anta at each end corner
        for x in (-COL_X, COL_X):
            m.box(x-0.9, x+0.9, *sorted((s*(END_Y-1.5), s*END_Y)), Z_STYL, Z_CAP, 'sandstone')
            m.box(x-1.2, x+1.2, *sorted((s*(END_Y-1.7), s*(END_Y+0.2))), 0, Z_STYL, 'granite', solid=True)
        # niches in the end blocks' outer faces (passage-side niches with statues are not modelled)
        m.box(-1.6, 1.6, *sorted((s*END_Y, s*(END_Y+0.02))), 4.0, 9.5, 'sandstone_dark')
    # architrave, triglyph frieze, cornice
    m.box(-5.6, 5.6, -END_Y-0.2, END_Y+0.2, Z_CAP, Z_ARCH, 'sandstone', solid=True)
    m.box(-5.45, 5.45, -END_Y-0.1, END_Y+0.1, Z_ARCH, Z_FRIEZE, 'sandstone', solid=True)
    tri_y = []
    for a, b in zip(COL_Y, COL_Y[1:]):
        tri_y += [a, (a+b)/2]
    tri_y.append(COL_Y[-1])
    tri_y = [-END_Y+0.6]+tri_y+[END_Y-0.6]
    for x, s in ((-5.45, -1), (5.45, 1)):
        for k, y in enumerate(tri_y):
            m.box(*sorted((x, x+s*0.12)), y-0.45, y+0.45, Z_ARCH+0.05, Z_FRIEZE, 'sandstone_light')
            # metopes between triglyphs: carved reliefs; a few knocked out
            if k+1 < len(tri_y) and h01(f'met{x}{k}') < 0.12:
                yy = (y+tri_y[k+1])/2
                m.box(*sorted((x-s*0.05, x+s*0.02)), yy-0.5, yy+0.5, Z_ARCH+0.3, Z_FRIEZE-0.3, 'sandstone_dark')
    u = -END_Y-0.6
    while u < END_Y+0.6-1e-6:
        e = min(u+2.5, END_Y+0.6)
        if h01(f'corn{u:.1f}') > 0.12:
            m.box(-6.2, 6.2, u, e, Z_FRIEZE, Z_CORN, 'sandstone_light')
        else:
            m.box(-5.5, 5.5, u, e, Z_FRIEZE, Z_CORN-0.2, 'sandstone_dark')
        u = e
    m.solid(-6.2, 6.2, -END_Y-0.6, END_Y+0.6, Z_FRIEZE, Z_CORN)
    # the stepped attic with its relief fields (Schadow's procession on the east and west)
    m.box(-5.0, 5.0, -16.9, 16.9, Z_CORN, Z_ATTIC1, 'sandstone', solid=True)
    m.box(-4.4, 4.4, -11.4, 11.4, Z_ATTIC1, Z_ATTIC2, 'sandstone', solid=True)
    m.box(-4.6, 4.6, -11.6, 11.6, Z_ATTIC2-0.25, Z_ATTIC2, 'sandstone_light')
    for x, s in ((-5.0, -1), (5.0, 1)):
        m.box(*sorted((x, x+s*0.06)), -8.8, 8.8, Z_CORN+0.35, Z_ATTIC1-0.15, 'sandstone_light')
        for k in range(14):
            y = -8.2+1.25*k
            hh = 0.7+0.5*h01(f'rel{x}{k}')
            m.box(*sorted((x+s*0.06, x+s*0.22)), y-0.35, y+0.35, Z_CORN+0.45, Z_CORN+0.45+hh, 'sandstone_dark',
                  part='attic relief')
    # 1945: a shell hole in the west face of the attic, north of the centre (Thurgood, 1945)
    m.box(-5.05, -4.0, 4.2, 6.4, Z_CORN+0.3, Z_ATTIC1-0.2, 'soot')
    m.box(-5.1, -4.95, 3.6, 7.0, Z_CORN+0.1, Z_ATTIC1, 'sandstone_dark')
    # the Quadriga's pedestal and its wreck (copper): the chariot's shell, four horses broken at
    # the body or the legs, one head and neck still up (Khaldei, May 1945); facing east
    m.box(-2.6, 2.6, -3.3, 3.3, Z_ATTIC2, Z_ATTIC2+0.9, 'sandstone', solid=True)
    zq = Z_ATTIC2+0.9
    m.box(-2.2, -0.6, -1.1, 1.1, zq, zq+1.5, 'copper_patina', part='quadriga')          # chariot remnant
    m.beam((-1.2, -0.9, zq+1.4), (-0.2, 1.4, zq+2.6), 0.08, 0.6, 'copper_patina')         # torn sheet
    for k, y in enumerate((-2.4, -0.85, 0.85, 2.4)):
        for dx in (0.4, 1.9):                 # legs
            if h01(f'leg{k}{dx}') < 0.75:
                m.box(dx-0.12, dx+0.12, y-0.3, y-0.08, zq, zq+1.3+0.5*h01(f'lh{k}{dx}'), 'copper_patina', part='quadriga')
                m.box(dx-0.12, dx+0.12, y+0.08, y+0.3, zq, zq+1.0+0.8*h01(f'lj{k}{dx}'), 'copper_patina', part='quadriga')
        if k in (0, 2, 3):
            # a body broken off behind the shoulder
            m.box(0.9, 2.2, y-0.32, y+0.32, zq+1.3, zq+2.1, 'copper_patina', part='quadriga')
        if k == 3:
            m.box(-0.1, 2.2, y-0.34, y+0.34, zq+1.3, zq+2.2, 'copper_patina', part='quadriga')
            m.beam((2.1, y, zq+2.0), (2.9, y, zq+3.2), 0.45, 0.4, 'copper_patina')       # neck
            m.box(2.7, 3.5, y-0.2, y+0.2, zq+3.0, zq+3.5, 'copper_patina', part='quadriga')   # head
    m.no_collision.add('quadriga wreck')


def gatehouse(m, s, key):
    """A gatehouse (Torhaus), burnt out: Doric colonnade on the side toward the passages,
    tetrastyle front with pediment toward Pariser Platz (east), plain west end."""
    x0, x1 = 1.6, 20.2
    yi, yo = s*END_Y, s*(END_Y+13.0)          # inner (passage side) and outer faces
    y0, y1 = sorted((yi, yo))
    zst = 0.5
    ch = 7.0
    zc = zst+ch
    ze = zc+1.6
    # stylobate
    m.box(x0, x1, y0, y1, 0, zst, 'granite', solid=True)
    # the cella walls (burnt out: no roof, sooted tops, gaping windows); the colonnade side is
    # set back 2.6 m behind the columns
    yc = yi+s*2.6
    cy0, cy1 = sorted((yc, yo))
    t = 0.8
    win = []
    for k in range(3):
        c = x0+3.5+(x1-x0-7.0)*k/2
        win.append((c-0.8, c+0.8, zst+1.6, zst+4.6))
    m.wall('y', *sorted((yc, yc+s*t)), x0+2.4, x1-2.4, zst, ze, win, 'sandstone')
    m.wall('y', *sorted((yo, yo-s*t)), x0+2.4, x1-2.4, zst, ze, win, 'sandstone')
    m.wall('x', x0+2.4, x0+2.4+t, cy0, cy1, zst, ze, [], 'sandstone')
    m.wall('x', x1-2.4-t, x1-2.4, cy0, cy1, zst, ze, [(min(cy0, cy1)+3.2, max(cy0, cy1)-3.2, zst, zst+4.2)], 'sandstone')
    # soot over the openings and along the burnt wall heads
    for (a0, a1, b0, b1) in win:
        for plane in (yc, yo):
            p0, p1 = sorted((plane-s*0.03, plane+s*0.0)) if plane == yc else sorted((plane, plane+s*0.03))
            m.box(a0+0.2, a1-0.2, p0, p1, b1, b1+1.6, 'soot')
    m.box(x0+2.4, x1-2.4, cy0, cy1, ze-0.5, ze-0.45, 'timber_charred')   # the burnt roof's beam stubs line
    for k in range(6):
        if h01(f'{key}.beam{k}') < 0.5:
            bx = x0+2.0+k*2.8
            m.beam((bx, cy0+0.4, ze-0.6), (bx+0.4, cy1-0.4-1.5*h01(f'{key}.bl{k}'), ze-1.4), 0.25, 0.3, 'timber_charred')
    # floor rubble inside the shell (low; no box: the shell's walls hold it)
    m.box(x0+3.4, x1-3.4, cy0+1.0, cy1-1.0, zst, zst+0.5, 'rubble')
    # the colonnade toward the passages: six columns
    for k in range(6):
        x = x0+1.4+(x1-x0-2.8)*k/5
        doric(m, x, yi+s*0.9, zst, ch, 0.5, 0.42, f'{key}.col{k}', seg=12)
    m.box(x0, x1, *sorted((yi+s*0.2, yc+s*t)), zc, ze, 'sandstone', solid=True)     # entablature
    # the east front: four columns and a pediment
    for k in range(4):
        y = yc+s*(1.4+(13.0-2.6-2.8)*k/3)
        doric(m, x1-0.9, y, zst, ch, 0.5, 0.42, f'{key}.ecol{k}', seg=12)
    m.box(x1-1.8, x1, y0, y1, zc, ze, 'sandstone', solid=True)
    m.gable(x1-1.8, x1+0.15, y0-0.15, y1+0.15, ze, 2.4, 'x', 'sandstone', solid=True)
    m.gable(x1+0.1, x1+0.18, y0+1.0, y1-1.0, ze+0.25, 1.7, 'x', 'sandstone_dark')
    # west end toward the Tiergarten: four columns under a flat entablature (Thurgood, 1945)
    m.box(x0, x0+1.6, y0, y1, zc, ze, 'sandstone', solid=True)
    for k in range(4):
        y = yc+s*(1.4+(13.0-2.6-2.8)*k/3)
        doric(m, x0+0.9, y, zst, ch, 0.5, 0.42, f'{key}.wcol{k}', seg=12)


def build():
    m = K.Model('brandenburg_gate_1945', 'tools/berlin1945/model_brandenburg_gate.py')
    gate(m)
    for s in (-1, 1):
        gatehouse(m, s, f'th{s}')
    return m


def main():
    a = K.args()
    out = a[0] if a else 'assets/worlds/berlin1945'
    prev = a[1] if len(a) > 1 else None
    K.clear_scene()
    m = build()
    cams = None
    if prev:
        cams = (prev, [
            ('west_thurgood', (-75.0, 0.0, 1.7), (0.0, 0.0, 12.0), 35),
            ('southeast_donath331', (60.0, -45.0, 14.0), (0.0, 8.0, 12.0), 35),
            ('east_platz', (55.0, -18.0, 1.7), (0.0, 0.0, 11.0), 28),
        ])
    m.export(out, cams)


if __name__ == '__main__':
    main()
