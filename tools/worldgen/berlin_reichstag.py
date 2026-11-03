#!/usr/bin/env python3
"""Berlin, the last stand: the Reichstag and a few blocks around it, 28 April - 2 May 1945.

    KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem python3 tools/worldgen/berlin_reichstag.py [--plan PNG]

Writes the workshop assets/worldgen/workshops/berlin-reichstag-v1/ (manifest.v1.json,
manifest_edits.json, heights.npy, mission.v1.json, a receipt) and the ground textures in
assets/terrain/berlin_reichstag_v1/. Needs, in the asset root (gitignored):
- assets/worlds/berlin1945/*.glb + *.colliders.json from tools/berlin1945/build_models.py
  (the Reichstag, the Brandenburg Gate, the landmark shells, the city blocks, props);
- assets/worlds/berlin1945/osm_berlin_mitte.json from tools/berlin1945/fetch_osm.py.

The frame: world origin at the centre of the Reichstag's footprint (OSM relation 2201742), engine
+X east, +Z south (north = -Z), metres; a 1.8 km square. The city is flat (the Spree valley,
about 34 m above sea level): street level is y = 0, the Spree's water 2.8 m below it.

What is where, and on what authority (full list: docs/BERLIN_REICHSTAG_1945.md):
- the Spree's banks, the Humboldthafen and the street lines of the Dorotheenstadt, Moabit and the
  Tiergarten avenues: OpenStreetMap (unchanged since the nineteenth century; checked against the
  1928 aerial survey, Geoportal Berlin luftbild_1928_04);
- Koenigsplatz, the Kroll, the ministry block, the Alsenviertel streets, the Lehrter Bahnhof: the
  1928 aerial survey (traced in this frame, a 6 m north offset of that mosaic corrected), with the
  1938-39 removal of the Victory Column and the Bismarck monument and the 1939-42 clearance of the
  Alsenviertel (de.wikipedia, Platz der Republik and Alsenviertel);
- the flooded cut across Koenigsplatz: Beevor (2002) "a collapsed tunnel ... filled with water
  from the Spree", its line from the Ivengo battle map (Wikimedia Commons, 2010; not a period
  source), about 7 m across here (sources give about 3 m: an estimate);
- the generic blocks: one shell per present-day OSM building plot outside the areas rebuilt on a
  new plan (the Spreebogen, the Ministergaerten, the Hauptbahnhof), the plots of the old blocks;
  1945 conditions in the proportions of the photographs (most gutted, some collapsed, a quarter
  still roofed);
- the warzone: the defenders' trenches and gun pits on Koenigsplatz and round the Reichstag
  (Ivengo map; Beevor), the barricaded Moltke Bridge, shell craters, shattered trees, a wrecked
  8.8 cm-class flak gun in front of the Kroll (Donath 1945), knocked-out armour and the dead
  after the 30 April assault across the square (Beevor: the first attacks cut down in the
  crossfire). No named individual and no invented atrocity; the dead are few and in the open
  where the assault crossed.
The player's tank, route and mission are not historical (CLAUDE.md): the map is the place.
"""
import argparse
import hashlib
import json
import math
import os
import sys
import zlib
from pathlib import Path

H_BASE = None

import numpy as np
from PIL import Image, ImageDraw

ASSET_ROOT = Path(os.environ.get('KESSEL_ASSET_ROOT', '.'))
DEST = ASSET_ROOT/'assets/worldgen/workshops/berlin-reichstag-v1'
TERRAIN_REL = 'assets/terrain/berlin_reichstag_v1'
MODELS = 'assets/worlds/berlin1945/'
OSM_FILE = ASSET_ROOT/MODELS/'osm_berlin_mitte.json'
VERSION = 1
SEED = 0x1945_0430
HALF = 900.0
RES = 901                        # 2.0 m spacing
SPACING = 2*HALF/(RES-1)
TEX = 4096
WATER_Y = -2.8                   # the Spree below street level (regulated at the Muehlendamm)
BED_Y = WATER_Y-3.5
GRID_DEG = 6.3                   # Dorotheenstadt grid: Unter den Linden's bearing 83.7 deg (OSM)
NAT = 'assets/pizza/_extracted/PSX Nature v1.7.3/PSX Nature (Tree branches separated)/Models/GLB (recommended)/'
PZ = 'assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/'
OUTPUT_FILES = ['manifest.v1.json', 'manifest_edits.json', 'heights.npy', 'mission.v1.json']

# ── the landmarks: model, centre (x east, n north), engine yaw (deg) ─────────────────────────────
GATE_X, GATE_N = 108.8, -258.2          # OSM way 518071791, the gate's centre
LANDMARKS = [
    ('reichstag_1945', 0.0, 0.0, 0.0),
    ('brandenburg_gate_1945', GATE_X, GATE_N, 6.5),
    ('kroll_opera_1945', -520.0, -1.0, 0.0),                   # 1928 aerial
    ('interior_ministry_1945', -422.5, 216.0, 0.0),            # 1928 aerial
    ('interior_ministry_ne_wing_1945', -425.2, 259.2, 148.1),  # the diagonal wing on Moltkestrasse
    ('swiss_legation_1945', -339.0, 278.5, 0.0),               # OSM relation 2886766, historic part
    ('reichstag_presidents_palace_1945', 123.0, 10.2, 0.0),    # OSM way 37408952
    ('hotel_adlon_1945', 275.5, -291.0, GRID_DEG),             # OSM 4582978's corner, the old Adlon's size
    ('lehrter_bahnhof_1945', -472.0, 622.0, 0.0),              # 1928 aerial
]
MOLTKE = dict(south=(-463.9, 333.5), north=(-531.6, 396.2))   # OSM way 34833363
KOENIGSPLATZ = (-273.0, -5.0)          # the old column site; the ring road's outer radius 96 m
DITCH = [(-276.0, 255.0), (-262.0, 150.0), (-244.0, -5.0), (-235.0, -87.0)]
DITCH_W, DITCH_WATER, DITCH_DEPTH = 7.0, -1.6, 3.4
HUMBOLDTHAFEN = [(-298.0, -239.0, 544.0, 701.0), (-344.0, -177.0, 701.0, 878.0)]   # 1928 aerial


def h01(key, salt=0):
    n = zlib.crc32(f'{key}|{salt}'.encode()) ^ SEED
    n = (n*2654435761) & 0xFFFFFFFF
    n ^= n >> 15
    n = (n*2246822519) & 0xFFFFFFFF
    n ^= n >> 13
    return ((n >> 8) & 0xFFFF)/65536.0


def r3(v):
    return round(float(v), 3)


def dumps(obj):
    return json.dumps(obj, indent=1, ensure_ascii=False, sort_keys=True)+'\n'


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def rot(x, n, deg):
    """Rotate a local (east, north) offset counter-clockwise by deg (a +yaw turns +X toward north)."""
    a = math.radians(deg)
    return x*math.cos(a)-n*math.sin(a), x*math.sin(a)+n*math.cos(a)


# ── grids ────────────────────────────────────────────────────────────────────────────────────────
AX = np.linspace(-HALF, HALF, RES)
X, ZG = np.meshgrid(AX, AX)          # rows: engine z (= -n), columns: x
NG = -ZG
TAX = np.linspace(-HALF, HALF, TEX)


def tex_px(x, n):
    return ((x+HALF)/(2*HALF)*(TEX-1), (HALF-n)/(2*HALF)*(TEX-1))


def grid_px(x, n):
    return ((x+HALF)/SPACING, (HALF-n)/SPACING)


def raster(shapes, size, to_px, width_m=None):
    """Rasterise polygons [(pts, fill)] or polylines into an L-mode mask."""
    im = Image.new('L', (size, size), 0)
    d = ImageDraw.Draw(im)
    for pts, fill in shapes:
        xy = [to_px(x, n) for x, n in pts]
        if width_m is None:
            d.polygon(xy, fill=fill)
        else:
            w = max(int(round(width_m*size/(2*HALF))), 1)
            d.line(xy, fill=fill, width=w, joint='curve')
            r = w/2
            for p in (xy[0], xy[-1]):
                d.ellipse([p[0]-r, p[1]-r, p[0]+r, p[1]+r], fill=fill)
    return np.asarray(im)


def sample(H, x, n):
    """Bilinear height at (x, n)."""
    fx, fz = (x+HALF)/SPACING, (-n+HALF)/SPACING
    i, j = int(np.clip(math.floor(fx), 0, RES-2)), int(np.clip(math.floor(fz), 0, RES-2))
    tx, tz = fx-i, fz-j
    return float(H[j, i]*(1-tx)*(1-tz)+H[j, i+1]*tx*(1-tz)+H[j+1, i]*(1-tx)*tz+H[j+1, i+1]*tx*tz)


def polyline_distance(pts, x0, x1, n0, n1):
    """Distance field to a polyline over the grid window [x0,x1]x[n0,n1] (local)."""
    i0, i1 = int(max((x0+HALF)/SPACING, 0)), int(min((x1+HALF)/SPACING+2, RES))
    j0, j1 = int(max((-n1+HALF)/SPACING, 0)), int(min((-n0+HALF)/SPACING+2, RES))
    Xw, Nw = X[j0:j1, i0:i1], NG[j0:j1, i0:i1]
    D = np.full(Xw.shape, np.inf)
    for (ax, an), (bx, bn) in zip(pts, pts[1:]):
        dx, dn = bx-ax, bn-an
        L2 = dx*dx+dn*dn or 1e-9
        t = np.clip(((Xw-ax)*dx+(Nw-an)*dn)/L2, 0, 1)
        D = np.minimum(D, np.hypot(Xw-(ax+t*dx), Nw-(an+t*dn)))
    return D, (slice(j0, j1), slice(i0, i1))


# ── OSM ──────────────────────────────────────────────────────────────────────────────────────────
def load_osm():
    if not OSM_FILE.exists():
        sys.exit(f'{OSM_FILE} missing: run tools/berlin1945/fetch_osm.py')
    return json.loads(OSM_FILE.read_text())


def spree_centreline(osm):
    """Chain the Spree's waterway ways west to east."""
    ways = [e['pts'] for e in osm['elements'] if e['type'] == 'way' and e['tags'].get('waterway') == 'river'
            and e['tags'].get('name') == 'Spree']
    chain = list(min(ways, key=lambda w: min(p[0] for p in w)))
    if chain[0][0] > chain[-1][0]:
        chain.reverse()
    rest = [w for w in ways if w is not None]
    rest.remove(min(ways, key=lambda w: min(p[0] for p in w)))
    while rest:
        end = chain[-1]
        best = min(rest, key=lambda w: min(math.dist(end, w[0]), math.dist(end, w[-1])))
        if math.dist(end, best[-1]) < math.dist(end, best[0]):
            best = list(reversed(best))
        if min(math.dist(end, best[0]), 0) > 30:
            break
        chain += list(best[1:])
        rest.remove(best if best in rest else list(reversed(best)))
    # resample every 8 m and clip to the map
    out = [chain[0]]
    for a, b in zip(chain, chain[1:]):
        L = math.dist(a, b)
        for k in range(1, max(int(L/8.0), 1)+1):
            t = k/max(int(L/8.0), 1)
            out.append((a[0]+(b[0]-a[0])*t, a[1]+(b[1]-a[1])*t))
    inside = [p for p in out if abs(p[0]) < HALF+60 and abs(p[1]) < HALF+60]
    return inside


def water_polygons(osm):
    polys = []
    for e in osm['elements']:
        t = e['tags']
        if t.get('water') in ('river', 'harbour') or (e['type'] == 'relation' and t.get('water') == 'river'):
            if e['type'] == 'way':
                polys.append((e['pts'], 255))
            else:
                for r in e['rings']:
                    polys.append((r['pts'], 255 if r['role'] == 'outer' else 0))
    # outer rings first, then holes
    return [p for p in polys if p[1] == 255]+[p for p in polys if p[1] == 0]


STREET_WIDTH = {
    'Unter den Linden': 58.0, 'Straße des 17. Juni': 42.0, 'Ebertstraße': 28.0, 'Wilhelmstraße': 22.0,
    'Friedrichstraße': 22.0, 'Dorotheenstraße': 17.0, 'Behrenstraße': 18.0, 'Reichstagufer': 16.0,
    'Schiffbauerdamm': 18.0, 'Luisenstraße': 20.0, 'Invalidenstraße': 26.0, 'Alt-Moabit': 26.0,
    'Scheidemannstraße': 20.0, 'Paul-Löbe-Allee': 16.0, 'Otto-von-Bismarck-Allee': 16.0,
    'Willy-Brandt-Straße': 20.0, 'John-Foster-Dulles-Allee': 14.0, 'Tiergartenstraße': 20.0,
    'Lennéstraße': 14.0, 'Pariser Platz': 30.0,
}
# streets laid out on the new plan after 1945 (the government quarter, the Ministergaerten, the
# Hauptbahnhof): not drawn
MODERN_STREETS = {'Yitzhak-Rabin-Straße', 'Hannah-Arendt-Straße', 'Cora-Berliner-Straße', 'Gertrud-Kolmar-Straße',
                  'In den Ministergärten', 'Ella-Trebe-Straße', 'Clara-Jaschke-Straße', 'Europaplatz',
                  'Washingtonplatz', 'Rahel-Hirsch-Straße', 'Friedrich-List-Ufer', 'Margarete-Steffin-Straße',
                  'Konrad-Adenauer-Straße', 'Heinrich-von-Gagern-Straße', 'Kapelle-Ufer', 'Spreeweg',
                  'Joachim-Karnatz-Allee', 'Minna-Cauer-Straße', 'Hugo-Preuß-Brücke', 'Gustav-Heinemann-Brücke',
                  'Kronprinzenbrücke', 'Moltkebrücke', 'Marschallbrücke', 'Ebertbrücke', 'Weidendammer Brücke'}
WIDTH_BY_CLASS = {'primary': 24.0, 'secondary': 20.0, 'tertiary': 16.0, 'residential': 12.0, 'unclassified': 12.0,
                  'living_street': 10.0, 'pedestrian': 10.0, 'service': 0.0, 'footway': 0.0, 'cycleway': 0.0,
                  'path': 0.0, 'steps': 0.0, 'track': 0.0, 'primary_link': 12.0, 'secondary_link': 10.0}


def modern_zone(x, n):
    """Where today's buildings and streets stand on a post-1945 plan (or where 1945 had none)."""
    if x < 55 and -840 < n < 470:          # Tiergarten, Koenigsplatz, the Spreebogen (traced from 1928)
        return True
    if -720 < x < -60 and n > 380:          # Lehrter Bahnhof / Humboldthafen / Hauptbahnhof
        return True
    if 55 <= x < 290 and n < -440:          # the Ministergaerten
        return True
    return False


def streets_1945(osm):
    out = []
    for e in osm['elements']:
        t = e['tags']
        if e['type'] != 'way' or 'highway' not in t or t.get('bridge') == 'yes' or t.get('tunnel') == 'yes':
            continue
        name = t.get('name', '')
        if name in MODERN_STREETS:
            continue
        w = STREET_WIDTH.get(name, WIDTH_BY_CLASS.get(t['highway'], 0.0))
        if w <= 0:
            continue
        pts = e['pts']
        mid = pts[len(pts)//2]
        keep_named = name in STREET_WIDTH or name in ('Invalidenstraße', 'Alt-Moabit')
        if not keep_named and modern_zone(*mid):
            continue
        out.append(dict(name=name, pts=pts, width=w, surface='asphalt' if w >= 20 else 'cobble'))
    # the 1945 streets that today's map has lost or moved (1928 aerial)
    s = MOLTKE['south']
    out += [
        dict(name='Moltkestrasse', pts=[s, (-372.0, 266.0), (-366.0, 140.0)], width=20.0, surface='cobble'),
        dict(name='Koenigsplatz north edge', pts=[(-560.0, 139.0), (-300.0, 139.0)], width=16.0, surface='cobble'),
        dict(name='Kroll forecourt road', pts=[(-478.0, -110.0), (-478.0, 139.0)], width=14.0, surface='cobble'),
        dict(name='Sommerstrasse', pts=[(66.0, -95.0), (64.0, 55.0)], width=22.0, surface='asphalt'),
        dict(name='Reichstag forecourt road', pts=[(-74.0, -92.0), (-74.0, 100.0)], width=14.0, surface='cobble'),
        dict(name='Siegesallee (path)', pts=[(-273.0, -101.0), (-273.0, -380.0)], width=10.0, surface='sand'),
    ]
    return out


# ── the terrain ──────────────────────────────────────────────────────────────────────────────────
def build_terrain(osm, rng):
    H = np.zeros((RES, RES), dtype=np.float64)
    tier = tiergarten_mask_grid()
    # a gently uneven Tiergarten (sub-metre: the park was laid out on the flat valley floor)
    noise = np.zeros_like(H)
    for scale, amp in ((180.0, 0.35), (60.0, 0.18), (22.0, 0.07)):
        k = int(2*HALF/scale)+2
        g = rng.standard_normal((k, k))
        im = Image.fromarray(g.astype(np.float32), 'F').resize((RES, RES), Image.BICUBIC)
        noise += np.asarray(im)*amp
    H += noise*tier
    global H_BASE
    H_BASE = H.copy()
    # the Spree and the Humboldthafen: vertical quay walls, a flat bed
    water = raster(water_polygons(osm), RES, grid_px) > 0
    for x0, x1, n0, n1 in HUMBOLDTHAFEN:
        water |= (X > x0) & (X < x1) & (NG > n0) & (NG < n1)
    H[water] = BED_Y
    return H, water, tier


def tiergarten_mask_grid(size=RES, axis=AX):
    Xs, Zs = np.meshgrid(axis, axis)
    Ns = -Zs
    # west of Hermann-Goering-Strasse, south of the Kroll and Koenigsplatz, north of Tiergartenstrasse
    m = (Xs < 60.0-0.11*(Ns+258)) & (Ns < -100.0) & (Ns > -860.0)
    # the north edge west of the Kroll: In den Zelten to the Spree
    m |= (Xs < -560.0) & (Ns < 30.0) & (Ns > -860.0)
    # the ministry gardens behind Wilhelmstrasse (the Ministergaerten)
    m |= (Xs > 60.0-0.11*(Ns+258)) & (Xs < 280.0) & (Ns < -450.0) & (Ns > -860.0)
    return m.astype(np.float64)


def carve_line(H, pts, width, depth, floor=None, slope_run=0.8):
    """A cut along a polyline: flat bottom `width` wide `depth` deep, sides over slope_run."""
    xs = [p[0] for p in pts]; ns = [p[1] for p in pts]
    D, sl = polyline_distance(pts, min(xs)-width-5, max(xs)+width+5, min(ns)-width-5, max(ns)+width+5)
    prof = -depth*np.clip(1.0-(D-width/2)/slope_run, 0, 1)
    H[sl] = np.minimum(H[sl], prof if floor is None else np.maximum(prof, floor))


def crater(H, x, n, r, depth):
    D, sl = polyline_distance([(x, n), (x+0.01, n)], x-r*1.8, x+r*1.8, n-r*1.8, n+r*1.8)
    q = D/r
    H[sl] += -depth*np.clip(1-q*q, 0, 1)+0.22*depth*np.exp(-((q-1.15)/0.25)**2)


def mound(H, x, n, rx, rn, h, yaw=0.0):
    i0, i1 = int(max((x-rx-rn+HALF)/SPACING, 0)), int(min((x+rx+rn+HALF)/SPACING+2, RES))
    j0, j1 = int(max((-n-rx-rn+HALF)/SPACING, 0)), int(min((-n+rx+rn+HALF)/SPACING+2, RES))
    a = math.radians(yaw)
    dx, dn = X[j0:j1, i0:i1]-x, NG[j0:j1, i0:i1]-n
    u = (dx*math.cos(a)+dn*math.sin(a))/rx
    v = (-dx*math.sin(a)+dn*math.cos(a))/rn
    q = np.sqrt(u*u+v*v)
    H[j0:j1, i0:i1] += h*np.clip(1-q, 0, 1)**1.5


# ── city blocks from the OSM plots ───────────────────────────────────────────────────────────────
def obb(pts):
    """Minimum-area oriented rectangle: (cx, cn, long, short, angle_deg of the long side)."""
    P = np.asarray(pts, dtype=np.float64)
    best = None
    for deg in np.arange(0.0, 90.0, 1.0):
        a = math.radians(deg)
        u = P@np.array([math.cos(a), math.sin(a)])
        v = P@np.array([-math.sin(a), math.cos(a)])
        area = (u.max()-u.min())*(v.max()-v.min())
        if best is None or area < best[0]:
            best = (area, deg, u.min(), u.max(), v.min(), v.max())
    _, deg, u0, u1, v0, v1 = best
    a = math.radians(deg)
    cu, cv = (u0+u1)/2, (v0+v1)/2
    cx, cn = cu*math.cos(a)-cv*math.sin(a), cu*math.sin(a)+cv*math.cos(a)
    lu, lv = u1-u0, v1-v0
    if lv > lu:
        return cx, cn, lv, lu, deg+90.0
    return cx, cn, lu, lv, deg


def block_lots(osm, landmark_rects, water, sm):
    """Shell buildings on the OSM plots: (cx, cn, frontage, depth, outward normal angle)."""
    lots = []
    for e in osm['elements']:
        t = e['tags']
        if 'building' not in t or t.get('building') in ('roof', 'construction', 'bridge', 'train_station'):
            continue
        if t.get('layer', '0').startswith('-') or t.get('location') == 'underground':
            continue
        pts = e['pts'] if e['type'] == 'way' else [p for r in e['rings'] if r['role'] == 'outer' for p in r['pts']]
        if len(pts) < 4:
            continue
        cx, cn, L, S, ang = obb(pts)
        if L*S < 90 or abs(cx) > HALF-30 or abs(cn) > HALF-30 or modern_zone(cx, cn):
            continue
        # snap to the street grid where the plot is near it
        for g in (GRID_DEG, GRID_DEG+90, GRID_DEG-90, 0.0, 90.0):
            if abs(((ang-g+90) % 180)-90) < 6:
                ang = g
                break
        a = math.radians(ang)
        ux, un = math.cos(a), math.sin(a)          # along the long side
        vx, vn = -un, ux                            # across
        if L <= 34 and S <= 26:
            # one house: its front on the side that faces a street (the long side when both do)
            sides = [(vx, vn, L, S, 0.1), (-vx, -vn, L, S, 0.1), (ux, un, S, L, 0.0), (-ux, -un, S, L, 0.0)]
            def score(sd):
                ox, on, wide, deep, bias = sd
                return sum(sm.near(cx+ox*(deep/2+k), cn+on*(deep/2+k), 3.0) for k in (2.0, 6.0, 10.0))+bias
            nx, nn, wide, deep, _ = max(sides, key=score)
            lots.append((cx, cn, wide, max(min(deep, 30.0), 10.0), math.degrees(math.atan2(nx, nn)), e['id']))
        else:
            # a block: houses along each side, 16 m deep, the frontage outward
            depth = min(16.0, S/2-1.0) if S < 34 else 16.0
            for side, (ox, on, len_, ax, an) in enumerate((
                    (vx*S/2, vn*S/2, L, ux, un), (-vx*S/2, -vn*S/2, L, ux, un),
                    (ux*L/2, un*L/2, S, vx, vn), (-ux*L/2, -un*L/2, S, vx, vn))):
                inset = depth if side >= 2 else 0.0
                usable = len_-2*inset
                if usable < 10.0:
                    continue
                k = 0
                s = -usable/2
                while s < usable/2-6.0:
                    w = 18.0+10.0*h01(f'{e["id"]}.{side}.{k}')
                    w = min(w, usable/2-s)
                    if w < 9.0:
                        break
                    mx = cx+ox+ax*(s+w/2)
                    mn = cn+on+an*(s+w/2)
                    nrm = math.hypot(ox, on) or 1.0
                    ix, in_ = -ox/nrm, -on/nrm
                    lots.append((mx+ix*depth/2, mn+in_*depth/2, w, depth, math.degrees(math.atan2(-ix, -in_)), e['id']))
                    s += w
                    k += 1
    # drop lots that overlap a landmark, the water, a street or a lot placed before them
    kept, rects = [], []
    for lot in lots:
        cx, cn, w, d, out_deg, oid = lot
        if any(x0-4 < cx < x1+4 and n0-4 < cn < n1+4 for x0, x1, n0, n1 in landmark_rects):
            continue
        gx, gz = grid_px(cx, cn)
        if water[int(np.clip(gz, 0, RES-1)), int(np.clip(gx, 0, RES-1))]:
            continue
        if sm.near(cx, cn, 3.0):
            continue
        poly = lot_polygon(cx, cn, w-1.0, d-1.0, out_deg)
        if any(polygons_overlap(poly, q) for q in rects):
            continue
        kept.append(lot)
        rects.append(poly)
    return kept


def lot_polygon(cx, cn, w, d, out_deg):
    a = math.radians(out_deg)
    fx, fn = math.sin(a), math.cos(a)          # outward (front) direction
    sx, sn = fn, -fx                            # along the frontage
    return [(cx+sx*u*w/2+fx*v*d/2, cn+sn*u*w/2+fn*v*d/2) for u, v in ((-1, -1), (1, -1), (1, 1), (-1, 1))]


def polygons_overlap(p, q):
    """Separating-axis test for two convex quadrilaterals."""
    for poly in (p, q):
        for i in range(4):
            (ax, an), (bx, bn) = poly[i], poly[(i+1) % 4]
            nx, nn = -(bn-an), bx-ax
            pa = [nx*x+nn*n for x, n in p]
            qa = [nx*x+nn*n for x, n in q]
            if max(pa) < min(qa) or max(qa) < min(pa):
                return False
    return True


class StreetMask:
    """Street rasters on a 1 m grid: `near(x, n, m)` is true within m metres of a carriageway."""
    MARGINS = (0.0, 3.0, 8.0)

    def __init__(self, streets):
        self.size = int(2*HALF)
        self.masks = {}
        for m in self.MARGINS:
            im = Image.new('L', (self.size, self.size), 0)
            d = ImageDraw.Draw(im)
            for s in streets:
                xy = [((x+HALF), (HALF-n)) for x, n in s['pts']]
                w = int(round(s['width']+2*m))
                d.line(xy, fill=255, width=max(w, 1), joint='curve')
                for q in (xy[0], xy[-1]):
                    d.ellipse([q[0]-w/2, q[1]-w/2, q[0]+w/2, q[1]+w/2], fill=255)
            self.masks[m] = np.asarray(im) > 0

    def near(self, x, n, m=0.0):
        i, j = int(x+HALF), int(HALF-n)
        if not (0 <= i < self.size and 0 <= j < self.size):
            return False
        return bool(self.masks[m][j, i])


# ── placements ───────────────────────────────────────────────────────────────────────────────────
class Placer:
    def __init__(self, H):
        self.H = H
        self.assets, self.placements, self.count = [], [], {}

    def asset(self, path):
        if path not in self.assets:
            self.assets.append(path)
        return self.assets.index(path)

    def put(self, path, x, n, yaw=0.0, scale=1.0, sink=0.0, tag=None, y=None):
        if not (ASSET_ROOT/path).exists() and not path.startswith('assets/tanks/wrecks/'):
            raise FileNotFoundError(path)
        yy = sample(self.H, x, n)-sink if y is None else y
        self.placements.append([self.asset(path), r3(x), r3(yy), r3(-n), round(yaw % 360.0, 2),
                                [r3(s) for s in scale] if isinstance(scale, (list, tuple)) else r3(scale)])
        self.count[tag or Path(path).stem] = self.count.get(tag or Path(path).stem, 0)+1


def city_style(cx, cn):
    if math.hypot(cx-175, cn+250) < 160 or (270 < cx < 360 and cn > -700):
        return 'palais3'
    if cx > 600 or (60 < cx < 700 and cn > 40):
        return 'office5' if h01(f'st{cx:.0f}{cn:.0f}') < 0.4 else 'mietshaus5'
    return 'mietshaus5' if h01(f'st{cx:.0f}{cn:.0f}') < 0.6 else 'office5'


def city_condition(key):
    r = h01(key, 3)
    return 'gutted' if r < 0.52 else 'collapsed' if r < 0.72 else 'roofed_damaged'


# ── the map ──────────────────────────────────────────────────────────────────────────────────────
def build(plan=None):
    rng = np.random.default_rng(SEED)
    osm = load_osm()
    streets = streets_1945(osm)
    H, water, tier = build_terrain(osm, rng)
    spree = spree_centreline(osm)
    # Koenigsplatz: the flooded cut, the defenders' trenches, gun pits, and the craters of 30 April
    carve_line(H, DITCH, DITCH_W, DITCH_DEPTH, slope_run=1.2)
    trenches = [
        [(-202.0, 83.0), (-198.0, 40.0), (-204.0, 0.0), (-199.0, -48.0)],                      # Ivengo map
        [(-154.0, 38.0), (-150.0, 0.0), (-155.0, -41.0)],
        [(-302.0, 45.0), (-330.0, 30.0), (-343.0, -5.0), (-332.0, -40.0), (-305.0, -55.0)],   # round the old circle
        [(-78.0, -60.0), (-82.0, -20.0), (-78.0, 20.0), (-82.0, 60.0)],                        # the Reichstag's forecourt
        [(-40.0, 82.0), (0.0, 85.0), (40.0, 82.0)], [(-40.0, -82.0), (0.0, -85.0), (40.0, -82.0)],
        [(-470.0, -60.0), (-470.0, 60.0)],                                                      # in front of the Kroll
        [(-390.0, 130.0), (-440.0, 128.0)],                                                     # the ministry's south front
    ]
    for t in trenches:
        zig = []
        for (ax, an), (bx, bn) in zip(t, t[1:]):
            L = math.hypot(bx-ax, bn-an)
            k = max(int(L/6.0), 1)
            for i in range(k):
                f = i/k
                off = 1.6*(1 if i % 2 else -1)
                nx_, nn_ = -(bn-an)/L, (bx-ax)/L
                zig.append((ax+(bx-ax)*f+nx_*off, an+(bn-an)*f+nn_*off))
        zig.append(t[-1])
        carve_line(H, zig, 1.2, 1.6, slope_run=1.0)
    gun_pits = [(-180.0, 60.0), (-185.0, -35.0), (-130.0, 25.0), (-320.0, 70.0), (-455.0, 30.0), (-455.0, -40.0),
                (-110.0, -120.0), (40.0, -140.0)]
    for k, (x, n) in enumerate(gun_pits):
        crater(H, x, n, 3.2, 1.1)
        mound(H, x, n, 5.0, 5.0, 0.6)
    craters = []
    for k in range(150):
        r = h01(f'cr{k}')
        if r < 0.55:      # the square and the forecourt took the heaviest fire (30 April)
            x, n = -330+300*h01(f'crx{k}'), -100+230*h01(f'crn{k}')
        elif r < 0.8:     # the Tiergarten's north edge and the Chaussee
            x, n = -900+950*h01(f'crx{k}'), -450+330*h01(f'crn{k}')
        else:             # the Alsenviertel's cleared ground toward the bridge
            x, n = -460+420*h01(f'crx{k}'), 120+300*h01(f'crn{k}')
        if math.hypot(x, n) < 80 or water[int((HALF-n)/SPACING), int((x+HALF)/SPACING)]:
            continue
        if min(math.dist((x, n), (lx, ln)) for _, lx, ln, _ in LANDMARKS) < 60:
            continue
        rr = 1.8+3.2*h01(f'crr{k}')
        craters.append((x, n, rr))
        crater(H, x, n, rr, 0.5+0.35*rr)
    # rubble against the Reichstag's walls and the shells' feet (the ground is the support)
    for k in range(40):
        side = k % 4
        t = h01(f'rr{k}')*2-1
        x, n = [(-46.0, t*60), (46.0, t*60), (t*40, 67.0), (t*40, -67.0)][side]
        mound(H, x+(-3 if side == 0 else 3 if side == 1 else 0), n+(3 if side == 2 else -3 if side == 3 else 0),
              2.5+2*h01(f'rrs{k}'), 1.6+h01(f'rrt{k}'), 0.6+0.6*h01(f'rrh{k}'), yaw=90.0*h01(f'rry{k}'))
    H[water] = BED_Y               # nothing raised in the water
    # the landmark footprints (for lot exclusion and the ground's paving)
    lm_rects = [(-60, 60, -75, 75), (GATE_X-25, GATE_X+28, GATE_N-34, GATE_N+34), (-560, -480, -58, 58),
                (-472, -372, 138, 300), (-354, -324, 266, 291), (90, 156, -9, 30), (248, 303, -315, -265),
                (-510, -432, 540, 700), (-560, -440, 300, 430)]
    sm = StreetMask(streets)
    lots = block_lots(osm, lm_rects, water, sm)
    P = Placer(H)
    for name, x, n, yaw in LANDMARKS:
        P.put(MODELS+name+'.glb', x, n, yaw, 1.0, tag='landmark', y=0.0)
    # the Moltke Bridge: along its OSM axis, the deck at street level; the deck's walkable top
    (sx, sn), (nx_, nn_) = MOLTKE['south'], MOLTKE['north']
    bx, bn = (sx+nx_)/2, (sn+nn_)/2
    byaw = 90.0-math.degrees(math.atan2(nx_-sx, nn_-sn))
    P.put(MODELS+'moltke_bridge_1945.glb', bx, bn, byaw, 1.0, tag='landmark', y=0.0)
    P.put(MODELS+'floor_ceiling_moltke_bridge_1945.glb', bx, bn, byaw, 1.0, tag='landmark', y=0.0)
    # standing water in the cut across the square and in the Humboldthafen (river_crossings draws
    # the Spree itself)
    for (ax, an), (cx_, cn_) in zip(DITCH, DITCH[1:]):
        L = math.hypot(cx_-ax, cn_-an)
        yaw = 90.0-math.degrees(math.atan2(cx_-ax, cn_-an))
        P.put(MODELS+'ditch_water_1945.glb', (ax+cx_)/2, (an+cn_)/2, yaw, [L+DITCH_W*0.6, 1.0, DITCH_W-1.4],
              tag='water', y=DITCH_WATER)
    for x0, x1, n0, n1 in HUMBOLDTHAFEN:
        P.put(MODELS+'ditch_water_1945.glb', (x0+x1)/2, (n0+n1)/2, 0.0, [x1-x0+4.0, 1.0, n1-n0+4.0], tag='water',
              y=WATER_Y)
    # the city blocks: every plot standing in the few blocks round the Reichstag and Pariser Platz;
    # further out about a third standing and the rest razed to rubble heaps (Mitte, May 1945)
    standing, razed = [], []
    for lot in lots:
        cx, cn = lot[0], lot[1]
        core = 55 <= cx <= 520 and -470 <= cn <= 470
        (standing if core or h01(f'out{lot[5]}{cx:.0f}{cn:.0f}') < 0.35 else razed).append(lot)
    for cx, cn, w, d, out_deg, oid in razed:
        mound(H, cx, cn, w*0.55, d*0.6, 1.4+1.8*h01(f'rz{cx:.0f}{cn:.0f}'), yaw=90.0-out_deg)
    lots = standing
    for cx, cn, w, d, out_deg, oid in razed:
        for k in range(2):
            P.put(PZ+'Debris & Misc/'+('debris_bricks_mx_1.glb' if k else 'gravel_pile_hr_2.glb'),
                  cx+(h01(f'rzx{cx:.0f}{k}')-0.5)*w*0.6, cn+(h01(f'rzn{cn:.0f}{k}')-0.5)*d*0.6,
                  360*h01(f'rzy{cx:.0f}{k}'), 1.2+h01(f'rzs{cn:.0f}{k}'), tag='razed')
    for cx, cn, w, d, out_deg, oid in lots:
        style = city_style(cx, cn)
        cond = city_condition(f'{oid}.{cx:.0f}.{cn:.0f}')
        base = 14.0 if w < 19.0 else 24.0
        sxs = float(np.clip(w/base, 0.72, 1.35))
        szs = float(np.clip(d/16.0, 0.65, 1.6))
        # out_deg is the outward normal's bearing; the model's street front is engine +Z
        yaw = math.degrees(math.atan2(math.sin(math.radians(out_deg)), -math.cos(math.radians(out_deg))))
        P.put(f'{MODELS}berlin_{style}_{int(base)}_{cond}.glb', cx, cn, yaw, [sxs, 1.0, szs], tag=f'block_{cond}')
    # grammar houses: the burnt row on In den Zelten (Donath 1945, Bundesarchiv 183-M1015-321)
    grammar = []
    for k in range(7):
        x = -1040.0+62.0*k
        typ = 'apartment_block' if k % 3 == 0 else 'merchant_brick_house'
        w = 24.0 if typ == 'apartment_block' else [12.0, 16.0, 20.0][int(h01(f'iz{k}')*3)]
        grammar.append(dict(id=f'berlin.zelten.house_{k:02d}', typology=typ, seed=int(zlib.crc32(f'iz{k}'.encode())),
                            width_m=w, depth_m=12.0, storeys=3 if typ == 'apartment_block' else 2, cellar=True,
                            condition=['burned', 'partly_collapsed', 'shelled'][k % 3], x=r3(x), z=r3(-30.0),
                            yaw_deg=0.0))
    dress(P, H, sm, tier, water, lots)
    trees = plant(P, H, tier, sm, water, lots)
    # ── course, encounter and mission ──
    route = course_route()
    course, street, garrison = course_blocks(route, H)
    manifest = dict(schema='kessel.act', version=1, act='Berlin, Reichstag', act_id=1, seed=SEED & 0xFFFFFFFF,
                    terrain=dict(res=RES, spacing=SPACING, heights='heights.npy',
                                 texture=f'{TERRAIN_REL}/ground_material.png',
                                 material_encoding='gaea-rock-srgb-snow-linear-alpha32-223-v1',
                                 ground_class=f'{TERRAIN_REL}/ground_class.png', grass_season='spring',
                                 color=[0.9, 0.9, 0.9, 1.0]),
                    assets=P.assets, placements=P.placements, wires=[],
                    authoring=dict(mode='course', tool='tools/worldgen/berlin_reichstag.py', tool_version=VERSION,
                                   variant='berlin-reichstag-v1'))
    start, goal = course['start'], course['goal']
    manifest['markers'] = [dict(name='canyon_start', kind='Spawn', x=start['x'], z=start['z'],
                                y=r3(sample(H, start['x'], -start['z']))),
                           dict(name='canyon_goal', kind='Goal', x=goal['x'], z=goal['z'],
                                y=r3(sample(H, goal['x'], -goal['z'])))]
    # navigation landmarks (src/world.rs LandmarkKind)
    (sx_, sn_), (nx2, nn2) = MOLTKE['south'], MOLTKE['north']
    for name, kind, x, n in (('reichstag', 'Monument', 0.0, 0.0), ('brandenburg_gate', 'Monument', GATE_X, GATE_N),
                             ('moltke_bridge', 'Bridge', (sx_+nx2)/2, (sn_+nn2)/2),
                             ('lehrter_bahnhof', 'RailStation', -472.0, 622.0)):
        manifest['markers'].append(dict(name=name, kind=kind, x=r3(x), z=r3(-n), y=r3(sample(H, x, n))))
    manifest['overview'] = dict(pos=[-420.0, 300.0, 420.0], target=[-60.0, 10.0, 60.0])
    # the Spree (src/river_crossings.rs): its water ribbon follows the OSM centreline, wide enough
    # for the widest reach (the quays, not the ribbon, bound the water: the ribbon runs under the
    # banks where the river is narrower)
    manifest['river'] = dict(schema='kessel.river', version=1, half_width_m=34.0, bank_run_m=0.0, bed_depth_m=3.5,
                             bank_above_m=-WATER_Y, centreline=[[r3(x), r3(-n), WATER_Y] for x, n in spree],
                             note='centreline points are [x, z, water_y]; the Spree from OSM; its banks are the '
                                  'quay walls carved into the heightfield')
    edits = dict(
        schema_note='Berlin 1945 workshop (tools/worldgen/berlin_reichstag.py)',
        building_instances=grammar, canyon_course=course, street_encounter=street,
        environment_lighting=dict(enabled=True, fog_density=0.0026, overcast=0.62, storm=0.0,
                                  schema='kessel.environment-authoring', utc='1945-04-30T15:30:00Z', version=2),
        editor_home=dict(eye=[-300.0, 3.0, 0.0], target=[-46.0, 18.0, 0.0]),
        authored_lights=[], sound_emitters=[], ops=[],
        base_placements=len(P.placements), base_markers=len(manifest['markers']))
    edits.pop('schema_note')
    DEST.mkdir(parents=True, exist_ok=True)
    old = DEST/'manifest_edits.json'
    if old.exists():
        prev = json.loads(old.read_text())
        edits['ops'] = [op for op in prev.get('ops', []) if op.get('op') in ('add', 'marker_add')]
    # ground textures
    colour, grass = ground(osm, H, streets, tier, water, lots, rng)
    tdir = ASSET_ROOT/TERRAIN_REL
    tdir.mkdir(parents=True, exist_ok=True)
    rgba = np.dstack([np.uint8(np.rint(np.clip(colour, 0, 1)*255)), np.full(colour.shape[:2], 32, np.uint8)])
    Image.fromarray(rgba, 'RGBA').save(tdir/'ground_material.png')
    g = np.uint8(np.rint(np.clip(grass, 0, 1)*255))
    Image.fromarray(np.dstack([g, np.zeros_like(g), np.zeros_like(g)]), 'RGB').save(tdir/'ground_class.png')
    (DEST/'manifest.v1.json').write_text(json.dumps(manifest, separators=(',', ':'), sort_keys=True)+'\n')
    (DEST/'manifest_edits.json').write_text(dumps(edits))
    np.save(DEST/'heights.npy', H.astype(np.float32))
    (DEST/'mission.v1.json').write_text(dumps(mission_sheet(route)))
    receipt = dict(schema='panzer.berlin-reichstag', version=VERSION, seed=SEED, extent_m=2*HALF, res=RES,
                   placements=len(P.placements), assets=len(P.assets), counts=P.count, city_lots=len(lots), razed_plots=len(razed),
                   grammar_houses=len(grammar), trees=trees, craters=len(craters), osm_snapshot=osm['osm_base'],
                   start=course['start'], goal=course['goal'],
                   outputs={f: sha256(DEST/f) for f in OUTPUT_FILES})
    (DEST/'berlin-reichstag.receipt.json').write_text(dumps(receipt))
    if plan:
        write_plan(plan, colour, P, route)
    print(json.dumps({k: receipt[k] for k in ('placements', 'assets', 'counts', 'city_lots', 'trees', 'start', 'goal')},
                     indent=1))
    return receipt


def dress(P, H, sm, tier, water, lots):
    """The fighting's traces: barricades, sandbags, guns, wrecks, rubble, the dead (few)."""
    W = 'assets/warprops/'
    PZD = PZ+'Debris & Misc/'
    # sandbagged positions at the Reichstag's west front and corners, the Kroll, the ministry
    bags = [(-70.0, -20.0, 90), (-70.0, 20.0, 90), (-66.0, -60.0, 90), (-66.0, 60.0, 90), (-52.0, 76.0, 0),
            (52.0, 76.0, 0), (-52.0, -76.0, 0), (60.0, 0.0, 90), (-472.0, -30.0, 90), (-472.0, 30.0, 90),
            (-400.0, 132.0, 0), (-430.0, 132.0, 0), (-339.0, 262.0, 0)]
    for x, n, yaw in bags:
        P.put(W+'sandbag_wall.glb', x, n, yaw, 1.0, tag='defence')
    for (x, n) in [(-180.0, 60.0), (-185.0, -35.0), (-130.0, 25.0), (-320.0, 70.0)]:
        for k in range(3):
            a = 120*k+40*h01(f'gp{x}{k}')
            P.put(W+'sandbag_wall.glb', x+4.2*math.cos(math.radians(a)), n+4.2*math.sin(math.radians(a)), a+90, 0.8,
                  tag='defence')
    # the guns: the wrecked heavy flak gun before the Kroll (Donath 1945); a wrecked field gun
    P.put(MODELS+'flak88_wreck.glb', -455.0, 30.0, 35.0, 1.0, tag='guns')
    P.put(MODELS+'flak88_wreck.glb', -455.0, -40.0, 160.0, 1.0, tag='guns')
    P.put(W+'field_gun_wrecked.glb', -180.0, 60.0, 300.0, 1.0, tag='guns')
    P.put(W+'field_gun_wrecked.glb', -110.0, -120.0, 250.0, 1.0, tag='guns')
    # knocked-out armour on the square and the forecourt (a recurring sight of the assault, not a
    # record of particular vehicles)
    P.put('assets/tanks/wrecks/t34_85.glb', -238.0, 70.0, 200.0, 1.0, tag='wrecks')
    P.put('assets/tanks/wrecks/is2.glb', -150.0, -70.0, 75.0, 1.0, tag='wrecks')
    P.put(W+'truck_burned.glb', 40.0, -150.0, 10.0, 1.0, tag='wrecks')
    P.put(W+'truck_he_hit.glb', -470.0, 110.0, 95.0, 1.0, tag='wrecks')
    P.put(W+'truck_burned.glb', 160.0, -230.0, 100.0, 1.0, tag='wrecks')
    # hedgehogs and barricades on the approaches (Hermann-Goering-Strasse, the Chaussee at the gate)
    for k in range(10):
        P.put(W+'hedgehog.glb', 30.0+9.0*(k % 5), -160.0-9.0*(k//5), 37*k, 1.0, tag='defence')
    for k in range(8):
        P.put(W+'hedgehog.glb', -40.0-11.0*k, -268.0+6.0*(k % 2), 45*k, 1.0, tag='defence')
    for k, (x, n, yaw) in enumerate([(-5.0, -250.0, 0), (-5.0, -275.0, 0), (66.0, -60.0, 90), (230.0, -248.0, 6)]):
        P.put(PZ+'Modular Props/barricade_a_2.glb', x, n, yaw, 1.4, tag='defence')
    # rubble and debris at the feet of the shells
    debris = [PZD+'debris_bricks_mx_1.glb', PZD+'debris_bricks_mx_2.glb', PZD+'bricks_stacked_mx_2.glb',
              PZD+'gravel_pile_hr_1.glb', PZD+'gravel_pile_hr_2.glb']
    for li, (cx, cn, w, d, out_deg, oid) in enumerate(lots):
        if h01(f'deb{li}') < 0.6:
            a = math.radians(out_deg)
            for k in range(1+int(h01(f'debn{li}')*3)):
                t = (h01(f'debt{li}{k}')-0.5)*w*0.8
                x = cx+math.sin(a)*(d/2+1.5+2.0*h01(f'debo{li}{k}'))+math.cos(a)*t
                n = cn+math.cos(a)*(d/2+1.5+2.0*h01(f'debo{li}{k}'))-math.sin(a)*t
                if sm.near(x, n, 8.0):
                    P.put(debris[int(h01(f'debk{li}{k}')*len(debris))], x, n, 360*h01(f'debr{li}{k}'),
                          0.9+0.8*h01(f'debs{li}{k}'), tag='rubble')
    for k in range(60):
        side = k % 4
        t = h01(f'rd{k}')*2-1
        x, n = [(-50.0, t*60), (50.0, t*60), (t*40, 70.0), (t*40, -70.0)][side]
        P.put(debris[k % len(debris)], x, n, 360*h01(f'rdy{k}'), 1.0+0.8*h01(f'rds{k}'), tag='rubble')
    # the dead of the 30 April assault across the square, by the cut and the trenches (few)
    dead = ['dead/dead_sol_sprawl_back.glb', 'dead/dead_sol_face_down.glb', 'dead/dead_sol_on_side.glb',
            'dead/dead_sol_curled.glb']
    for k in range(14):
        a = h01(f'dd{k}')
        if a < 0.6:
            f = h01(f'ddf{k}')
            i = min(int(f*(len(DITCH)-1)), len(DITCH)-2)
            (ax, an), (bx, bn) = DITCH[i], DITCH[i+1]
            t = f*(len(DITCH)-1)-i
            x, n = ax+(bx-ax)*t+6+10*h01(f'ddx{k}'), an+(bn-an)*t
        else:
            x, n = -200+120*h01(f'ddx{k}'), -60+140*h01(f'ddn{k}')
        P.put(W+dead[k % 4], x, n, 360*h01(f'ddy{k}'), 1.0, tag='dead')
        if h01(f'ddh{k}') < 0.4:
            P.put(W+'helmet_ground.glb', x+1.1, n-0.7, 360*h01(f'ddhy{k}'), 1.0, tag='dead')
    for k in range(10):
        P.put(W+'ammo_crate_burst.glb' if k % 2 else W+'fuel_drum_burst.glb', -320+260*h01(f'kit{k}'),
              -80+180*h01(f'kitn{k}'), 360*h01(f'kity{k}'), 1.0, tag='kit')


def plant(P, H, tier, sm, water, lots):
    """The Tiergarten's trees, many shattered; the square's edge trees; a few on Unter den Linden."""
    live = [NAT+'tree_5.glb', NAT+'tree_6.glb', NAT+'tree_7.glb', NAT+'tree_8.glb']
    broken = [MODELS+'tree_shattered_a.glb', MODELS+'tree_shattered_b.glb', MODELS+'tree_shattered_c.glb']
    lot_pts = np.array([(l[0], l[1]) for l in lots]) if lots else np.zeros((0, 2))
    count = 0
    step = 11.0
    for j, n in enumerate(np.arange(-HALF+20, HALF-20, step)):
        for i, x in enumerate(np.arange(-HALF+20, HALF-20, step)):
            key = f't{i}.{j}'
            x2 = x+(h01(key, 1)-0.5)*step*0.9
            n2 = n+(h01(key, 2)-0.5)*step*0.9
            gx, gz = int((x2+HALF)/SPACING), int((HALF-n2)/SPACING)
            if not (0 <= gx < RES and 0 <= gz < RES) or tier[gz, gx] < 0.5 or water[gz, gx]:
                continue
            if h01(key, 3) > 0.62:
                continue
            if sm.near(x2, n2, 3.0) or math.hypot(x2-GATE_X+60, n2-GATE_N) < 75:
                continue
            # the Chaussee's verges cleared for the last days' airstrip (de.wikipedia Strasse des 17. Juni)
            if -500.0 < x2 < 60.0 and abs(n2-(-258.0+0.107*(x2-100.0))) < 38.0:
                continue
            if len(lot_pts) and np.min(np.hypot(lot_pts[:, 0]-x2, lot_pts[:, 1]-n2)) < 16:
                continue
            # nearer the square, the Chaussee and the flak positions more of them are broken
            near = math.hypot(x2-KOENIGSPLATZ[0], n2-KOENIGSPLATZ[1]) < 380 or abs(n2-(-258-0.11*(GATE_X-x2))) < 70
            p_broken = 0.55 if near else 0.25
            if h01(key, 4) < p_broken:
                P.put(broken[int(h01(key, 5)*3)], x2, n2, 360*h01(key, 6), 0.9+0.4*h01(key, 7), sink=0.05, tag='trees_broken')
            else:
                P.put(live[int(h01(key, 5)*4)], x2, n2, 360*h01(key, 6), 1.6+0.9*h01(key, 7), sink=0.05, tag='trees')
            count += 1
    # the ring of trees round Koenigsplatz (1928) and along the Spree's south bank
    cx, cn = KOENIGSPLATZ
    for k in range(64):
        a = 2*math.pi*k/64
        x, n = cx+104*math.cos(a), cn+104*math.sin(a)
        if abs(x-(-244)) < 14 or n > 120:
            continue
        p = broken[k % 3] if h01(f'ring{k}') < 0.6 else live[k % 4]
        P.put(p, x, n, 360*h01(f'ringy{k}'), 1.0 if p in broken else 1.7, sink=0.05, tag='trees_square')
        count += 1
    return count


def course_route():
    """The tank's way for the course machinery: in from the Tiergarten on the Charlottenburger
    Chaussee, north up the old Siegesallee's line onto Koenigsplatz, across the square past the
    Reichstag's west front, round the cut's north end and on to the Moltke Bridge's south end."""
    knots = [(-870.0, -345.0), (-700.0, -328.0), (-420.0, -298.0), (-285.0, -284.0), (-273.0, -240.0),
             (-273.0, -110.0), (-230.0, -110.0), (-200.0, -60.0), (-110.0, -20.0), (-100.0, 60.0),
             (-180.0, 150.0), (-245.0, 250.0), (-280.0, 300.0), (-380.0, 300.0), (-440.0, 322.0)]
    # (round the flooded cut's south end and its north end by the Swiss legation: a tank cannot
    # cross the cut)
    pts = []
    for a, b in zip(knots, knots[1:]):
        L = math.dist(a, b)
        k = max(int(L/5.0), 1)
        for i in range(k):
            pts.append((a[0]+(b[0]-a[0])*i/k, a[1]+(b[1]-a[1])*i/k))
    pts.append(knots[-1])
    return pts


def flat_spot(H, x, n, r=3.0, tol=0.2, reach=30.0):
    """The nearest point to (x, n) whose ground within r metres varies by less than tol."""
    for ring in range(0, int(reach/2)+1):
        for k in range(max(1, ring*6)):
            a = 2*math.pi*k/max(1, ring*6)
            px, pn = x+2.0*ring*math.cos(a), n+2.0*ring*math.sin(a)
            hs = [sample(H, px+dx, pn+dn) for dx in (-r, 0, r) for dn in (-r, 0, r)]
            if max(hs)-min(hs) < tol:
                return px, pn
    return x, n


def course_blocks(route, H):
    rr = [[r3(x), r3(-n)] for x, n in route]
    seg = [math.dist(a, b) for a, b in zip(route, route[1:])]
    k0 = int(np.argmin([abs(x+560.0) for x, _ in route]))
    start = route[k0]
    nxt = route[k0+4]
    heading = math.degrees(math.atan2(nxt[0]-start[0], -(nxt[1]-start[1])))
    goal = (-110.0, -20.0)
    def site(kind, on, x, n):
        fx, fn = flat_spot(H, x, n)
        return dict(kind=kind, on=on, x=r3(fx), z=r3(-fn))
    # anti-tank guns: west of the flooded cut on the square, at the Kroll, before the ministry
    sites = [site('ground', 'koenigsplatz.west', -305.0, -60.0), site('ground', 'koenigsplatz.west', -312.0, 62.0),
             site('corner', 'kroll.front', -462.0, 66.0), site('rubble', 'ministry.south', -405.0, 126.0),
             site('ground', 'alsen.clearing', -300.0, 200.0), site('corner', 'swiss.legation', -320.0, 262.0)]
    posts = [(-64.0, -70.0, 270.0), (-64.0, 70.0, 270.0), (-64.0, 0.0, 270.0), (-462.0, -48.0, 90.0),
             (-462.0, 40.0, 90.0), (-380.0, 125.0, 180.0), (-430.0, 125.0, 180.0), (-330.0, 255.0, 180.0),
             (75.0, -40.0, 270.0), (75.0, 45.0, 270.0), (30.0, -110.0, 0.0), (-150.0, -110.0, 0.0),
             (-250.0, 125.0, 180.0), (-120.0, 120.0, 180.0), (150.0, -215.0, 270.0), (60.0, -300.0, 0.0)]
    garrison = []
    for x, n, hd in posts:
        fx, fn = flat_spot(H, x, n, r=2.0, tol=0.15)
        garrison.append(dict(x=r3(fx), z=r3(-fn), heading_deg=hd))
    course = dict(schema='kessel.canyon-course', version=1, tool='tools/worldgen/berlin_reichstag.py', tool_version=VERSION,
                  variant='berlin-reichstag-v1', route=rr, length_m=r3(sum(seg)),
                  start=dict(x=r3(start[0]), z=r3(-start[1]), heading_deg=r3(heading)),
                  goal=dict(x=goal[0], z=r3(-goal[1]), radius_m=60.0),
                  gun_zones=[dict(id='koenigsplatz', kill_zone='route.koenigsplatz', candidates=[0, 1, 2]),
                             dict(id='alsenviertel', kill_zone='route.alsenviertel', candidates=[3, 4, 5])],
                  garrison=garrison,
                  design=[dict(name='tiergarten_and_chaussee', from_fraction=0.0, to_fraction=0.42),
                          dict(name='koenigsplatz', from_fraction=0.42, to_fraction=0.70),
                          dict(name='alsenviertel_to_the_bridge', from_fraction=0.70, to_fraction=1.0)],
                  finish='mission', director=False, opening_platoons=False, camp=True,
                  headline='The square is held',
                  provenance=dict(
                      place='docs/BERLIN_REICHSTAG_1945.md: the Reichstag, Koenigsplatz, the Kroll, the Ministry of the '
                            'Interior, the Moltke Bridge, the Brandenburg Gate, 28 April - 2 May 1945.',
                      route='Not historical: a way through the place for the course machinery.',
                      warzone='Beevor, Berlin: The Downfall 1945 (2002): the bridge barricaded and its arch blown on '
                              '28 April; the ministry taken on 29 April; the assaults across the square on 30 April '
                              'under fire from the Reichstag, the Kroll and the Zoo flak tower; the flooded cut. '
                              'Defensive positions after the Ivengo battle map (Wikimedia Commons).'),
                  note='Reaching the goal disc does not finish the run: the mission does (src/canyon.rs).')
    rp = np.array(route)
    def line(i0, i1):
        return [[r3(x), r3(-n)] for x, n in rp[i0:i1:6]]
    n_ = len(route)
    street = dict(schema='kessel.street-encounter', version=1, tool='tools/worldgen/berlin_reichstag.py', tool_version=VERSION,
                  kill_zone=[dict(id='route.whole', role='route', weight=1.0, points=line(0, n_)),
                             dict(id='route.koenigsplatz', role='route', weight=1.0, points=line(int(n_*0.42), int(n_*0.72))),
                             dict(id='route.alsenviertel', role='route', weight=1.0, points=line(int(n_*0.68), n_))],
                  gun_sites=sites,
                  # the platoons' beats, east of the flooded cut (a tank cannot cross it): the square in
                  # front of the Reichstag, and the cleared Alsenviertel toward the ministry ([x, engine z])
                  beats=[dict(platoon='A', points=[[-215.0, 60.0], [-215.0, -60.0], [-110.0, -75.0], [-110.0, 75.0]]),
                         dict(platoon='B', points=[[-230.0, -140.0], [-90.0, -140.0], [-90.0, -225.0], [-230.0, -225.0]])],
                  ways=dict(district=[-HALF, HALF, -HALF, HALF],
                            rects=[[r3(start[0]-40), r3(start[0]+40), r3(-start[1]-40), r3(-start[1]+40)],
                                   [-238.0, -62.0, -110.0, 110.0], [-250.0, -40.0, -245.0, -118.0],
                                   [-400.0, -255.0, -110.0, 110.0], [-120.0, 60.0, 85.0, 300.0]],
                            corridors=[dict(a=[r3(a[0]), r3(-a[1])], b=[r3(b[0]), r3(-b[1])], width_m=30.0)
                                       for a, b in zip(route[::30], route[30::30])]),
                  provenance=dict(doctrine=['Beevor 2002: the defenders fought from the Reichstag, the Kroll and '
                                            'the ministry; guns covered the square from both sides.']),
                  note='The street encounter machinery of the CANYON course (src/street_plan.rs) on this map.')
    return course, street, None


def mission_sheet(route):
    rp = np.array(route)
    seg = np.hypot(*np.diff(rp, axis=0).T)
    along = np.concatenate([[0.0], np.cumsum(seg)])
    def frac(x, n):
        return round(float(along[int(np.argmin(np.hypot(rp[:, 0]-x, rp[:, 1]-n)))]/along[-1]), 4)
    def p(x, n):
        return {'x': r3(x), 'z': r3(-n)}
    def line(text):
        return [{'from': 'Adler', 'text': text}]
    return {
        'schema': 'kessel.mission', 'version': 1, 'id': 'berlin-reichstag', 'act_id': 1,
        'title': 'BEFEHL: KOENIGSPLATZ', 'callsign': 'Kiebitz', 'net': 'Adler',
        'note': ('Berlin 1945 map (tools/worldgen/berlin_reichstag.py). The place, its ruins and the defence are '
                 'attested (docs/BERLIN_REICHSTAG_1945.md); this order, the tank and its crew are not a record of a '
                 'unit or an engagement.'),
        'order': ['Panzer 231. Out of the Tiergarten onto Koenigsplatz.',
                  'Soviet infantry and armour hold the ministry and the Moltke Bridge.',
                  'Hold the square in front of the Reichstag.'],
        'beats': [
            {'id': 'chaussee', 'title': 'The Charlottenburger Chaussee', 'goal': 'reach', 'at': p(-273.0, -280.0),
             'radius_m': 70.0, 'complete': {'kind': 'enter', 'dwell_s': 1.0},
             'lines': {'open': line('Kiebitz, Adler. North up the old Siegesallee to the square. Out.'),
                       'done': line('Kiebitz on the Chaussee. Out.')},
             'map': {'mark': 'box', 'bearing': True}},
            {'id': 'reichstag', 'title': 'The Reichstag', 'goal': 'observe', 'at': p(0.0, 0.0), 'radius_m': 400.0,
             'complete': {'kind': 'observe', 'cone_deg': 40.0, 'dwell_s': 3.0},
             'lines': {'open': line('Kiebitz, Adler. The Reichstag ahead of you, the square in front of it. Out.'),
                       'done': line('Reichstag in sight. Out.')},
             'map': {'mark': 'circle', 'bearing': True}},
            {'id': 'square', 'title': 'Hold Koenigsplatz', 'goal': 'hold', 'at': p(-180.0, -10.0), 'radius_m': 80.0,
             'complete': {'kind': 'capture', 'seconds': 90.0, 'contest_radius_m': 220.0},
             'pacing': {'release': {'count': 3, 'fraction': frac(-380.0, 300.0)}},
             'cues': [{'kind': 'alert', 'at': p(-380.0, 300.0), 'radius_m': 500.0}],
             'lines': {'open': line('Kiebitz, Adler. Tanks from the ministry. Hold the square. Out.'),
                       'done': line('The square is held. Out.')},
             'map': {'mark': 'circle', 'bearing': True}},
            {'id': 'all_tanks', 'title': 'Destroy every tank', 'goal': 'destroy', 'at': p(-180.0, -10.0),
             'radius_m': 120.0, 'complete': {'kind': 'destroy_all'},
             'lines': {'open': line('Kiebitz, Adler. Armour still on the square. Out.'),
                       'done': line('Kiebitz, Adler. No more armour. Out.')},
             'map': {'mark': 'cross', 'bearing': False}},
        ]}


def ground(osm, H, streets, tier, water, lots, rng):
    """The ground's colour (sRGB 0..1) and grass amount on the TEX grid."""
    XT, ZT = np.meshgrid(TAX, TAX)
    NT = -ZT
    def up(field):
        im = Image.fromarray(field.astype(np.float32), 'F').resize((TEX, TEX), Image.BILINEAR)
        return np.asarray(im)
    fine = np.zeros((TEX, TEX), np.float32)
    for scale, amp in ((3.0, 0.5), (12.0, 0.3), (60.0, 0.2)):
        k = int(2*HALF/scale)+2
        g = rng.standard_normal((k, k)).astype(np.float32)
        fine += np.asarray(Image.fromarray(g, 'F').resize((TEX, TEX), Image.BICUBIC))*amp
    fine = np.clip(fine*0.5, -1, 1)
    # base: trodden earth, dust and brick grit of a bombed city
    colour = np.stack([0.33+0.04*fine, 0.30+0.035*fine, 0.25+0.03*fine], -1)
    grass = np.zeros((TEX, TEX), np.float32)
    tierT = up(tier)
    # the Tiergarten: spring grass under the trees, leaf litter, churned where it was fought over
    park = np.stack([0.20+0.04*fine, 0.25+0.05*fine, 0.12+0.03*fine], -1)
    colour = colour*(1-tierT[..., None])+park*tierT[..., None]
    grass += tierT*(0.75+0.25*fine)
    # Koenigsplatz: the lawns of the 1928 layout, worn and torn up
    cx, cn = KOENIGSPLATZ
    rr = np.hypot(XT-cx, NT-cn)
    lawn = (rr < 88) & (rr > 18)
    colour[lawn] = (colour[lawn]*0.3+np.stack([0.24+0.05*fine, 0.27+0.05*fine, 0.14+0.03*fine], -1)[lawn]*0.7)
    grass[lawn] = 0.55+0.25*fine[lawn]
    # streets: asphalt, setts, sand; tram lines on Hermann-Goering-Strasse and the Linden
    for surf, tint in (('sand', (0.42, 0.38, 0.30)), ('cobble', (0.27, 0.26, 0.25)), ('asphalt', (0.19, 0.19, 0.19))):
        group = [st for st in streets if st['surface'] == surf]
        if not group:
            continue
        core = np.zeros((TEX, TEX), bool)
        walk = np.zeros((TEX, TEX), bool)
        imc, imw = Image.new('L', (TEX, TEX), 0), Image.new('L', (TEX, TEX), 0)
        dc, dw = ImageDraw.Draw(imc), ImageDraw.Draw(imw)
        for st in group:
            xy = [tex_px(x, n) for x, n in st['pts']]
            w = st['width']*TEX/(2*HALF)
            dc.line(xy, fill=255, width=max(int(w), 1), joint='curve')
            if st['width'] >= 16:
                dw.line(xy, fill=255, width=int(w+7.0*TEX/(2*HALF)), joint='curve')
        core = np.asarray(imc) > 0
        walk = (np.asarray(imw) > 0) & ~core
        colour[walk] = np.array((0.36, 0.35, 0.33), np.float32)*(0.9+0.1*fine[walk])[:, None]
        grass[walk] = 0.0
        colour[core] = np.array(tint, np.float32)*(0.9+0.12*fine[core])[:, None]
        grass[core] = 0.0
    # tram tracks: Hermann-Goering-Strasse and Sommerstrasse past the gate and the Reichstag, and
    # Unter den Linden (B2, R4 and R7 in docs/BERLIN_REICHSTAG_1945.md show the rails)
    im = Image.new('L', (TEX, TEX), 0)
    d = ImageDraw.Draw(im)
    for st in streets:
        if st['name'] not in ('Ebertstraße', 'Sommerstrasse', 'Unter den Linden'):
            continue
        pts = st['pts']
        for off in (-2.9, -1.4, 1.4, 2.9):
            line = []
            for k, (x, n) in enumerate(pts):
                a_, b_ = pts[max(k-1, 0)], pts[min(k+1, len(pts)-1)]
                dx, dn = b_[0]-a_[0], b_[1]-a_[1]
                L = math.hypot(dx, dn) or 1.0
                line.append(tex_px(x-dn/L*off, n+dx/L*off))
            d.line(line, fill=255, width=1)
    rails = np.asarray(im) > 0
    colour[rails] = (0.12, 0.115, 0.11)
    # Koenigsplatz's ring road and radial paths (1928), the Platz vor dem Brandenburger Tor, Pariser Platz
    ring = (rr > 90) & (rr < 102)
    colour[ring] = (0.29, 0.28, 0.26)
    grass[ring] = 0
    for k in range(6):
        a = math.pi/6+math.pi*k/3
        d = np.abs((XT-cx)*math.sin(a)-(NT-cn)*math.cos(a))
        along = (XT-cx)*math.cos(a)+(NT-cn)*math.sin(a)
        path = (d < 2.5) & (along > 0) & (rr < 92)
        colour[path] = (0.42, 0.39, 0.32)
        grass[path] = 0
    for (x0, x1, n0, n1) in [(-60, 60, -75, 75)]:
        pass
    plaza = raster([([rot(-60, -40, GRID_DEG), rot(-60, 40, GRID_DEG), rot(-6, 40, GRID_DEG), rot(-6, -40, GRID_DEG)], 255)],
                   TEX, lambda x, n: tex_px(x+GATE_X, n+GATE_N)) > 0
    colour[plaza] = np.array((0.30, 0.29, 0.27))*(0.9+0.1*fine[plaza])[:, None]
    grass[plaza] = 0
    pp = raster([([rot(6, -55, GRID_DEG), rot(6, 55, GRID_DEG), rot(132, 55, GRID_DEG), rot(132, -55, GRID_DEG)], 255)],
                TEX, lambda x, n: tex_px(x+GATE_X, n+GATE_N)) > 0
    colour[pp] = np.array((0.30, 0.29, 0.27))*(0.9+0.1*fine[pp])[:, None]
    grass[pp] = 0
    # the Reichstag's forecourt and the cleared Alsenviertel (sand and rubble of the 1939-42 demolitions)
    fore = (XT > -80) & (XT < 64) & (NT > -88) & (NT < 88)
    colour[fore] = np.array((0.31, 0.29, 0.26))*(0.9+0.12*fine[fore])[:, None]
    grass[fore] = 0
    alsen = (XT > -360) & (XT < 40) & (NT > 115) & (NT < 470)
    colour[alsen] = colour[alsen]*0.35+np.array((0.45, 0.40, 0.31))*(0.9+0.12*fine[alsen])[:, None]*0.65
    grass[alsen] *= 0.25
    # the rail yards north of the river (ballast)
    rail = (XT > -720) & (XT < -60) & (NT > 380) & ~((XT > -350) & (XT < -170) & (NT > 540))
    colour[rail] = colour[rail]*0.3+np.array((0.25, 0.22, 0.19))*(0.85+0.2*fine[rail])[:, None]*0.7
    grass[rail] *= 0.2
    # water, quays, the cut, the trenches and craters (from the heightfield)
    Hup = up(H-H_BASE)
    wet = up(water.astype(np.float32)) > 0.5
    colour[wet] = (0.10, 0.11, 0.10)
    grass[wet] = 0
    low = (Hup < -0.35) & ~wet
    colour[low] = np.array((0.19, 0.16, 0.12))*(0.9+0.15*fine[low])[:, None]
    grass[low] *= 0.2
    high = Hup > 0.3
    colour[high] = colour[high]*0.4+np.array((0.42, 0.33, 0.27))*0.6
    grass[high] *= 0.3
    # rubble aprons in front of the city blocks
    for cx_, cn_, w, d, out_deg, oid in lots:
        a = math.radians(out_deg)
        fx, fn = cx_+math.sin(a)*(d/2+2.0), cn_+math.cos(a)*(d/2+2.0)
        px, pz = tex_px(fx, fn)
        r = int(max(w, 8)*0.55*TEX/(2*HALF))
        i0, i1 = max(int(px)-r, 0), min(int(px)+r, TEX)
        j0, j1 = max(int(pz)-r, 0), min(int(pz)+r, TEX)
        if i1 > i0 and j1 > j0:
            colour[j0:j1, i0:i1] = colour[j0:j1, i0:i1]*0.55+np.array((0.43, 0.33, 0.27))*0.45
    return colour, grass


def write_plan(path, colour, P, route):
    im = Image.fromarray(np.uint8(np.rint(np.clip(colour[::2, ::2], 0, 1)*255))).convert('RGB')
    d = ImageDraw.Draw(im)
    s = (TEX//2-1)/(2*HALF)
    for row in P.placements:
        x, z = row[1], row[3]
        name = P.assets[row[0]]
        col = (255, 120, 0) if '/berlin_' in name else (255, 40, 40) if 'worlds/berlin1945' in name else (60, 160, 60)
        r = 3 if 'worlds/berlin1945' in name and '/berlin_' not in name and 'tree' not in name else 1
        d.ellipse([(x+HALF)*s-r, (z+HALF)*s-r, (x+HALF)*s+r, (z+HALF)*s+r], fill=col)
    d.line([((x+HALF)*s, (-n+HALF)*s) for x, n in route], fill=(255, 255, 255), width=2)
    im.save(path)


if __name__ == '__main__':
    ap = argparse.ArgumentParser()
    ap.add_argument('--plan', type=Path)
    a = ap.parse_args()
    build(a.plan)
