"""Landmark shells around the Reichstag in May 1945, for the Berlin 1945 map.

    blender --background --python tools/berlin1945/model_landmarks.py -- OUT_DIR [PREVIEW_PREFIX]

Each model's frame: +X east (or along its own axis where stated), +Y north, origin at ground
level at the footprint centre; the map places and turns them (tools/worldgen/berlin_reichstag.py).
Sources and their limits: docs/BERLIN_REICHSTAG_1945.md.

- kroll_opera_1945: the Kroll Opera on the west side of Koenigsplatz (the Reichstag's chamber
  from 1933), bombed out in November 1943 and fought over on 30 April 1945. Footprint from the
  1928 aerial survey (about 75 x 110 m: an east front block on the square, the auditorium, the
  stage house to the west); the 1945 ruin from Donath's photograph (Bundesarchiv 183-M1015-322):
  a gutted two-storey front block with arched windows and the stage house's gable with its great
  arched opening.
- interior_ministry_1945 and interior_ministry_ne_wing_1945: the former General Staff building
  on the north-west of Koenigsplatz by the Moltke Bridge, which housed the Reich Ministry of the
  Interior; taken room by room by the 150th Rifle Division on 29 April 1945 (Beevor 2002). Plan
  from the 1928 aerial survey (a block about 93 x 156 m, its north-east side on the diagonal
  street from the bridge); four storeys, burnt and shelled. Elevation detail is an estimate:
  no 1945 photograph was found.
- swiss_legation_1945: the Swiss legation at Fuerst-Bismarck-Strasse / Moltkestrasse, the one
  building of the Alsenviertel to survive (OSM relation 2886766, the historic part); damaged.
- reichstag_presidents_palace_1945: the Reichstag President's palace across Hermann-Goering-
  Strasse (OSM way 37408952, 60 x 34 m; Wallot 1904), roofed and damaged.
- hotel_adlon_1945: the Adlon at the south-east corner of Pariser Platz, undamaged at the
  surrender on 2 May 1945 (it burned in the following days; de.wikipedia Hotel Adlon).
- moltke_bridge_1945 and floor_ceiling_moltke_bridge_1945: the Moltke Bridge (Stahn/Hobrecht
  1886-91; 78 m, 26.7 m wide, red Main sandstone, river arches of 16.3, 16.3 and 17 m), barricaded
  at both ends, its southern river arch blown by the defenders' charges on 28 April 1945 and still
  passable to infantry (Beevor 2002, via en.wikipedia Moltke Bridge). Frame: +X along the bridge
  from its south end to its north end, +Y across. The second model is the deck's walkable top
  (src/walk_surfaces.rs reads `floor_ceiling_` slabs for people on foot); the barricades, not the
  deck, are what stop a tank, as in April 1945.
- ditch_water_1945: a unit slab of standing water for the flooded tunnel cut across Koenigsplatz
  and the Humboldthafen, scaled by the map (the same opaque, smooth water surface as the river:
  docs/PHYSICAL_TRUTH_AUDIT.md river water row).
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import bl_kit as K  # noqa: E402
import bl_city as C  # noqa: E402

h01 = K.h01


def kroll():
    m = K.Model('kroll_opera_1945', 'tools/berlin1945/model_landmarks.py')
    # east front block on the square: two tall storeys, arcaded, gutted
    C.shell(m, 22.5, 37.5, -55.0, 55.0, 2, 'gutted', 'kroll.front', storey_h=6.0, ground_h=6.5, bay=4.4,
            w_frac=0.55, wall_mat='plaster', trim_mat='sandstone', faces=('e', 'w', 'n', 's'), shop=False)
    # side wings
    for s in (-1, 1):
        y0, y1 = sorted((s*35.0, s*55.0))
        C.shell(m, -20.0, 22.5, y0, y1, 2, 'gutted', f'kroll.wing{s}', storey_h=5.0, ground_h=5.5, bay=4.0,
                w_frac=0.45, wall_mat='plaster', trim_mat='sandstone', faces=('n', 's', 'w'), party=('e',), shop=False)
    # the auditorium: high walls, roofless
    C.shell(m, -10.0, 22.5, -35.0, 35.0, 3, 'gutted', 'kroll.aud', storey_h=5.6, ground_h=6.0, bay=6.0,
            w_frac=0.25, wall_mat='plaster', trim_mat='plaster_dark', faces=('n', 's'), party=('e', 'w'), shop=False)
    # the stage house with its gable and great arched opening (Donath 1945)
    top = C.shell(m, -37.5, -10.0, -28.0, 28.0, 5, 'gutted', 'kroll.stage', storey_h=5.2, ground_h=6.0, bay=7.0,
                  w_frac=0.22, wall_mat='plaster', trim_mat='plaster_dark', faces=('n', 's', 'w'), party=('e',),
                  shop=False)
    t = 0.75
    r = 5.0
    pts = [(-28.0, top), (-r, top)]
    for k in range(9):
        a = math.pi-math.pi*k/8
        pts.append((r*math.cos(a), top+r*math.sin(a)))
    pts += [(r, top), (28.0, top), (0.0, top+9.0)]
    m.vprism(pts, 'x', -10.0-t, -10.0, 'plaster', solid=True, part='kroll gable', excess=4.5)
    m.wall('x', -10.0-t, -10.0, -r, r, top-6.0, top, [(-r+0.3, r-0.3, top-6.0, top)], 'plaster')
    m.box(-10.0-t-0.5, -10.0+0.5, -28.5, 28.5, top+0.0, top+0.4, 'sandstone')
    return m


def ministry():
    m = K.Model('interior_ministry_1945', 'tools/berlin1945/model_landmarks.py')
    kw = dict(storey_h=4.8, ground_h=5.6, bay=4.2, w_frac=0.40, wall_mat='brick', trim_mat='sandstone', shop=False)
    C.shell(m, -46.5, -30.5, -78.0, 78.0, 4, 'gutted', 'mi.west', faces=('w', 'e', 'n', 's'), **kw)
    C.shell(m, -30.5, 46.5, -78.0, -62.0, 4, 'collapsed', 'mi.south', faces=('s', 'n', 'e'), party=('w',),
            collapse_side='n', **kw)
    C.shell(m, 30.5, 46.5, -62.0, 22.0, 4, 'gutted', 'mi.east', faces=('e', 'w', 'n'), party=('s',), **kw)
    # the south front's central projection (light-roofed in the 1928 aerial), an entrance risalit
    C.shell(m, -14.0, 8.0, -80.0, -62.0, 4, 'gutted', 'mi.risalit', faces=('s',), party=('w', 'e', 'n'),
            storey_h=4.8, ground_h=5.6, bay=3.6, w_frac=0.45, wall_mat='sandstone', trim_mat='sandstone_light',
            shop=False)
    wing = K.Model('interior_ministry_ne_wing_1945', 'tools/berlin1945/model_landmarks.py')
    C.shell(wing, -53.0, 53.0, -8.0, 8.0, 4, 'gutted', 'mi.ne', faces=('n', 's'), party=('e', 'w'), **kw)
    return [m, wing]


def swiss():
    m = K.Model('swiss_legation_1945', 'tools/berlin1945/model_landmarks.py')
    C.shell(m, -13.0, 13.0, -11.0, 11.0, 3, 'roofed_damaged', 'swiss', storey_h=4.6, ground_h=5.0, bay=3.6,
            w_frac=0.42, wall_mat='plaster', trim_mat='sandstone', faces=('s', 'n', 'e', 'w'), shop=False)
    # sandbagged ground-floor windows of a strongpoint (the Ivengo map marks it held)
    for x in (-9.0, -3.0, 3.0, 9.0):
        m.box(x-1.2, x+1.2, -12.2, -11.0, 0.0, 1.6, 'rubble', solid=True)
    return m


def palace():
    m = K.Model('reichstag_presidents_palace_1945', 'tools/berlin1945/model_landmarks.py')
    C.shell(m, -30.0, 30.0, -17.0, 17.0, 3, 'roofed_damaged', 'rpp', storey_h=5.0, ground_h=5.4, bay=4.0,
            w_frac=0.42, wall_mat='sandstone_light', trim_mat='sandstone', faces=('w', 'e', 'n', 's'), shop=False)
    return m


def adlon():
    m = K.Model('hotel_adlon_1945', 'tools/berlin1945/model_landmarks.py')
    C.shell(m, -24.0, 24.0, -20.0, 20.0, 5, 'roofed', 'adlon', storey_h=4.0, ground_h=5.4, bay=3.4,
            w_frac=0.48, wall_mat='plaster', trim_mat='sandstone', faces=('s', 'w', 'n'), party=('e',), shop=False)
    return m


def moltke():
    m = K.Model('moltke_bridge_1945', 'tools/berlin1945/model_landmarks.py')
    deck = K.Model('floor_ceiling_moltke_bridge_1945', 'tools/berlin1945/model_landmarks.py')
    L, W = 39.0, 13.35
    z_bed, z_spring = -7.0, -3.1
    arches = [(-25.6, -8.6), (-6.6, 9.7), (11.7, 28.0)]       # the river spans, south to north
    blown = arches[0]
    # elevation outline of the bridge body: deck line, abutments to the bed, arch intradoses
    def outline(skip_blown):
        pts = [(-L, -0.12), (L, -0.12), (L, z_bed)]
        for (a, b) in reversed(arches):
            span = b-a
            rise = 2.4
            R = (span*span/4+rise*rise)/(2*rise)
            cx = (a+b)/2
            cz = z_spring+rise-R
            pts.append((b, z_bed))
            pts.append((b, z_spring))
            for k in range(1, 12):
                x = b-span*k/12
                pts.append((x, cz+math.sqrt(max(R*R-(x-cx)**2, 0.0))))
            pts.append((a, z_spring))
            pts.append((a, z_bed))
        pts.append((-L, z_bed))
        return pts
    # the body is built in three strips across the width so the blown arch can drop on one side
    strips = [(-W, -4.5), (-4.5, 4.0), (4.0, W)]
    for si, (y0, y1) in enumerate(strips):
        pts = outline(False)
        if si < 2:
            # downstream two thirds of the southern river arch fell into the Spree: cut the body
            # there into two parts either side of the gap
            south = [p for p in pts if p[0] <= blown[0]+0.01]
            north = [p for p in pts if p[0] >= blown[1]-0.01]
            # south part: from the abutment to the blown arch's south springing
            sp = [(-L, -0.12), (blown[0]+1.2, -0.12), (blown[0]+0.4, z_spring+0.6), (blown[0], z_spring),
                  (blown[0], z_bed), (-L, z_bed)]
            m.vprism(sp, 'y', y0, y1, 'sandstone_red', solid=False)
            npts = [(blown[1]-1.0, -0.12), (L, -0.12)]+[p for p in pts[2:] if p[0] >= blown[1]-0.01 and p != (-L, z_bed)]
            npts.append((blown[1], z_bed))
            npts.append((blown[1], z_spring))
            npts.append((blown[1]-0.5, z_spring+0.8))
            m.vprism(npts, 'y', y0, y1, 'sandstone_red', solid=False)
        else:
            m.vprism(pts, 'y', y0, y1, 'sandstone_red', solid=False)
    # collision of the body: the piers and abutments, the deck slab over each intact span
    m.solid(-L, blown[0], -W, W, z_bed, -0.12)
    for (a, b) in arches[1:]:
        m.solid(a, b, -W, W, z_spring+2.4-0.9, -0.12)
    m.solid(blown[0], blown[1], 4.0, W, z_spring+2.4-0.9, -0.12)
    for a, b in ((-8.6, -6.6), (9.7, 11.7), (28.0, L)):
        m.solid(a, b, -W, W, z_bed, -0.12)
    m.bounded.append(('arch spandrels', 2.4))
    # cutwaters on the piers, both faces
    for px in (-7.6, 10.7):
        for s in (-1, 1):
            m.prism([(px-1.0, s*W), (px+1.0, s*W), (px, s*(W+2.2))], z_bed, z_spring+0.5, 'sandstone_red',
                    solid=True, part='cutwater', excess=0.5)
    # parapets (sandstone balustrade), broken over the blown arch and in places
    for s in (-1, 1):
        x = -L
        while x < L-1e-6:
            e = min(x+2.5, L)
            gap = blown[0]-0.5 < x < blown[1]+0.5 and (s < 0 or x < blown[0]+3)
            if not gap and h01(f'par{s}{x:.1f}') > 0.12:
                m.box(x, e, *sorted((s*(W-0.6), s*W)), -0.12, 1.15, 'sandstone_red', solid=True)
            x = e
        # Piper's griffins at the bridge's four corners (stone blocks; decoration)
        for gx in (-L+1.0, L-1.0):
            m.box(gx-1.2, gx+1.2, *sorted((s*(W-1.4), s*W)), 1.15, 3.2, 'sandstone_red', part='griffins')
    # roadway paving and the deck's walkable top (a separate floor slab model)
    for (x0, x1, y0, y1) in ((-L, blown[0]-0.4, -W+0.6, W-0.6), (blown[0]-0.4, blown[1]+0.4, 4.4, W-0.6),
                             (blown[1]+0.4, L, -W+0.6, W-0.6)):
        m.box(x0, x1, y0, y1, -0.12, -0.05, 'granite')
        deck.box(x0, x1, y0, y1, -0.05, 0.0, 'granite')
    # the blown span's wreckage in the river: tilted slabs of the arch, down to the water
    for k in range(5):
        x = blown[0]+2.0+3.0*k
        y = -9.0+3.0*h01(f'wr{k}')
        m.beam((x, y-3.0, -0.8-1.2*h01(f'wz{k}')), (x+1.5, y+3.5, -4.6), 2.6, 0.9, 'sandstone_red')
    m.box(blown[0]+1.0, blown[1]-1.0, -12.0, 3.0, z_bed, -4.2, 'rubble')
    # barricades across both ends (Beevor 2002: the bridge barricaded at both ends): a wall of
    # paving setts and timber cribbing with a gap for men on foot on the downstream side
    for x in (-L+3.5, L-3.5):
        m.box(x-1.0, x+1.0, -W+0.6, W-2.6, -0.05, 2.1, 'rubble', solid=True)
        for k in range(6):
            yy = -W+1.5+3.6*k
            m.beam((x-1.2, yy, 0.4), (x+1.2, yy+1.0, 2.0), 0.3, 0.3, 'timber_charred')
    m.no_collision.add('fallen arch slabs (in the river)')
    return [m, deck]


def lehrter():
    """The Lehrter Bahnhof's train shed (1871) north of the Spree, bombed out: long walls with tall
    arched windows, the head building on the forecourt, the roof's iron arches bare (1928 aerial
    for the plan, about 65 x 155 m; the 1945 state is the city's: no photograph of it was used)."""
    m = K.Model('lehrter_bahnhof_1945', 'tools/berlin1945/model_landmarks.py')
    C.shell(m, -32.5, 32.5, -64.0, 77.5, 2, 'gutted', 'lehrter.shed', storey_h=7.0, ground_h=8.0, bay=8.0,
            w_frac=0.55, wall_mat='sandstone_light', trim_mat='sandstone', faces=('e', 'w'), party=('n',), shop=False)
    C.shell(m, -36.0, 36.0, -77.5, -64.0, 3, 'collapsed', 'lehrter.head', storey_h=5.5, ground_h=7.0, bay=5.0,
            w_frac=0.5, wall_mat='sandstone_light', trim_mat='sandstone', faces=('s', 'e', 'w'), party=('n',),
            shop=False, collapse_side='s')
    for k in range(18):
        y = -60.0+8.0*k
        if h01(f'arch{k}') < 0.25:
            continue
        pts = []
        for i in range(13):
            t = math.pi*i/12
            pts.append((-32.0*math.cos(t), y, 15.0+13.0*math.sin(t)))
        for a_, b_ in zip(pts, pts[1:]):
            if h01(f'arch{k}{a_[0]:.0f}') < 0.12:
                continue
            m.beam(a_, b_, 0.4, 0.7, 'steel')
    for x in (-24.0, -8.0, 8.0, 24.0):
        m.beam((x, -60.0, 15.0+13.0*math.sin(math.acos(x/32.0))), (x, 76.0, 15.0+13.0*math.sin(math.acos(x/32.0))),
               0.3, 0.4, 'steel')
    m.no_collision.add('roof arches')
    return m


def water():
    m = K.Model('ditch_water_1945', 'tools/berlin1945/model_landmarks.py')
    m.box(-0.5, 0.5, -0.5, 0.5, -0.02, 0.0, 'water')
    m.no_collision.add('water surface')
    return m


def main():
    a = K.args()
    out = a[0] if a else 'assets/worlds/berlin1945'
    prev = a[1] if len(a) > 1 else None
    builders = [kroll, ministry, swiss, palace, adlon, moltke, lehrter, water]
    for b in builders:
        K.clear_scene()
        models = b()
        if not isinstance(models, list):
            models = [models]
        for m in models:
            cams = None
            if prev and m.name in ('kroll_opera_1945', 'moltke_bridge_1945', 'interior_ministry_1945'):
                if m.name == 'kroll_opera_1945':
                    cams = (f'{prev}_{m.name}', [('east', (110.0, -40.0, 4.0), (0.0, 0.0, 12.0), 30)])
                elif m.name == 'moltke_bridge_1945':
                    cams = (f'{prev}_{m.name}', [('side', (10.0, -90.0, 2.0), (0.0, 0.0, -2.0), 30)])
                else:
                    cams = (f'{prev}_{m.name}', [('south', (20.0, -200.0, 20.0), (0.0, 0.0, 8.0), 30)])
            m.export(out, cams)
            for o in list(K.bpy.data.objects):
                K.bpy.data.objects.remove(o)


if __name__ == '__main__':
    main()
