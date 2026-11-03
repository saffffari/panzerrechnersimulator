#!/usr/bin/env python3
"""Spring bridge test map: a river town on the spring steppe, its bridge to take and hold.

The steppe grammar (tools/worldgen/steppe_spring.py) lays the ground, the road and the outlying
farms with the road running west to east through the map's centre. This script then:

- cuts a river north to south through the centre: a floodplain valley, a 44 m channel with cut
  banks too steep for a tank (src/tank_support.rs: 35 deg, 0.76 m step), so the bridge is the
  crossing; the water level falls gently downstream;
- puts a road bridge where the road meets the river and grades its approaches;
- lays a town on both banks on streets (a quay street on each bank, a back street, cross streets),
  brick houses near the bridge, timber houses and workshops further out;
- moves the start a few hundred metres west of the town so the expedition camp, which sits
  40-512 m from the start away from the goal (src/support_camp_runtime.rs), lands just outside it;
- posts the street platoons on the town's streets, one loop on each bank;
- writes `river` and `crossings` blocks for the engine (water surface and deck support).

A generated test map, not a survey of a real town or crossing.

Run: KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem python3 tools/worldgen/spring_bridge.py [--plan PNG]
"""
import argparse
import json
import math
import os
import sys
from pathlib import Path

import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import steppe_spring as sp  # noqa: E402  (sets the asset root on the grammar modules)

cc, gaea = sp.cc, sp.gaea
ASSET_ROOT = sp.ASSET_ROOT
DEST = ASSET_ROOT/'assets/worldgen/workshops/spring-bridge-v1'
TERRAIN = ASSET_ROOT/'assets/terrain/spring_bridge'
VERSION = 1
SEED = 0x5747_1943          # the steppe's own seed: same hills, same woods outside the valley
EXTENT, RES, TEX = sp.EXTENT, sp.RES, sp.TEX

# The road: west edge to east edge through the centre (4096-grid pixels, x then z).
ANCHORS = [(330, 2080), (1150, 1990), (2048, 2048), (2950, 2110), (3760, 2020)]
# Outlying settlements only; the town at the centre is laid out below.
GROUPS = [
    ('hamlet', (.03, .10), (18., 105.), {'wooden_house': .9, 'merchant_brick_house': .1}, 12),
    ('farm', (.68, .76), (22., 105.), {'wooden_house': .7, 'workshop': .3}, 14),
    ('strongpoint', (.86, .95), (22., 125.), {'wooden_house': .5, 'merchant_brick_house': .3, 'workshop': .2}, 24),
]

RIVER_HALF_W = 22.0        # water edge to centreline
BANK_RUN = 2.5             # the cut bank: from bed to bank top over this run (about 66 deg)
BED_DEPTH = 2.6            # bed below the water
BANK_ABOVE = 3.2           # bank top above the water
FLOOD_HALF = 150.0         # flat floodplain each side of the centreline
VALLEY_RUN = 300.0         # floodplain to the untouched steppe
MEANDER_M, MEANDER_L = 70.0, 2400.0
TOWN_HALF_X, TOWN_HALF_Z = 340.0, 270.0
DECK_ABOVE_BANK = 0.6
BRIDGE_HALF_W = 5.5
BRIDGE_HALF_L = RIVER_HALF_W+BANK_RUN+10.0
START_FROM_TOWN_M = 330.0  # start this far outside the town's west edge
CONDITIONS = [('intact', .5), ('shelled', .25), ('burned', .15), ('partly_collapsed', .10)]


def smoothstep(t):
    t = np.clip(t, 0.0, 1.0)
    return t*t*(3-2*t)


def river_x(z, cx, cz):
    """The river's centreline: straight north-south through the bridge, bowing east away from it."""
    u = z-cz
    return cx+MEANDER_M*np.sin(2*np.pi*u/MEANDER_L)*(1-np.exp(-(u/320.0)**2))


def gauss1d(v, sigma_cells):
    k = int(3*sigma_cells)
    w = np.exp(-0.5*(np.arange(-k, k+1)/sigma_cells)**2)
    w /= w.sum()
    pad = np.pad(v, k, mode='edge')
    return np.convolve(pad, w, mode='valid')


def carve(H, cx, cz):
    """River valley, channel and cut banks. Returns new heights, the signed offset field and levels."""
    axis = np.linspace(-EXTENT/2, EXTENT/2, RES)
    X, Z = np.meshgrid(axis, axis)
    grid = gaea.SurfaceGrid(RES, EXTENT/(RES-1))
    c = river_x(axis, cx, cz)                                   # centreline per row (z)
    centre_h = np.array([grid.sample(H, float(x), float(z)) for x, z in zip(c, axis)])
    # The water level: the ground along the course, smoothed over a kilometre, a little below it,
    # never rising downstream (north, -z, is upstream).
    water = gauss1d(centre_h, 1000.0/(EXTENT/(RES-1))/2.0)-3.0
    water = np.minimum.accumulate(water)
    water_row = water[:, None]
    d = X-c[:, None]
    ad = np.abs(d)
    bank = water_row+BANK_ABOVE
    bed = water_row-BED_DEPTH
    # Floodplain: lower (never raise) the steppe toward the bank level across the valley.
    s = smoothstep((ad-FLOOD_HALF)/VALLEY_RUN)
    valley = bank+(H-bank)*s
    H1 = np.where(H > valley, valley, H)
    # A low spot in the floodplain is filled to the bank so the channel has two banks to cut.
    # ...faded in over the floodplain's outer 150 m, never a step (a hard fill left a cliff).
    fill = 1.0-smoothstep((ad-(FLOOD_HALF-150.0))/150.0)
    H1 = H1+np.maximum(bank-H1, 0.0)*fill
    wall = bed+(bank-bed)*np.clip((ad-RIVER_HALF_W)/BANK_RUN, 0, 1)
    H2 = np.where(ad < RIVER_HALF_W+BANK_RUN, np.minimum(H1, wall), H1)
    return H2, d, c, water, axis


def grade_bridge(H, axis, cx, cz, deck_y):
    """Abutments at deck height under the deck's ends and ramps down to the bank either side."""
    X, Z = np.meshgrid(axis, axis)
    ax = np.abs(X-cx)
    lane = np.abs(Z-cz) < BRIDGE_HALF_W+2.5
    seat = lane & (ax >= RIVER_HALF_W+BANK_RUN) & (ax <= BRIDGE_HALF_L)
    H[seat] = deck_y-0.05
    ramp = lane & (ax > BRIDGE_HALF_L) & (ax < BRIDGE_HALF_L+45.0)
    t = (ax[ramp]-BRIDGE_HALF_L)/45.0
    H[ramp] = np.maximum(H[ramp], (deck_y-0.05)*(1-t)+H[ramp]*t)


def town(cx, cz):
    """Streets and building records on both banks. Coordinates are world metres."""
    streets = []      # (x0, x1, z0, z1) rectangles, painted and kept clear
    blocks = []       # (x0, x1, z0, z1, bank, ring)
    main_w, street_w = 12.0, 10.0
    streets.append((cx-TOWN_HALF_X-60, cx+TOWN_HALF_X+60, cz-main_w/2, cz+main_w/2))
    for s in (-1, 1):
        quay = cx+s*(RIVER_HALF_W+BANK_RUN+9.0)
        back = cx+s*190.0
        for x in (quay, back):
            streets.append((x-street_w/2, x+street_w/2, cz-TOWN_HALF_Z, cz+TOWN_HALF_Z))
        lo, hi = sorted((quay+s*street_w/2, cx+s*TOWN_HALF_X))
        for zc in (cz-135.0, cz+135.0):
            streets.append((lo, hi, zc-street_w/2, zc+street_w/2))
        inner = sorted((quay+s*(street_w/2+1), back-s*(street_w/2+1)))
        outer = sorted((back+s*(street_w/2+1), cx+s*TOWN_HALF_X))
        for z0, z1 in ((cz+main_w/2+1, cz+130.0-1), (cz+140.0+1, cz+TOWN_HALF_Z), (cz-130.0+1, cz-main_w/2-1),
                       (cz-TOWN_HALF_Z, cz-140.0-1)):
            blocks.append((inner[0], inner[1], z0, z1, s, 0))
            blocks.append((outer[0], outer[1], z0, z1, s, 1))
    buildings, rects = [], []
    n = 0

    def pick(key, table):
        r = cc.rnd(key, 7)
        acc = 0.0
        for name, p in table:
            acc += p
            if r < acc:
                return name
        return table[-1][0]

    for bi, (x0, x1, z0, z1, bank, ring) in enumerate(blocks):
        near = ring == 0 and min(abs(z0-cz), abs(z1-cz)) < 20.0
        # Perimeter lots along the block's edges, facing the street; the yard stays open.
        toward_main = ('n', z0) if z0 > cz else ('s', z1)
        edges = [toward_main] if ring == 1 else [('n', z0), ('s', z1)] if abs((z0+z1)/2-cz) < 140 else [toward_main]
        for side, ze in edges:
            x = x0+2.0
            k = 0
            while True:
                key = f'bridge.{bi}.{side}.{k}'
                if ring == 0:
                    typ = 'apartment_block' if (near and k == 0 and side == ('n' if ze > cz else 's')) else \
                        pick(key, [('merchant_brick_house', .7), ('workshop', .15), ('wooden_house', .15)])
                else:
                    typ = pick(key, [('wooden_house', .7), ('workshop', .2), ('merchant_brick_house', .1)])
                w = {'apartment_block': 24.0, 'merchant_brick_house': [12.0, 16.0, 20.0][int(cc.rnd(key, 1)*3)],
                     'workshop': [12.0, 16.0][int(cc.rnd(key, 1)*2)], 'wooden_house': [8.0, 12.0][int(cc.rnd(key, 1)*2)]}[typ]
                d = {'apartment_block': 12.0, 'merchant_brick_house': 12.0, 'workshop': 12.0,
                     'wooden_house': [8.0, 12.0][int(cc.rnd(key, 2)*2)]}[typ]
                if x+w > x1-2.0:
                    break
                bx = x+w/2
                bz = ze+d/2+1.0 if side == 'n' else ze-d/2-1.0
                yaw = 180.0 if side == 'n' else 0.0
                rect = cc.footprint(bx, bz, w, d, yaw)
                if all(cc.rect_gap(rect, r) >= 0.0 for r in rects) and (rect[3]-rect[2]) <= (z1-z0):
                    storeys = {'apartment_block': 3+int(cc.rnd(key, 3)*2), 'merchant_brick_house': 1+int(cc.rnd(key, 3)*3),
                               'workshop': 1, 'wooden_house': 1}[typ]
                    cellar = typ in ('merchant_brick_house', 'apartment_block') and cc.rnd(key, 4) < .5
                    gap_lot = cc.rnd(key, 6) < (0.30 if ring == 0 else 0.55)   # an empty lot, a garden
                    if not gap_lot:
                        n += 1
                        bank_name = 'w' if bank < 0 else 'e'
                        buildings.append(dict(id=f'canyon.hamlet.bridge_{bank_name}_{n:02d}', typology=typ,
                            seed=int(cc.uid.fnv1a32(key)), width_m=w, depth_m=d, storeys=storeys, cellar=bool(cellar),
                            condition=pick(key+'c', CONDITIONS), x=cc.r3(bx), z=cc.r3(bz), yaw_deg=yaw))
                        rects.append(rect)
                x += w+(2.0+4.0*cc.rnd(key, 5) if ring == 0 else 8.0+10.0*cc.rnd(key, 5))
                k += 1
    return streets, buildings, rects


def pad(H, axis, rect, apron=6.0):
    """Flatten a building's footprint to its mean ground and blend back over an apron."""
    x0, x1, z0, z1 = rect
    sp_ = axis[1]-axis[0]
    i0, i1 = np.searchsorted(axis, [x0-apron-sp_, x1+apron+sp_])
    j0, j1 = np.searchsorted(axis, [z0-apron-sp_, z1+apron+sp_])
    if i1 <= i0 or j1 <= j0:
        return
    X, Z = np.meshgrid(axis[i0:i1], axis[j0:j1])
    dx = np.maximum(np.maximum(x0-X, X-x1), 0)
    dz = np.maximum(np.maximum(z0-Z, Z-z1), 0)
    dist = np.hypot(dx, dz)
    inside = dist <= 0
    sub = H[j0:j1, i0:i1]
    level = float(sub[inside].mean()) if inside.any() else float(sub.mean())
    t = smoothstep(dist/apron)
    H[j0:j1, i0:i1] = level+(sub-level)*t


def straighten_route(route, cx, cz):
    """The road through the town is the main street: a straight line over the bridge."""
    xs = route[:, 0]
    inside = np.where(np.abs(xs-cx) < TOWN_HALF_X+420)[0]
    i0, i1 = int(inside.min()), int(inside.max())
    a, b = route[i0], route[i1]          # the road eases onto the main street over 360 m
    pts = [a, np.array([cx-TOWN_HALF_X-60, cz]), np.array([cx+TOWN_HALF_X+60, cz]), b]
    line = []
    for p, q in zip(pts, pts[1:]):
        n = max(int(np.hypot(*(q-p))/5.0), 1)
        for t in np.linspace(0, 1, n, endpoint=False):
            line.append(p+(q-p)*t)
    return np.vstack([route[:i0], np.array(line), route[i1:]])


def ground_paint(colour, grass, d_tex, streets, rects, axis_tex):
    """River, banks, streets and yards over the steppe's ground."""
    ad = np.abs(d_tex)
    water = ad < RIVER_HALF_W
    bank = (ad >= RIVER_HALF_W) & (ad < RIVER_HALF_W+BANK_RUN+3.0)
    colour[water] = [0.09, 0.10, 0.08]
    colour[bank] = [0.30, 0.25, 0.18]
    grass[water | bank] = 0.0
    shore = np.clip(1.0-(ad-RIVER_HALF_W-BANK_RUN)/40.0, 0, 1)[..., None]*0.45   # wet meadow along the river
    colour[:] = colour*(1-shore)+np.array([0.18, 0.28, 0.10])*shore
    to_px = (TEX-1)/EXTENT
    for x0, x1, z0, z1 in streets:
        i0, i1 = int((x0+EXTENT/2)*to_px), int((x1+EXTENT/2)*to_px)+1
        j0, j1 = int((z0+EXTENT/2)*to_px), int((z1+EXTENT/2)*to_px)+1
        sl = (slice(max(j0, 0), min(j1, TEX)), slice(max(i0, 0), min(i1, TEX)))
        colour[sl] = [0.33, 0.30, 0.26]
        grass[sl] = 0.0
    for x0, x1, z0, z1 in rects:
        m = 5.0
        i0, i1 = int((x0-m+EXTENT/2)*to_px), int((x1+m+EXTENT/2)*to_px)+1
        j0, j1 = int((z0-m+EXTENT/2)*to_px), int((z1+m+EXTENT/2)*to_px)+1
        sl = (slice(max(j0, 0), min(j1, TEX)), slice(max(i0, 0), min(i1, TEX)))
        colour[sl] = colour[sl]*0.4+np.array([0.31, 0.27, 0.20])*0.6
        grass[sl] *= 0.2


def carry_editor_work(path, edits):
    """Keep what the world editor saved across a regeneration: every key this script does not
    write, and the objects and markers added in the editor. Changes to generated objects are by
    index into the old generation and cannot be carried; they are reported, not applied."""
    if not path.exists():
        return {}
    old = json.loads(path.read_text())
    kept = {k: v for k, v in old.items() if k not in edits and k not in ('base_placements', 'base_markers', 'ops')}
    edits.update(kept)
    ops = old.get('ops', [])
    adds = [op for op in ops if op.get('op') in ('add', 'marker_add')]
    lost = [op for op in ops if op.get('op') not in ('add', 'marker_add')]
    edits['ops'] = adds
    report = dict(keys=sorted(kept), added=len(adds), not_carried=len(lost))
    print('[spring-bridge] editor work carried:', json.dumps(report))
    if lost:
        (path.parent/'editor_ops_not_carried.json').write_text(json.dumps(lost, indent=1))
    return report


def line(text, sender='Adler'):
    return [dict({'from': sender, 'text': text})]


def mission_sheet(cx, cz, route):
    """The order: take the bridge, hold it through two counterattacks, destroy every tank."""
    seg = np.hypot(*np.diff(route, axis=0).T)
    along = np.concatenate([[0.0], np.cumsum(seg)])
    def frac(x, z):
        return round(float(along[int(np.argmin(np.hypot(route[:, 0]-x, route[:, 1]-z)))]/along[-1]), 4)
    first, second = frac(cx+900, cz), frac(cx+1400, cz)
    p = lambda x, z: {'x': cc.r3(x), 'z': cc.r3(z)}
    return {
        'schema': 'kessel.mission', 'version': 1, 'id': 'spring-bridge', 'act_id': 1,
        'title': 'BEFEHL: BRUECKE NEHMEN UND HALTEN', 'callsign': 'Kiebitz', 'net': 'Adler',
        'note': ('Spring bridge test map (tools/worldgen/spring_bridge.py). Places are the map\'s own geometry: the '
                 'town\'s west edge, its west-bank streets, the bridge deck, the east road the counterattacks come down. '
                 'The garrison is the street platoons, one loop on each bank; the counterattacks are platoons released '
                 'on the east road when the bridge is taken. A generated test map, not a recorded engagement.'),
        'order': ['Panzer 231. Crew: you alone, the camp and the half-track behind you on the west road.',
                  'The road bridge in the town is intact. Soviet tanks hold both banks of the town.',
                  'Take the bridge and hold it. Expect armour from the east once you are on it. Destroy every tank.'],
        'beats': [
            {'id': 'edge', 'title': 'The town\'s west edge', 'goal': 'reach', 'at': p(cx-TOWN_HALF_X, cz), 'radius_m': 120.0,
             'complete': {'kind': 'enter', 'dwell_s': 1.0},
             'lines': {'open': line('Kiebitz, Adler. The town is 300 metres east. Bridge in its centre. Out.'),
                       'done': line('Kiebitz at the edge. Tanks in the streets on this bank. Out.')},
             'map': {'mark': 'box', 'bearing': True}},
            {'id': 'west_bank', 'title': 'Clear the west bank', 'goal': 'destroy', 'at': p(cx-110, cz), 'radius_m': 200.0,
             'complete': {'kind': 'destroy_near', 'count': 2, 'radius_m': 320.0},
             'cues': [{'kind': 'alert', 'radius_m': 420.0}],
             'lines': {'open': line('Kiebitz, Adler. Clear the streets to the river. Out.'),
                       'done': line('West bank clear. The bridge. Out.')},
             'map': {'mark': 'cross', 'bearing': True}},
            {'id': 'bridge', 'title': 'Take the bridge', 'goal': 'hold', 'at': p(cx, cz), 'radius_m': 30.0,
             'complete': {'kind': 'capture', 'seconds': 20.0, 'contest_radius_m': 200.0},
             'cues': [{'kind': 'alert', 'at': p(cx+150, cz), 'radius_m': 500.0}],
             'lines': {'open': line('Kiebitz, Adler. On the bridge. Nothing of theirs within 200 metres while you hold it. Out.'),
                       'done': line('Bridge taken. Armour on the east road. Hold. Out.')},
             'map': {'mark': 'circle', 'bearing': True}},
            {'id': 'first_counterattack', 'title': 'Hold: the first counterattack', 'goal': 'hold', 'at': p(cx, cz), 'radius_m': 120.0,
             'complete': {'kind': 'capture', 'seconds': 60.0, 'contest_radius_m': 250.0},
             'pacing': {'release': {'count': 3, 'fraction': first}},
             'lines': {'open': line('Kiebitz, Adler. Three tanks coming down the east road. Hold the bridge. Out.'),
                       'done': line('First attack beaten off. More behind it. Out.')},
             'map': {'mark': 'circle', 'bearing': True}},
            {'id': 'second_counterattack', 'title': 'Hold: the second counterattack', 'goal': 'hold', 'at': p(cx, cz), 'radius_m': 120.0,
             'complete': {'kind': 'capture', 'seconds': 60.0, 'contest_radius_m': 250.0},
             'pacing': {'release': {'count': 3, 'fraction': second}},
             'lines': {'open': line('Kiebitz, Adler. Second attack, three tanks, east road. Out.'),
                       'done': line('Second attack beaten off. Out.')},
             'map': {'mark': 'circle', 'bearing': True}},
            {'id': 'all_tanks', 'title': 'Destroy every tank', 'goal': 'destroy', 'at': p(cx, cz), 'radius_m': 120.0,
             'complete': {'kind': 'destroy_all'},
             'lines': {'open': line('Kiebitz, Adler. Tanks still in the town. Destroy them. Out.'),
                       'done': line('Kiebitz, Adler. No more armour. The bridge is ours. Out.')},
             'map': {'mark': 'cross', 'bearing': False}},
        ]}


def build(dest=DEST, plan=None):
    rng = np.random.default_rng(SEED)
    H0, ravine, _ = sp.heights(rng)
    sp.SOURCE = ASSET_ROOT/'assets/terrain/spring_bridge/source'
    source = sp.write_source(H0)
    gaea.ANCHORS = ANCHORS
    cc.GROUPS = GROUPS
    cc.CRATERS = 14
    cc.RAZED_PLOTS = 3
    cc.ASSETS['tree_tall'] = sp.NAT+'tree_5.glb'
    cc.ASSETS['tree_small'] = sp.NAT+'tree_7.glb'
    manifest, edits, H, receipt = gaea.build(source)
    drifts = {i for i, p in enumerate(manifest['assets']) if 'snow_drift' in p}
    manifest['placements'] = [p for p in manifest['placements'] if p[0] not in drifts]
    route = np.array(edits['canyon_course']['route'], dtype=np.float64)
    # The bridge stands where the road comes nearest the map's centre.
    k = int(np.argmin(np.hypot(route[:, 0], route[:, 1])))
    cx, cz = float(np.round(route[k, 0])), float(np.round(route[k, 1]))
    H_before = H.copy()
    H, d, centre, water, axis = carve(H, cx, cz)
    row = int(np.argmin(np.abs(axis-cz)))
    bank_y = float(water[row]+BANK_ABOVE)
    deck_y = bank_y+DECK_ABOVE_BANK
    streets, town_buildings, town_rects = town(cx, cz)
    # Clear the valley and the town of the grammar's houses and dressing.
    def in_town(x, z, m=0.0):
        return abs(x-cx) < TOWN_HALF_X+m and abs(z-cz) < TOWN_HALF_Z+m

    def in_river(x, z, m):
        return abs(x-float(np.interp(z, axis, centre))) < RIVER_HALF_W+BANK_RUN+m
    kept = [b for b in edits['building_instances']
            if not in_town(b['x'], b['z'], 40.0) and not in_river(b['x'], b['z'], 60.0)]
    dropped_buildings = len(edits['building_instances'])-len(kept)
    # Streets first (flat across their width), then the houses' pads over them.
    for x0, x1, z0, z1 in streets:
        pad(H, axis, (x0, x1, z0, z1), 3.0) if (x1-x0) < 20 or (z1-z0) < 20 else None
    for b in kept:
        pad(H, axis, cc.footprint(b['x'], b['z'], b['width_m'], b['depth_m'], b['yaw_deg']))
    for r in town_rects:
        pad(H, axis, r, 4.0)
    grade_bridge(H, axis, cx, cz, deck_y)
    # Re-cut the channel after the pads so no pad bridges the water.
    X, Z = np.meshgrid(axis, axis)
    ad = np.abs(X-centre[:, None])
    wall = (water[:, None]-BED_DEPTH)+(BANK_ABOVE+BED_DEPTH)*np.clip((ad-RIVER_HALF_W)/BANK_RUN, 0, 1)
    channel = ad < RIVER_HALF_W+BANK_RUN
    H = np.where(channel, np.minimum(H, wall), H)
    edits['building_instances'] = kept+town_buildings
    # Placements: drop those in the river, the town and on the streets; reseat the rest.
    grid = gaea.SurfaceGrid(RES, EXTENT/(RES-1))
    before = len(manifest['placements'])
    placed = []
    for p in manifest['placements']:
        x, z = p[1], p[3]
        if in_town(x, z, 20.0) or in_river(x, z, 12.0):
            continue
        p = list(p)
        p[2] = cc.r3(p[2]+grid.sample(H, x, z)-grid.sample(H_before, x, z))
        placed.append(p)
    manifest['placements'] = placed
    # The road over the bridge.
    route = straighten_route(route, cx, cz)
    course = edits['canyon_course']
    course['route'] = [[cc.r3(x), cc.r3(z)] for x, z in route]
    start_x = cx-TOWN_HALF_X-START_FROM_TOWN_M
    s = int(np.argmin(np.abs(route[:, 0]-start_x)+1e3*(route[:, 0] > cx)))
    heading = math.degrees(math.atan2(*(route[min(s+3, len(route)-1)]-route[s])))
    course['start'] = dict(x=cc.r3(route[s, 0]), z=cc.r3(route[s, 1]), heading_deg=cc.r3(heading))
    street = edits['street_encounter']
    # The town's two platoons drive the street rectangles, one on each bank.
    def loop(sgn):
        # The rectangle of streets round the inner blocks, a knot every 15 m (each knot must
        # project onto ground measured clear for a T-34, src/canyon_game.rs).
        quay = cx+sgn*(RIVER_HALF_W+BANK_RUN+9.0)
        back = cx+sgn*190.0
        corners = [(quay, cz-135), (quay, cz+135), (back, cz+135), (back, cz-135)]
        pts = []
        for (ax, az), (bx, bz) in zip(corners, corners[1:]+corners[:1]):
            n = max(int(math.hypot(bx-ax, bz-az)/15.0), 1)
            pts += [[cc.r3(ax+(bx-ax)*k/n), cc.r3(az+(bz-az)*k/n)] for k in range(n)]
        return pts
    street['beats'] = [dict(platoon='A', points=loop(-1)), dict(platoon='B', points=loop(1))]
    # Gun sites inside the town or the river are withdrawn; the zones' candidates are reindexed.
    remap, sites = {}, []
    for i, g in enumerate(street['gun_sites']):
        if not (in_town(g['x'], g['z'], 30.0) or in_river(g['x'], g['z'], 15.0)):
            remap[i] = len(sites)
            sites.append(g)
    street['gun_sites'] = sites
    for zone in course['gun_zones']:
        zone['candidates'] = [remap[i] for i in zone['candidates'] if i in remap]
    for kz in street['kill_zone']:
        if kz['id'] == 'route.whole':
            kz['points'] = [[cc.r3(x), cc.r3(z)] for x, z in route[::16]]
    cors = []
    for a, b in zip(route[::40], route[40::40]):
        on_bridge = abs(a[0]-cx) < BRIDGE_HALF_L+20 or abs(b[0]-cx) < BRIDGE_HALF_L+20
        cors.append(dict(a=[cc.r3(v) for v in a], b=[cc.r3(v) for v in b], width_m=2*BRIDGE_HALF_W if on_bridge else 40.0))
    street['ways']['corridors'] = cors
    xs = [v for c in cors for v in (c['a'][0], c['b'][0])]
    zs = [v for c in cors for v in (c['a'][1], c['b'][1])]
    street['ways']['district'] = [cc.r3(min(xs)-260.0), cc.r3(max(xs)+260.0),
                                  cc.r3(min(min(zs), cz-TOWN_HALF_Z)-60.0), cc.r3(max(max(zs), cz+TOWN_HALF_Z)+60.0)]
    sx, sz = course['start']['x'], course['start']['z']
    street['ways']['rects'] = [[cc.r3(sx-40), cc.r3(sx+40), cc.r3(sz-40), cc.r3(sz+40)]]
    # Riflemen in the town: posts at the corners of the brick houses near the bridge.
    posts = []
    for b in town_buildings:
        if b['typology'] in ('merchant_brick_house', 'apartment_block') and abs(b['x']-cx) < 200:
            x0, x1, z0, z1 = cc.footprint(b['x'], b['z'], b['width_m'], b['depth_m'], b['yaw_deg'])
            posts.append(dict(x=cc.r3(x0-2), z=cc.r3(z0-2), heading_deg=270.0 if b['x'] > cx else 90.0, host=b['id']))
    course['garrison'] = posts[:18]
    course['design'] = [dict(name=n, from_fraction=a, to_fraction=b) for n, a, b in [
        ('camp_and_approach', 0.0, .35), ('west_bank_streets', .35, .48), ('the_bridge', .48, .52),
        ('east_bank_streets', .52, .62), ('counterattack_ground', .62, 1.0)]]
    # Markers follow the course.
    for m in manifest['markers']:
        if m['kind'] == 'Spawn':
            m.update(x=course['start']['x'], z=course['start']['z'], y=cc.r3(grid.sample(H, sx, sz)))
    manifest['markers'] = [m for m in manifest['markers'] if m['kind'] in ('Spawn', 'Goal')]
    manifest['markers'].append(dict(name='the_bridge', kind='Bridge', x=cx, z=cz, y=cc.r3(deck_y)))
    edits['base_markers'] = len(manifest['markers'])
    # The engine's river and crossing records (src/river_crossings.rs).
    step = 25
    manifest['river'] = dict(schema='kessel.river', version=1, half_width_m=RIVER_HALF_W, bank_run_m=BANK_RUN,
        bed_depth_m=BED_DEPTH, centreline=[[cc.r3(float(centre[i])), cc.r3(float(axis[i])), cc.r3(float(water[i]))]
                                           for i in range(0, RES, step)] + [[cc.r3(float(centre[-1])), cc.r3(float(axis[-1])), cc.r3(float(water[-1]))]],
        note='centreline points are [x, z, water_y]; the water surface is flat across the channel')
    manifest['crossings'] = [dict(center=[cx, cz], deck_y=cc.r3(deck_y), half_length=BRIDGE_HALF_L,
                                  half_width=BRIDGE_HALF_W, bed_y=cc.r3(float(water[row]-BED_DEPTH)), name='the_bridge')]
    # Woods and ground.
    buildings = edits['building_instances']
    woods = sp.wood_mask(rng, H, ravine, route, buildings)
    woods &= ~((np.abs(X-centre[:, None]) < FLOOD_HALF*0.5) | ((np.abs(X-cx) < TOWN_HALF_X+60) & (np.abs(Z-cz) < TOWN_HALF_Z+60)))
    trees = sp.plant_woods(manifest, H, woods)
    colour, grass = sp.ground(rng, H, ravine*(np.abs(X-centre[:, None]) > FLOOD_HALF), route, buildings, woods)
    axis_tex = np.linspace(-EXTENT/2, EXTENT/2, TEX, dtype=np.float32)
    centre_tex = np.interp(axis_tex, axis, centre).astype(np.float32)
    d_tex = axis_tex[None, :]-centre_tex[:, None]
    ground_paint(colour, grass, d_tex, streets, town_rects, axis_tex)
    TERRAIN.mkdir(parents=True, exist_ok=True)
    rgba = np.dstack([np.uint8(np.rint(np.clip(colour, 0, 1)*255)), np.full((TEX, TEX), 32, dtype=np.uint8)])
    Image.fromarray(rgba, 'RGBA').save(TERRAIN/'ground_material.png')
    cls = np.uint8(np.rint(np.clip(grass, 0, 1)*255))
    Image.fromarray(np.dstack([cls, np.zeros_like(cls), np.zeros_like(cls)]), 'RGB').save(TERRAIN/'ground_class.png')
    manifest['terrain'].update(texture='assets/terrain/spring_bridge/ground_material.png',
        material_encoding='gaea-rock-srgb-snow-linear-alpha32-223-v1',
        ground_class='assets/terrain/spring_bridge/ground_class.png', grass_season='spring', color=[0.9, 0.9, 0.9, 1.0])
    manifest['authoring'].update(tool='tools/worldgen/spring_bridge.py', tool_version=VERSION, variant='spring-bridge')
    gl = course['goal']
    mid = [cx, cz]
    manifest['overview'] = dict(pos=[cc.r3(cx+520), 420.0, cc.r3(cz+620)], target=[cx, cc.r3(deck_y), cz])
    # The editor's Home view: standing on the middle of the deck, looking east over the river.
    edits['editor_home'] = dict(eye=[cx, cc.r3(deck_y+1.8), cz], target=[cc.r3(cx+30.0), cc.r3(deck_y+0.8), cz])
    edits['base_placements'] = len(manifest['placements'])
    course.update(variant='spring-bridge', tool='tools/worldgen/spring_bridge.py')
    # The run ends with the mission (every tank dead), not in a goal disc; the only enemies are
    # the town's platoons and the mission's counterattacks (src/canyon.rs CourseSpec).
    course.update(finish='mission', director=False, opening_platoons=False, headline='The bridge is held',
                  camp=True)   # the expedition camp and half-track, sited from the start
    course['provenance'] = dict(
        terrain='Generated spring steppe with a river valley cut through it; not a survey of real ground.',
        buildings=cc.PROVENANCE['buildings']+' A river town on both banks: brick houses near the bridge, timber houses and workshops further out.',
        defence=cc.PROVENANCE['defence'],
        scope='Generative test map for a bridge assault and its defence, not a recorded engagement.')
    # A spring forenoon: the sun in the south-east, broken cloud, a little haze over the river.
    edits['environment_lighting'] = dict(enabled=True, fog_density=0.0009, overcast=0.36, storm=0.0,
        schema='kessel.environment-authoring', utc='1943-05-12T07:20:00Z', version=2)
    dest = Path(dest)
    dest.mkdir(parents=True, exist_ok=True)
    carried = carry_editor_work(dest/'manifest_edits.json', edits)
    (dest/'manifest.v1.json').write_text(cc.dumps(manifest, indent=None))
    (dest/'manifest_edits.json').write_text(cc.dumps(edits))
    np.save(dest/'heights.npy', H.astype(np.float32))
    (dest/'mission.v1.json').write_text(cc.dumps(mission_sheet(cx, cz, route)))
    report = dict(schema='panzer.spring-bridge', version=VERSION, seed=SEED, bridge=dict(x=cx, z=cz, deck_y=deck_y),
        town_buildings=len(town_buildings), outlying_buildings=len(kept), grammar_buildings_cleared=dropped_buildings,
        placements_cleared=before-len(placed), wood_trees=trees, start=course['start'], goal=gl,
        height_range_m=[float(H.min()), float(H.max())], water_y_range_m=[float(water.min()), float(water.max())],
        outputs={n: cc.sha256_file(dest/n) for n in cc.OUTPUT_FILES})
    (dest/'spring-bridge.receipt.json').write_text(cc.dumps(report))
    if plan:
        preview = np.uint8(np.rint(np.clip(colour[::4, ::4], 0, 1)*255)).copy()
        to_px = (TEX//4-1)/EXTENT
        for x, z in route:
            px, pz = int((x+EXTENT/2)*to_px), int((z+EXTENT/2)*to_px)
            preview[max(pz-1, 0):pz+2, max(px-1, 0):px+2] = [255, 255, 255]
        for b in buildings:
            px, pz = int((b['x']+EXTENT/2)*to_px), int((b['z']+EXTENT/2)*to_px)
            preview[max(pz-1, 0):pz+2, max(px-1, 0):px+2] = [230, 120, 40]
        for name, (x, z) in (('start', (sx, sz)), ('bridge', (cx, cz))):
            px, pz = int((x+EXTENT/2)*to_px), int((z+EXTENT/2)*to_px)
            preview[max(pz-4, 0):pz+5, max(px-4, 0):px+5] = [255, 40, 40]
        Image.fromarray(preview).save(plan)
    print(json.dumps({k: report[k] for k in ('bridge', 'town_buildings', 'outlying_buildings', 'start',
                                              'height_range_m', 'water_y_range_m', 'wood_trees')}, indent=1))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--dest', type=Path, default=DEST)
    parser.add_argument('--plan', type=Path)
    args = parser.parse_args()
    build(args.dest, args.plan)
