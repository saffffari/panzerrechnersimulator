#!/usr/bin/env python3
"""Spring bridge test map, version 2: a ruined river town on organic streets, a high bridge,
on a 2.5 km square of spring steppe. Two densities from one layout:

    spring-bridge-v2       playtest density: every house, kerb, tree and piece of the warzone
    spring-bridge-lite-v2  render-test density: the same town and river, a fraction of the dressing

What it builds (on top of the spring steppe grammar, tools/worldgen/steppe_spring.py):

- a 70 m river north to south through the centre in a floodplain valley; cut banks too steep for
  a tank (src/tank_support.rs), so the bridge is the crossing;
- a road bridge 8 m above the water on piers in the stream, its deck landing on earth abutments
  with ramps down into the town (nothing passes under the deck: the deck's support is its
  footprint, src/river_crossings.rs);
- a town on both banks on organic streets: a square at each bridgehead, a quay along each bank
  following the river, lanes fanning out from the squares at irregular angles and bending, alleys
  off the quays. Houses face their street; the building grammar admits quarter-turn yaws only
  (src/building_grammar.rs), so on a slanting lane the houses step along it in a sawtooth;
- the town in ruins: most houses burned, shelled or partly collapsed (the grammar's own rubble),
  debris, bricks and charred timber round them, craters in the streets, barricades and sandbags at
  the east bridgehead, wrecked armour and transport, the dead, kerbstones along the main street
  and the quays;
- many more trees: woods, street trees, the river's banks, gardens behind the outer houses;
- a bright, warm afternoon in May with blue haze.

The objects the owner added in the world editor (manifest_edits.json `add` ops of v1 or of this
map) are carried across a regeneration at their world positions.

A generated test map: the town, river and fighting are attested recurring conditions of the
1943-44 river crossings on the Eastern Front, not a survey of a real town or a recorded
engagement.

Run: KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem python3 tools/worldgen/spring_bridge_v2.py
         [--variant playtest|render] [--carry-from WORKSHOP_DIR] [--plan PNG]
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
import spring_bridge as v1  # noqa: E402

sp, cc, gaea = v1.sp, v1.cc, v1.gaea
ASSET_ROOT = v1.ASSET_ROOT
WORKSHOPS = ASSET_ROOT/'assets/worldgen/workshops'
VERSION = 2
EXTENT, RES, TEX = sp.EXTENT, sp.RES, sp.TEX
CROP_HALF = 1250.0                      # the map: a 2.5 km square centred on the map's origin
PZ = 'assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/'
NATP = sp.NAT

# River and bridge.
RIVER_HALF_W = 35.0
BANK_RUN, BED_DEPTH, BANK_ABOVE = 2.5, 3.0, 3.2
FLOOD_HALF, VALLEY_RUN = 260.0, 360.0
DECK_ABOVE_WATER = 8.0
BRIDGE_HALF_W = 6.0
BRIDGE_HALF_L = RIVER_HALF_W+BANK_RUN+12.0
RAMP_M = 90.0
SQUARE_R = 30.0

VARIANTS = {
    'playtest': dict(dest='spring-bridge-v2', lot_gap=(0.0, 3.0), outer_gap=(4.0, 10.0), dressing=1.0,
                     kerbs=True, trees=26000, street_tree_m=13.0, craters=34, alleys=True),
    'render': dict(dest='spring-bridge-lite-v2', lot_gap=(3.0, 8.0), outer_gap=(10.0, 20.0), dressing=0.35,
                   kerbs=False, trees=7000, street_tree_m=24.0, craters=12, alleys=False),
}
CONDITIONS = [('intact', .15), ('shelled', .30), ('burned', .25), ('partly_collapsed', .30)]

ASSET = {
    'kerb': PZ+'Props/concrete_block_mx_1.glb',           # 0.6 x 1.1 x 2.2 m, scaled to a kerbstone
    'debris_bricks': PZ+'Debris & Misc/debris_bricks_mx_1.glb', 'debris_bricks2': PZ+'Debris & Misc/debris_bricks_mx_2.glb',
    'bricks_stacked': PZ+'Debris & Misc/bricks_stacked_mx_2.glb', 'brick': PZ+'Debris & Misc/brick_mx_1.glb',
    'gravel': PZ+'Debris & Misc/gravel_pile_hr_1.glb', 'gravel2': PZ+'Debris & Misc/gravel_pile_hr_2.glb',
    'plank': PZ+'Props/wooden_plank_2.glb', 'board': PZ+'Props/wooden_board_1.glb',
    'crate': PZ+'Props/wooden_crate_1.glb', 'barrel': PZ+'Props/wooden_barrel_1.glb',
    'metal_barrel': PZ+'Props/metal_barrel_hr_2.glb', 'bench': PZ+'Props/bench_mx_1.glb',
    'cart_wheel_tire': PZ+'Props/tire_1.glb', 'cardboard': PZ+'Props/cardboard_box_2.glb',
    'barricade_a': PZ+'Modular Props/barricade_a_2.glb', 'barricade_b': PZ+'Modular Props/barricade_b_1.glb',
    'fence_a': PZ+'Modular Props/fence_a_1.glb', 'fence_b': PZ+'Modular Props/fence_b_1.glb',
    'chimney': PZ+'Large Props & Machinery/chimney_a_2.glb',
    'hedgehog': 'assets/warprops/hedgehog.glb', 'sandbags': 'assets/warprops/sandbag_wall.glb',
    'truck_burned': 'assets/warprops/truck_burned.glb', 'truck_hit': 'assets/warprops/truck_he_hit.glb',
    'wagon_wreck': 'assets/warprops/panje_wagon_wrecked.glb', 'gun_wreck': 'assets/warprops/field_gun_wrecked.glb',
    'horse_dead': 'assets/warprops/horse_dead.glb', 'helmet': 'assets/warprops/helmet_ground.glb',
    'rifle_grave': 'assets/warprops/rifle_marker.glb', 'grave_cross': 'assets/warprops/grave_cross.glb',
    'ammo_burst': 'assets/warprops/ammo_crate_burst.glb', 'drum_burst': 'assets/warprops/fuel_drum_burst.glb',
    'stretcher': 'assets/warprops/stretcher.glb',
    'dead_back': 'assets/warprops/dead/dead_sol_sprawl_back.glb', 'dead_down': 'assets/warprops/dead/dead_sol_face_down.glb',
    'dead_side': 'assets/warprops/dead/dead_sol_on_side.glb', 'dead_curled': 'assets/warprops/dead/dead_sol_curled.glb',
    'wreck_kv1': cc.ASSETS['wreck_kv1'], 'wreck_pz3': cc.ASSETS['wreck_pz3'],
    'charred': cc.ASSETS['charred_plank'], 'scorch': cc.ASSETS['scorch'],
}
BIG_TREES = ['tree_5', 'tree_6', 'tree_7', 'tree_8']
SMALL_TREES = ['tree_1', 'tree_9']
WATER_RGB = [44, 84, 116]          # a clear river under a blue sky; see the ledger's river water row
WATER_ROUGHNESS = 0.05


def smoothstep(t):
    t = np.clip(t, 0.0, 1.0)
    return t*t*(3-2*t)


def rnd(key, salt):
    return cc.rnd(key, salt)


# ── streets ──────────────────────────────────────────────────────────────────────────────────────
def bezier(p0, p1, p2, step=4.0):
    p0, p1, p2 = map(np.asarray, (p0, p1, p2))
    length = np.linalg.norm(p1-p0)+np.linalg.norm(p2-p1)
    t = np.linspace(0, 1, max(int(length/step), 2))[:, None]
    return (1-t)**2*p0+2*(1-t)*t*p1+t**2*p2


def lay_streets(cx, cz, centre_x, variant):
    """The street network: list of dicts (pts N x 2, half width m, kind, surface)."""
    streets = []
    def add(pts, half, kind, surface):
        streets.append(dict(pts=np.asarray(pts, dtype=np.float64), half=half, kind=kind, surface=surface))
    reach = BRIDGE_HALF_L+RAMP_M
    # The main street over the bridge, a gentle S on each side beyond the squares.
    for s in (-1, 1):
        sq = np.array([cx+s*(reach+SQUARE_R), cz])
        far = np.array([cx+s*700.0, cz+s*35.0])
        a = np.array([cx+s*(RIVER_HALF_W+BANK_RUN), cz])
        add(a+(sq-a)*np.linspace(0, 1, int(np.linalg.norm(sq-a)/4.0)+2)[:, None], 6.0, 'main', 'cobble')
        add(bezier(sq, sq+np.array([s*160.0, -s*30.0]), far), 6.0, 'main', 'cobble')
    # Quays along each bank, north and south of the bridge's abutment.
    zs = np.arange(cz-320.0, cz+321.0, 6.0)
    for s in (-1, 1):
        off = s*(RIVER_HALF_W+BANK_RUN+9.0)
        for side in (-1, 1):
            z = zs[(zs-cz)*side >= BRIDGE_HALF_W+16.0]
            add(np.column_stack([centre_x(z)+off, z]), 5.0, 'quay', 'cobble')
    # Lanes fanning out of each square at irregular angles, bending as they go.
    fans = {-1: [(38, 300, 0.30), (-52, 280, -0.25), (104, 230, 0.35), (-118, 250, -0.30)],
            1: [(28, 320, -0.28), (-40, 300, 0.32), (114, 240, -0.30), (-96, 260, 0.25)]}
    for s in (-1, 1):
        sq = np.array([cx+s*(reach+SQUARE_R), cz])
        for k, (deg, length, bend) in enumerate(fans[s]):
            a = math.radians(deg)
            out = np.array([s*math.cos(a), math.sin(a)])
            normal = np.array([-out[1], out[0]])
            start = sq+out*SQUARE_R
            mid = start+out*length*0.5+normal*length*bend*0.5
            end = start+out*length
            add(bezier(start, mid, end), 4.5, 'lane', 'cobble' if k < 2 else 'dirt')
    # Alleys from the quays outward, curving to meet the fan.
    if variant['alleys']:
        for s in (-1, 1):
            for zc, bend in ((cz-175.0, 0.4), (cz+165.0, -0.35), (cz-275.0, -0.3), (cz+270.0, 0.3)):
                start = np.array([float(centre_x(np.array([zc]))[0])+s*(RIVER_HALF_W+BANK_RUN+14.0), zc])
                end = start+np.array([s*230.0, (zc-cz)*0.25])
                mid = (start+end)/2+np.array([0.0, 60.0*bend])
                add(bezier(start, mid, end), 3.5, 'alley', 'dirt')
    return streets


def street_samples(streets):
    pts = np.vstack([s['pts'] for s in streets])
    half = np.concatenate([np.full(len(s['pts']), s['half']) for s in streets])
    return pts, half


def rect_hits_streets(rect, pts, half, margin=0.4):
    x0, x1, z0, z1 = rect
    m = half+margin
    return bool(np.any((pts[:, 0] > x0-m) & (pts[:, 0] < x1+m) & (pts[:, 1] > z0-m) & (pts[:, 1] < z1+m)))


# ── houses ───────────────────────────────────────────────────────────────────────────────────────
def pick(key, table):
    r = rnd(key, 7)
    acc = 0.0
    for name, p in table:
        acc += p
        if r < acc:
            return name
    return table[-1][0]


REJECT = {}


def build_town(cx, cz, streets, centre_x, variant):
    pts, half = street_samples(streets)
    reach = BRIDGE_HALF_L+RAMP_M
    squares = [np.array([cx+s*(reach+SQUARE_R), cz]) for s in (-1, 1)]
    buildings, rects = [], []
    n = 0
    for si, st in enumerate(streets):
        if st['kind'] == 'main' and np.abs(st['pts'][:, 0]-cx).max() < reach+1:
            continue                                     # the abutment and ramp: no frontage
        p = st['pts']
        seg = np.diff(p, axis=0)
        along = np.concatenate([[0.0], np.cumsum(np.linalg.norm(seg, axis=1))])
        for side in (-1, 1):
            s_m = 3.0
            k = 0
            while s_m < along[-1]-3.0:
                key = f'v2.{si}.{side}.{k}'
                i = int(np.searchsorted(along, s_m))-1
                i = max(0, min(i, len(seg)-1))
                d = seg[i]/max(np.linalg.norm(seg[i]), 1e-6)
                base = p[i]+d*(s_m-along[i])
                normal = np.array([-d[1], d[0]])*side
                dist = float(np.hypot(base[0]-cx, base[1]-cz))
                inner = dist < 260.0
                if inner:
                    typ = pick(key, [('merchant_brick_house', .66), ('apartment_block', .10), ('workshop', .12), ('wooden_house', .12)])
                else:
                    typ = pick(key, [('wooden_house', .68), ('workshop', .20), ('merchant_brick_house', .12)])
                w = {'apartment_block': 24.0, 'merchant_brick_house': [12.0, 16.0, 20.0][int(rnd(key, 1)*3)],
                     'workshop': [12.0, 16.0][int(rnd(key, 1)*2)], 'wooden_house': [8.0, 12.0][int(rnd(key, 1)*2)]}[typ]
                dep = {'apartment_block': 12.0, 'merchant_brick_house': 12.0, 'workshop': 12.0,
                       'wooden_house': [8.0, 12.0][int(rnd(key, 2)*2)]}[typ]
                # The frontage faces the street; the grammar takes quarter turns, so the house is
                # turned to the quarter nearest the street's direction.
                street_deg = math.degrees(math.atan2(d[0], d[1]))
                yaw = (round(street_deg/90.0)*90.0+(90.0 if side > 0 else -90.0)) % 360.0
                along_x = abs(round(street_deg/90.0)) % 2 == 0     # street runs mostly along z
                fw, fd = (dep, w) if along_x else (w, dep)
                # On a slanting street the turned house's front corner reaches toward it by the
                # slant: set the house back by that overhang.
                dev = math.radians(abs(((street_deg+45.0) % 90.0)-45.0))
                frontage = w if along_x else dep
                overhang = 0.5*frontage*math.sin(dev)+0.5*(fw if along_x else fd)*(1-math.cos(dev))
                setback = st['half']+0.8+overhang+(2.5*rnd(key, 3) if not inner else 0.6*rnd(key, 3))
                c = base+normal*(setback+(fw if along_x else fd)*0.5)
                rect = cc.footprint(float(c[0]), float(c[1]), w, dep, yaw)
                why = ('street' if rect_hits_streets(rect, pts, half) else
                       'house' if not all(cc.rect_gap(rect, r) >= 0.0 for r in rects) else
                       'square' if not all(np.hypot(*(np.array([(rect[0]+rect[1])/2, (rect[2]+rect[3])/2])-q)) > SQUARE_R+max(w, dep)*0.5+2.0 for q in squares) else
                       'river' if not abs(float(c[0])-float(centre_x(np.array([c[1]]))[0])) > RIVER_HALF_W+BANK_RUN+12.0+max(w, dep)*0.5 else
                       'bridge' if (abs(c[1]-cz) < BRIDGE_HALF_W+14.0+dep and abs(c[0]-cx) < reach+10.0) else 'edge' if not (abs(c[0]) < CROP_HALF-60 and abs(c[1]) < CROP_HALF-60) else 'ok')
                REJECT[(st['kind'], why)] = REJECT.get((st['kind'], why), 0)+1
                ok = (not rect_hits_streets(rect, pts, half)
                      and all(cc.rect_gap(rect, r) >= 0.0 for r in rects)
                      and all(np.hypot(*(np.array([(rect[0]+rect[1])/2, (rect[2]+rect[3])/2])-q)) > SQUARE_R+max(w, dep)*0.5+2.0 for q in squares)
                      and abs(float(c[0])-float(centre_x(np.array([c[1]]))[0])) > RIVER_HALF_W+BANK_RUN+12.0+max(w, dep)*0.5
                      and not (abs(c[1]-cz) < BRIDGE_HALF_W+14.0+dep and abs(c[0]-cx) < reach+10.0)
                      and abs(c[0]) < CROP_HALF-60 and abs(c[1]) < CROP_HALF-60)
                empty_lot = rnd(key, 6) < (0.10 if inner else 0.30)
                if ok and not empty_lot:
                    n += 1
                    storeys = {'apartment_block': 3+int(rnd(key, 4)*2), 'merchant_brick_house': 1+int(rnd(key, 4)*3),
                               'workshop': 1, 'wooden_house': 1}[typ]
                    cellar = typ in ('merchant_brick_house', 'apartment_block') and rnd(key, 5) < .5
                    bank = 'w' if c[0] < cx else 'e'
                    buildings.append(dict(id=f'canyon.hamlet.bridge2_{bank}_{n:03d}', typology=typ,
                        seed=int(cc.uid.fnv1a32(key)), width_m=w, depth_m=dep, storeys=storeys, cellar=bool(cellar),
                        condition=pick(key+'c', CONDITIONS), x=cc.r3(float(c[0])), z=cc.r3(float(c[1])), yaw_deg=yaw))
                    rects.append(rect)
                gap = variant['lot_gap'] if inner else variant['outer_gap']
                s_m += max(w, dep)+gap[0]+(gap[1]-gap[0])*rnd(key, 8)
                k += 1
    return buildings, rects, squares


# ── terrain ──────────────────────────────────────────────────────────────────────────────────────
def grade_bridge(H, axis, cx, cz, deck_y):
    """Earth abutments at deck height from the cut bank to the deck's end, ramps down beyond, with
    embankment shoulders at 1 in 1.5 either side of the carriageway."""
    X, Z = np.meshgrid(axis, axis)
    ax = np.abs(X-cx)
    dz = np.abs(Z-cz)
    lane = BRIDGE_HALF_W+2.0
    top = np.full_like(H, -1e9)
    seat = (ax >= RIVER_HALF_W+BANK_RUN) & (ax <= BRIDGE_HALF_L)
    top[seat] = deck_y-0.05
    ramp = (ax > BRIDGE_HALF_L) & (ax < BRIDGE_HALF_L+RAMP_M)
    t = (ax-BRIDGE_HALF_L)/RAMP_M
    top = np.where(ramp, (deck_y-0.05)*(1-t)+H*t, top)
    shoulder = top-np.maximum(dz-lane, 0.0)/1.5
    return np.where((seat | ramp) & (shoulder > H), shoulder, H)


def crater(H, axis, x, z, r, depth):
    sp_ = axis[1]-axis[0]
    i0, i1 = np.searchsorted(axis, [x-r*1.6-sp_, x+r*1.6+sp_])
    j0, j1 = np.searchsorted(axis, [z-r*1.6-sp_, z+r*1.6+sp_])
    X, Z = np.meshgrid(axis[i0:i1], axis[j0:j1])
    q = np.hypot(X-x, Z-z)/r
    bowl = -depth*np.clip(1-q*q, 0, 1)
    rim = 0.25*depth*np.exp(-((q-1.15)/0.22)**2)
    H[j0:j1, i0:i1] += bowl+rim


# ── placements ───────────────────────────────────────────────────────────────────────────────────
class Placer:
    def __init__(self, manifest, H, grid):
        self.m, self.H, self.grid = manifest, H, grid
        self.index = {}
        self.count = {}

    def asset(self, path):
        assets = self.m['assets']
        if path not in assets:
            assets.append(path)
        return assets.index(path)

    def put(self, name, x, z, yaw=0.0, scale=1.0, sink=0.0, tag=None, path=None):
        path = path or ASSET[name]
        if not (ASSET_ROOT/path).exists():
            return False
        y = self.grid.sample(self.H, x, z)-sink
        self.m['placements'].append([self.asset(path), cc.r3(x), cc.r3(y), cc.r3(z), round(yaw % 360.0, 2),
                                     scale if isinstance(scale, list) else cc.r3(scale)])
        self.count[tag or name] = self.count.get(tag or name, 0)+1
        return True


def dress(P, buildings, streets, squares, cx, cz, centre_x, variant, deck_y):
    """The town's ruins and the fighting's traces, the kerbs and the trees."""
    pts, half = street_samples(streets)
    rects = [cc.footprint(b['x'], b['z'], b['width_m'], b['depth_m'], b['yaw_deg']) for b in buildings]
    def free(x, z, r=1.0):
        if rect_hits_streets((x-r, x+r, z-r, z+r), pts, half, margin=-0.5):
            return False
        return all(not (x0-r < x < x1+r and z0-r < z < z1+r) for x0, x1, z0, z1 in rects)
    def in_river(x, z, m=6.0):
        return abs(x-float(centre_x(np.array([z]))[0])) < RIVER_HALF_W+BANK_RUN+m
    dens = variant['dressing']
    # Rubble, bricks and charred timber round every damaged house.
    for b, (x0, x1, z0, z1) in zip(buildings, rects):
        if b['condition'] == 'intact':
            continue
        n = int((3+5*rnd(b['id'], 1))*dens+0.5)
        for k in range(n):
            key = f"{b['id']}.d{k}"
            side = int(rnd(key, 1)*4)
            t = rnd(key, 2)
            out = 1.0+3.5*rnd(key, 3)
            x, z = [(x0+(x1-x0)*t, z0-out), (x0+(x1-x0)*t, z1+out), (x0-out, z0+(z1-z0)*t), (x1+out, z0+(z1-z0)*t)][side]
            if in_river(x, z):
                continue
            burned = b['condition'] == 'burned'
            name = pick(key, [('charred', .45), ('debris_bricks', .2), ('plank', .15), ('brick', .2)] if burned else
                        [('debris_bricks', .3), ('debris_bricks2', .2), ('bricks_stacked', .15), ('gravel', .15), ('board', .1), ('brick', .1)])
            P.put(name, x, z, rnd(key, 4)*360, 0.8+0.6*rnd(key, 5), tag='rubble')
        if b['condition'] in ('burned', 'shelled') and rnd(b['id'], 9) < 0.6*dens:
            P.put('scorch', (x0+x1)/2+(x1-x0)*0.6, (z0+z1)/2, rnd(b['id'], 10)*360, 1.6+rnd(b['id'], 11), sink=-0.02, tag='scorch')
    # Kerbstones along the main street and the quays (playtest density).
    if variant['kerbs']:
        for st in streets:
            if st['kind'] not in ('main', 'quay'):
                continue
            p = st['pts']
            seg = np.diff(p, axis=0)
            along = np.concatenate([[0.0], np.cumsum(np.linalg.norm(seg, axis=1))])
            s_m = 0.0
            while s_m < along[-1]:
                i = max(0, min(int(np.searchsorted(along, s_m))-1, len(seg)-1))
                d = seg[i]/max(np.linalg.norm(seg[i]), 1e-6)
                base = p[i]+d*(s_m-along[i])
                yaw = math.degrees(math.atan2(d[0], d[1]))
                for side in (-1, 1):
                    q = base+np.array([-d[1], d[0]])*side*(st['half']+0.12)
                    if abs(q[1]-cz) < BRIDGE_HALF_W+3 and abs(q[0]-cx) < BRIDGE_HALF_L+RAMP_M:
                        continue
                    if not in_river(q[0], q[1], 1.0):
                        P.put('kerb', float(q[0]), float(q[1]), yaw, [0.42, 0.14, 1.0], sink=0.05, tag='kerb')
                s_m += 2.2
    # The east bridgehead was the defence: sandbags, barricades in the lanes' mouths, hedgehogs.
    east_sq = squares[1]
    for k in range(int(6*max(dens, 0.5))):
        a = math.radians(-70+140*k/5)
        x, z = east_sq[0]-math.cos(a)*(SQUARE_R-4), east_sq[1]+math.sin(a)*(SQUARE_R-4)
        P.put('sandbags', float(x), float(z), math.degrees(a)+90, 1.0, tag='defence')
    for k, st in enumerate(s for s in streets if s['kind'] == 'lane'):
        if rnd(f'barr{k}', 1) < 0.6:
            q = st['pts'][min(4, len(st['pts'])-1)]
            d = st['pts'][min(5, len(st['pts'])-1)]-st['pts'][min(3, len(st['pts'])-2)]
            yaw = math.degrees(math.atan2(d[0], d[1]))+90
            P.put('barricade_a' if k % 2 else 'barricade_b', float(q[0]), float(q[1]), yaw, 1.0, tag='defence')
    for k in range(int(10*dens)+3):
        key = f'hedge{k}'
        x = east_sq[0]+SQUARE_R+10+40*rnd(key, 1)
        z = cz+(rnd(key, 2)-0.5)*60
        if free(x, z, 1.5):
            P.put('hedgehog', x, z, rnd(key, 3)*360, 1.0, tag='defence')
    # Wrecks and the dead along the main street and in the squares.
    wrecks = [('wreck_kv1', 0.62), ('wreck_pz3', 0.30), ('truck_burned', 0.80), ('truck_hit', 0.18),
              ('wagon_wreck', 0.86), ('gun_wreck', 0.24), ('wreck_kv1', 0.70)]
    main = [s for s in streets if s['kind'] == 'main']
    route_pts = np.vstack([m['pts'] for m in main])
    order = np.argsort(route_pts[:, 0])
    route_pts = route_pts[order]
    for k, (name, f) in enumerate(wrecks[:max(3, int(len(wrecks)*dens+0.5))]):
        q = route_pts[int(f*(len(route_pts)-1))]
        off = (8.0+4*rnd(f'wr{k}', 1))*(1 if k % 2 else -1)
        x, z = float(q[0]), float(q[1])+off
        if abs(x-cx) < BRIDGE_HALF_L+RAMP_M+10 or in_river(x, z, 20):
            continue
        P.put(name, x, z, rnd(f'wr{k}', 2)*40-20+(0 if k % 2 else 180), 1.0, tag='wreck')
        if name == 'wagon_wreck':
            P.put('horse_dead', x+3.5, z+2.0, rnd(f'wr{k}', 3)*360, 1.0, tag='wreck')
    dead = ['dead_back', 'dead_down', 'dead_side', 'dead_curled']
    for k in range(int(26*dens)+6):
        key = f'dead{k}'
        q = route_pts[int((0.25+0.6*rnd(key, 1))*(len(route_pts)-1))] if rnd(key, 6) < 0.6 else \
            squares[int(rnd(key, 7)*2)]+np.array([(rnd(key, 8)-0.5)*40, (rnd(key, 9)-0.5)*40])
        x, z = float(q[0])+(rnd(key, 2)-0.5)*18, float(q[1])+(rnd(key, 3)-0.5)*18
        if in_river(x, z, 8) or (abs(z-cz) < BRIDGE_HALF_W+4 and abs(x-cx) < BRIDGE_HALF_L+RAMP_M):
            continue
        if free(x, z, 0.6) or rnd(key, 10) < 0.5:
            P.put(dead[int(rnd(key, 4)*4)], x, z, rnd(key, 5)*360, 1.0, tag='dead')
            if rnd(key, 11) < 0.35:
                P.put('helmet', x+1.2, z-0.6, rnd(key, 12)*360, 1.0, tag='dead')
    for k in range(int(8*dens)+2):
        key = f'grave{k}'
        q = squares[0]+np.array([-(SQUARE_R+8+10*rnd(key, 1)), (k-4)*2.4])
        P.put('rifle_grave' if k % 3 else 'grave_cross', float(q[0]), float(q[1]), 90.0, 1.0, tag='graves')
    for k in range(int(12*dens)+2):
        key = f'kit{k}'
        q = route_pts[int(rnd(key, 1)*(len(route_pts)-1))]
        x, z = float(q[0])+(rnd(key, 2)-0.5)*20, float(q[1])+(rnd(key, 3)-0.5)*20
        if not in_river(x, z, 6):
            P.put(pick(key, [('ammo_burst', .25), ('drum_burst', .2), ('crate', .2), ('barrel', .15), ('metal_barrel', .1), ('stretcher', .1)]),
                  x, z, rnd(key, 4)*360, 1.0, tag='kit')
    # Benches on the west square, garden fences behind the outer timber houses.
    for k in range(3):
        a = math.radians(120+60*k)
        P.put('bench', float(squares[0][0]+math.cos(a)*(SQUARE_R-3)), float(squares[0][1]+math.sin(a)*(SQUARE_R-3)),
              math.degrees(a)+90, 1.0, tag='furniture')
    for b, (x0, x1, z0, z1) in zip(buildings, rects):
        if b['typology'] != 'wooden_house' or rnd(b['id'], 20) > 0.5*dens+0.2:
            continue
        back_z = z1+6.0 if b['yaw_deg'] in (0.0, 360.0) else z0-6.0
        for t in (0.0, 4.0):
            x = x0+t+2.0
            if free(x, back_z, 1.0):
                P.put('fence_a' if rnd(b['id'], 21) < 0.5 else 'fence_b', x, back_z, 90.0, 1.0, tag='fences')
    # Trees: street trees on the outer lanes, the river's banks, gardens.
    big = [NATP+t+'.glb' for t in BIG_TREES]
    small = [NATP+t+'.glb' for t in SMALL_TREES]
    def tree(key, x, z, big_tree=True):
        if in_river(x, z, 4.0) or not free(x, z, 2.5):
            return
        if abs(z-cz) < BRIDGE_HALF_W+8 and abs(x-cx) < BRIDGE_HALF_L+RAMP_M:
            return
        pool = big if big_tree else small
        P.put(None, x, z, rnd(key, 1)*360, 1.3+0.9*rnd(key, 2), sink=0.05, tag='town_trees', path=pool[int(rnd(key, 3)*len(pool))])
    for si, st in enumerate(streets):
        if st['kind'] in ('lane', 'alley'):
            p = st['pts']
            for k in range(0, len(p), max(int(variant['street_tree_m']/4.0), 1)):
                for side in (-1, 1):
                    if k+1 >= len(p):
                        continue
                    d = (p[k+1]-p[k])/max(np.linalg.norm(p[k+1]-p[k]), 1e-6)
                    q = p[k]+np.array([-d[1], d[0]])*side*(st['half']+1.6)
                    if np.hypot(q[0]-cx, q[1]-cz) > 200 and rnd(f'st{si}.{k}.{side}', 4) < 0.7:
                        tree(f'st{si}.{k}.{side}', float(q[0]), float(q[1]))
    for side in (-1, 1):
        for z in np.arange(-CROP_HALF+40, CROP_HALF-40, 9.0 if dens > 0.5 else 20.0):
            key = f'bank{side}.{int(z)}'
            off = RIVER_HALF_W+BANK_RUN+3.0+3.0*rnd(key, 5)
            if abs(z-cz) < 330 and dens > 0.5:
                off = RIVER_HALF_W+BANK_RUN+4.5          # between the quay and the water in town
            x = float(centre_x(np.array([z]))[0])+side*off
            if rnd(key, 6) < 0.75:
                tree(key, x, float(z), big_tree=rnd(key, 7) < 0.8)
    for b, (x0, x1, z0, z1) in zip(buildings, rects):
        if b['typology'] == 'wooden_house' and rnd(b['id'], 30) < 0.8*dens+0.2:
            for k in range(1+int(rnd(b['id'], 31)*3)):
                key = f"{b['id']}.g{k}"
                tree(key, (x0+x1)/2+(rnd(key, 4)-0.5)*16, (z1 if rnd(key, 5) < .5 else z0)+(rnd(key, 6)-0.5)*20, big_tree=rnd(key, 7) < 0.5)


def ground_paint(colour, grass, axis_tex, streets, rects, squares, centre_tex, cx, cz):
    """Streets (cobbles in the centre, dirt lanes), squares, yards; the river and its banks."""
    d_tex = axis_tex[None, :]-centre_tex[:, None]
    ad = np.abs(d_tex)
    # Inside the town the strip fields give way to gardens and meadow between the houses.
    rr = np.hypot(axis_tex[None, :]-cx, axis_tex[:, None]-cz)
    town = np.clip((560.0-rr)/90.0, 0, 1)
    town = (town*town*(3-2*town))[..., None]
    mottle = (0.5+0.5*np.sin(axis_tex[None, :]*0.21)*np.cos(axis_tex[:, None]*0.17)).astype(np.float32)[..., None]
    garden = np.array([0.24, 0.36, 0.12], dtype=np.float32)*(0.85+0.3*mottle)
    colour[:] = colour*(1-town)+garden*town
    grass[:] = np.maximum(grass, 0.75*town[..., 0])
    water = ad < RIVER_HALF_W
    bank = (ad >= RIVER_HALF_W) & (ad < RIVER_HALF_W+BANK_RUN+2.0)
    colour[water] = [0.10, 0.12, 0.11]
    colour[bank] = [0.32, 0.27, 0.20]
    grass[water | bank] = 0.0
    shore = np.clip(1.0-(ad-RIVER_HALF_W-BANK_RUN)/40.0, 0, 1)[..., None]*0.4
    colour[:] = colour*(1-shore)+np.array([0.20, 0.30, 0.11])*shore
    to_px = (TEX-1)/EXTENT
    fine = (np.sin(axis_tex[None, :]*3.1)*np.cos(axis_tex[:, None]*2.7)*0.5+0.5).astype(np.float32)
    def stamp(x, z, r, tint, mix=1.0):
        i0, i1 = int((x-r+EXTENT/2)*to_px), int((x+r+EXTENT/2)*to_px)+2
        j0, j1 = int((z-r+EXTENT/2)*to_px), int((z+r+EXTENT/2)*to_px)+2
        i0, j0 = max(i0, 0), max(j0, 0)
        i1, j1 = min(i1, TEX), min(j1, TEX)
        if i1 <= i0 or j1 <= j0:
            return
        X, Z = np.meshgrid(axis_tex[i0:i1], axis_tex[j0:j1])
        inside = np.clip(1.0-(np.hypot(X-x, Z-z)-r)/0.8, 0, 1)*mix
        t = np.asarray(tint, dtype=np.float32)*(0.88+0.24*fine[j0:j1, i0:i1, None])
        colour[j0:j1, i0:i1] = colour[j0:j1, i0:i1]*(1-inside[..., None])+t*inside[..., None]
        grass[j0:j1, i0:i1] *= 1-inside
    for st in streets:
        tint = [0.36, 0.35, 0.33] if st['surface'] == 'cobble' else [0.38, 0.31, 0.22]
        for q in st['pts'][::1]:
            stamp(float(q[0]), float(q[1]), st['half'], tint)
    for q in squares:
        stamp(float(q[0]), float(q[1]), SQUARE_R, [0.37, 0.36, 0.34])
    for x0, x1, z0, z1 in rects:
        stamp((x0+x1)/2, (z0+z1)/2, max(x1-x0, z1-z0)*0.5+4.0, [0.33, 0.28, 0.21], 0.7)


# ── the map ──────────────────────────────────────────────────────────────────────────────────────
def build(variant_name='playtest', carry_from=None, plan=None):
    variant = VARIANTS[variant_name]
    dest = WORKSHOPS/variant['dest']
    rng = np.random.default_rng(v1.SEED)
    H0, ravine, _ = sp.heights(rng)
    sp.SOURCE = ASSET_ROOT/'assets/terrain/spring_bridge_v2/source'
    source = sp.write_source(H0)
    gaea.ANCHORS = v1.ANCHORS
    cc.GROUPS = v1.GROUPS
    cc.CRATERS = variant['craters']
    cc.RAZED_PLOTS = 4
    cc.ASSETS['tree_tall'] = NATP+'tree_5.glb'
    cc.ASSETS['tree_small'] = NATP+'tree_7.glb'
    manifest, edits, H, receipt = gaea.build(source)
    drifts = {i for i, p in enumerate(manifest['assets']) if 'snow_drift' in p}
    manifest['placements'] = [p for p in manifest['placements'] if p[0] not in drifts]
    route = np.array(edits['canyon_course']['route'], dtype=np.float64)
    k = int(np.argmin(np.hypot(route[:, 0], route[:, 1])))
    cx, cz = float(np.round(route[k, 0])), float(np.round(route[k, 1]))
    # The river (v1's carving with this version's river).
    for name in ('RIVER_HALF_W', 'BANK_RUN', 'BED_DEPTH', 'BANK_ABOVE', 'FLOOD_HALF', 'VALLEY_RUN'):
        setattr(v1, name, globals()[name])
    H_before = H.copy()
    H, d, centre, water, axis = v1.carve(H, cx, cz)
    centre_x = lambda z: np.interp(z, axis, centre)
    row = int(np.argmin(np.abs(axis-cz)))
    deck_y = float(water[row]+DECK_ABOVE_WATER)
    streets = lay_streets(cx, cz, centre_x, variant)
    town, town_rects, squares = build_town(cx, cz, streets, centre_x, variant)
    print('[spring-bridge-v2] lots by street kind and outcome:', {f'{k}/{w}': n for (k, w), n in sorted(REJECT.items())})
    # Clear the valley and the town of the grammar's houses and dressing.
    town_r = 520.0
    def in_town(x, z, m=0.0):
        return math.hypot(x-cx, z-cz) < town_r+m
    def in_river(x, z, m):
        return abs(x-float(centre_x(z))) < RIVER_HALF_W+BANK_RUN+m
    kept = [b for b in edits['building_instances']
            if not in_town(b['x'], b['z'], 30.0) and not in_river(b['x'], b['z'], 60.0)
            and abs(b['x']) < CROP_HALF-60 and abs(b['z']) < CROP_HALF-60]
    # Pads: streets level across, then the houses' pads, then the bridge, then the channel again.
    X, Z = np.meshgrid(axis, axis)
    for st in streets:
        for q in st['pts'][::3]:
            v1.pad(H, axis, (q[0]-st['half'], q[0]+st['half'], q[1]-st['half'], q[1]+st['half']), 2.0)
    for b in kept:
        v1.pad(H, axis, cc.footprint(b['x'], b['z'], b['width_m'], b['depth_m'], b['yaw_deg']))
    for r in town_rects:
        v1.pad(H, axis, r, 3.0)
    for q in squares:
        v1.pad(H, axis, (q[0]-SQUARE_R, q[0]+SQUARE_R, q[1]-SQUARE_R, q[1]+SQUARE_R), 6.0)
    # Craters in the town's streets and yards (shellfire), clear of the bridge and the houses.
    pts, half = street_samples(streets)
    for k in range(variant['craters']):
        key = f'tc{k}'
        q = pts[int(rnd(key, 1)*(len(pts)-1))]
        x, z = float(q[0])+(rnd(key, 2)-0.5)*14, float(q[1])+(rnd(key, 3)-0.5)*14
        if in_river(x, z, 10) or (abs(z-cz) < BRIDGE_HALF_W+10 and abs(x-cx) < BRIDGE_HALF_L+RAMP_M+10):
            continue
        if any(x0-5 < x < x1+5 and z0-5 < z < z1+5 for x0, x1, z0, z1 in town_rects):
            continue
        crater(H, axis, x, z, 2.2+2.0*rnd(key, 4), 0.7+0.6*rnd(key, 5))
    H = grade_bridge(H, axis, cx, cz, deck_y)
    ad = np.abs(X-centre[:, None])
    wall = (water[:, None]-BED_DEPTH)+(BANK_ABOVE+BED_DEPTH)*np.clip((ad-RIVER_HALF_W)/BANK_RUN, 0, 1)
    H = np.where(ad < RIVER_HALF_W+BANK_RUN, np.minimum(H, wall), H)
    edits['building_instances'] = kept+town
    grid = gaea.SurfaceGrid(RES, EXTENT/(RES-1))
    # Placements: the grammar's, reseated; none in the town, the river or off the map.
    placed = []
    for p in manifest['placements']:
        x, z = p[1], p[3]
        if in_town(x, z, 10.0) or in_river(x, z, 12.0) or abs(x) > CROP_HALF-5 or abs(z) > CROP_HALF-5:
            continue
        p = list(p)
        p[2] = cc.r3(p[2]+grid.sample(H, x, z)-grid.sample(H_before, x, z))
        placed.append(p)
    manifest['placements'] = placed
    P = Placer(manifest, H, grid)
    dress(P, town, streets, squares, cx, cz, centre_x, variant, deck_y)
    # Woods over the whole square, many more than the steppe's.
    sp.MAX_WOOD_TREES = variant['trees']
    sp.WOOD_REACH_M = 4000.0
    sp.WOOD_TREES = ['tree_5', 'tree_6', 'tree_7', 'tree_8', 'tree_1', 'tree_9', 'tree_7', 'pine_tree_n_1', 'pine_tree_n_2']
    route = v1.straighten_route(route, cx, cz)
    buildings = edits['building_instances']
    woods = sp.wood_mask(rng, H, ravine, route, buildings)
    woods &= ~((np.abs(X-centre[:, None]) < FLOOD_HALF*0.6) | (np.hypot(X-cx, Z-cz) < town_r+40))
    woods &= (np.abs(X) < CROP_HALF-20) & (np.abs(Z) < CROP_HALF-20)
    trees = sp.plant_woods(manifest, H, woods)
    for p in manifest['placements'][-trees:]:
        p[5] = cc.r3(p[5]*1.45)                   # mature trees: the library's are saplings
    # The course: route, start, goal, encounter, all on the cropped square.
    course = edits['canyon_course']
    inside = (np.abs(route[:, 0]) < CROP_HALF-60) & (np.abs(route[:, 1]) < CROP_HALF-60)
    idx = np.where(inside)[0]
    route = route[idx.min():idx.max()+1]
    course['route'] = [[cc.r3(x), cc.r3(z)] for x, z in route]
    seg = np.hypot(*np.diff(route, axis=0).T)
    course['length_m'] = cc.r3(float(seg.sum()))
    start_x = cx-town_r-260.0
    s = int(np.argmin(np.abs(route[:, 0]-start_x)+1e3*(route[:, 0] > cx)))
    heading = math.degrees(math.atan2(*(route[min(s+3, len(route)-1)]-route[s])))
    course['start'] = dict(x=cc.r3(route[s, 0]), z=cc.r3(route[s, 1]), heading_deg=cc.r3(heading))
    course['goal'] = dict(x=cc.r3(route[-1, 0]), z=cc.r3(route[-1, 1]), radius_m=85.0)
    street = edits['street_encounter']
    remap, sites = {}, []
    for i, g in enumerate(street['gun_sites']):
        if not (in_town(g['x'], g['z'], 20.0) or in_river(g['x'], g['z'], 15.0)) and abs(g['x']) < CROP_HALF-40 and abs(g['z']) < CROP_HALF-40:
            remap[i] = len(sites)
            sites.append(g)
    street['gun_sites'] = sites
    zones = []
    for zone in course['gun_zones']:
        zone['candidates'] = [remap[i] for i in zone['candidates'] if i in remap]
        if zone['candidates']:
            zones.append(zone)
    course['gun_zones'] = zones
    street['kill_zone'] = [kz for kz in street['kill_zone'] if kz['id'] in {f"route.{z['id']}" for z in zones} or kz['id'] == 'route.whole']
    for kz in street['kill_zone']:
        if kz['id'] == 'route.whole':
            kz['points'] = [[cc.r3(x), cc.r3(z)] for x, z in route[::12]]
        else:
            kz['points'] = [q for q in kz['points'] if abs(q[0]) < CROP_HALF and abs(q[1]) < CROP_HALF]
    def loop(sgn):
        # The square and the quay on each bank: the garrison platoons' streets.
        sq = squares[0 if sgn < 0 else 1]
        quay_x = cx+sgn*(RIVER_HALF_W+BANK_RUN+9.0)
        corners = [(quay_x, cz-200), (quay_x, cz-30), (float(sq[0]), cz-20), (float(sq[0]), cz+20), (quay_x, cz+30),
                   (quay_x, cz+200), (float(sq[0])+sgn*120, cz+120), (float(sq[0])+sgn*120, cz-120)]
        pts_ = []
        for (ax_, az), (bx, bz) in zip(corners, corners[1:]+corners[:1]):
            n = max(int(math.hypot(bx-ax_, bz-az)/15.0), 1)
            pts_ += [[cc.r3(ax_+(bx-ax_)*k/n), cc.r3(az+(bz-az)*k/n)] for k in range(n)]
        return pts_
    street['beats'] = [dict(platoon='A', points=loop(-1)), dict(platoon='B', points=loop(1))]
    cors = [dict(a=[cc.r3(v) for v in a], b=[cc.r3(v) for v in b],
                 width_m=2*BRIDGE_HALF_W if (abs(a[0]-cx) < BRIDGE_HALF_L+20 or abs(b[0]-cx) < BRIDGE_HALF_L+20) else 40.0)
            for a, b in zip(route[::30], route[30::30])]
    street['ways']['corridors'] = cors
    street['ways']['district'] = [-CROP_HALF, CROP_HALF, -CROP_HALF, CROP_HALF]
    sx, sz = course['start']['x'], course['start']['z']
    street['ways']['rects'] = [[cc.r3(sx-40), cc.r3(sx+40), cc.r3(sz-40), cc.r3(sz+40)]]
    posts = []
    for b in town:
        if b['typology'] in ('merchant_brick_house', 'apartment_block') and abs(b['x']-cx) < 320:
            x0, x1, z0, z1 = cc.footprint(b['x'], b['z'], b['width_m'], b['depth_m'], b['yaw_deg'])
            posts.append(dict(x=cc.r3(x0-2), z=cc.r3(z0-2), heading_deg=270.0 if b['x'] > cx else 90.0, host=b['id']))
    course['garrison'] = posts[:24]
    course.update(variant=variant['dest'], tool='tools/worldgen/spring_bridge_v2.py', finish='mission', director=False,
                  opening_platoons=False, headline='The bridge is held', camp=True)
    course['provenance'] = dict(
        terrain='Generated spring steppe with a river valley; not a survey of real ground.',
        buildings=cc.PROVENANCE['buildings']+' A river town on organic streets on both banks, most of it in ruins.',
        defence=cc.PROVENANCE['defence'],
        warzone=('Attested recurring conditions of the 1943-44 river crossings on the Eastern Front (Dnieper, Desna, '
                 'Bug): bridgehead towns fought through street by street, sandbagged and barricaded bridgeheads, '
                 'anti-tank obstacles, abandoned transport and horse-drawn trains, the dead in the streets and field '
                 'graves marked with a rifle or a cross. Not a reconstruction of a named town or engagement.'),
        scope='Generative test map for a bridge assault and its defence.')
    # Markers, river and crossing records.
    for m in manifest['markers']:
        if m['kind'] == 'Spawn':
            m.update(x=course['start']['x'], z=course['start']['z'], y=cc.r3(grid.sample(H, sx, sz)))
        if m['kind'] == 'Goal':
            m.update(x=course['goal']['x'], z=course['goal']['z'], y=cc.r3(grid.sample(H, course['goal']['x'], course['goal']['z'])))
    manifest['markers'] = [m for m in manifest['markers'] if m['kind'] in ('Spawn', 'Goal')]
    manifest['markers'].append(dict(name='the_bridge', kind='Bridge', x=cx, z=cz, y=cc.r3(deck_y)))
    lo, hi = int(np.searchsorted(axis, -CROP_HALF-1e-3)), int(np.searchsorted(axis, CROP_HALF+1e-3))
    step = 10
    cl = [[cc.r3(float(centre[i])), cc.r3(float(axis[i])), cc.r3(float(water[i]))] for i in range(lo, hi, step)]
    manifest['river'] = dict(schema='kessel.river', version=1, half_width_m=RIVER_HALF_W, bank_run_m=BANK_RUN,
        bed_depth_m=BED_DEPTH, bank_above_m=BANK_ABOVE, centreline=cl, color=WATER_RGB, roughness=WATER_ROUGHNESS,
        note='centreline points are [x, z, water_y]; the water surface is flat across the channel')
    piers = [[cc.r3(dx), cc.r3(float(water[row]-BED_DEPTH))] for dx in np.linspace(-RIVER_HALF_W+10, RIVER_HALF_W-10, 3)]
    manifest['crossings'] = [dict(center=[cx, cz], deck_y=cc.r3(deck_y), half_length=BRIDGE_HALF_L, half_width=BRIDGE_HALF_W,
                                  deck_thickness_m=1.2, piers=piers, name='the_bridge')]
    # Ground, cropped with everything else.
    colour, grass = sp.ground(rng, H, ravine*(np.abs(X-centre[:, None]) > FLOOD_HALF), route, buildings, woods)
    axis_tex = np.linspace(-EXTENT/2, EXTENT/2, TEX, dtype=np.float32)
    centre_tex = np.interp(axis_tex, axis, centre).astype(np.float32)
    ground_paint(colour, grass, axis_tex, streets, town_rects, squares, centre_tex, cx, cz)
    t0, t1 = int(np.searchsorted(axis_tex, -CROP_HALF)), int(np.searchsorted(axis_tex, CROP_HALF))
    terrain_dir = ASSET_ROOT/f"assets/terrain/{variant['dest'].replace('-', '_')}"
    terrain_dir.mkdir(parents=True, exist_ok=True)
    c = colour[t0:t1, t0:t1]
    rgba = np.dstack([np.uint8(np.rint(np.clip(c, 0, 1)*255)), np.full(c.shape[:2], 32, dtype=np.uint8)])
    Image.fromarray(rgba, 'RGBA').save(terrain_dir/'ground_material.png')
    g = np.uint8(np.rint(np.clip(grass[t0:t1, t0:t1], 0, 1)*255))
    Image.fromarray(np.dstack([g, np.zeros_like(g), np.zeros_like(g)]), 'RGB').save(terrain_dir/'ground_class.png')
    Hc = H[lo:hi, lo:hi]
    manifest['terrain'].update(res=int(Hc.shape[0]), spacing=float(EXTENT/(RES-1)),
        texture=f"assets/terrain/{terrain_dir.name}/ground_material.png", material_encoding='gaea-rock-srgb-snow-linear-alpha32-223-v1',
        ground_class=f"assets/terrain/{terrain_dir.name}/ground_class.png", grass_season='spring', color=[0.9, 0.9, 0.9, 1.0])
    manifest['authoring'].update(tool='tools/worldgen/spring_bridge_v2.py', tool_version=VERSION, variant=variant['dest'])
    manifest['overview'] = dict(pos=[cc.r3(cx+420), 260.0, cc.r3(cz+480)], target=[cx, cc.r3(deck_y), cz])
    edits['editor_home'] = dict(eye=[cx, cc.r3(deck_y+1.8), cz], target=[cc.r3(cx+30.0), cc.r3(deck_y+0.8), cz])
    # A bright, warm afternoon in May, the sun in the south-west, thin high cloud, blue haze.
    edits['environment_lighting'] = dict(enabled=True, fog_density=0.00045, overcast=0.06, storm=0.0,
        schema='kessel.environment-authoring', utc='1943-05-12T14:10:00Z', version=2)
    edits['base_placements'] = len(manifest['placements'])
    edits['base_markers'] = len(manifest['markers'])
    edits['ops'] = []
    dest.mkdir(parents=True, exist_ok=True)
    carried = carry(dest/'manifest_edits.json', carry_from, edits)
    (dest/'manifest.v1.json').write_text(cc.dumps(manifest, indent=None))
    (dest/'manifest_edits.json').write_text(cc.dumps(edits))
    np.save(dest/'heights.npy', Hc.astype(np.float32))
    (dest/'mission.v1.json').write_text(cc.dumps(v1.mission_sheet(cx, cz, route)))
    sheet = json.loads((dest/'mission.v1.json').read_text())
    sheet['beats'][0]['at'] = {'x': cc.r3(cx-town_r+60), 'z': cc.r3(cz)}
    (dest/'mission.v1.json').write_text(cc.dumps(sheet))
    report = dict(schema='panzer.spring-bridge', version=VERSION, variant=variant_name, seed=v1.SEED,
        bridge=dict(x=cx, z=cz, deck_y=deck_y, length_m=2*BRIDGE_HALF_L), town_buildings=len(town),
        conditions={c_: sum(b['condition'] == c_ for b in town) for c_, _ in CONDITIONS},
        outlying_buildings=len(kept), placements=len(manifest['placements']), dressing=P.count, wood_trees=trees,
        start=course['start'], extent_m=2*CROP_HALF, carried=carried,
        outputs={n_: cc.sha256_file(dest/n_) for n_ in cc.OUTPUT_FILES})
    (dest/'spring-bridge.receipt.json').write_text(cc.dumps(report))
    if plan:
        prev = np.uint8(np.rint(np.clip(c[::2, ::2], 0, 1)*255)).copy()
        to_px = (prev.shape[0]-1)/(2*CROP_HALF)
        def dot(x, z, col, r=1):
            px, pz = int((x+CROP_HALF)*to_px), int((z+CROP_HALF)*to_px)
            prev[max(pz-r, 0):pz+r+1, max(px-r, 0):px+r+1] = col
        for b in buildings:
            x0, x1, z0, z1 = cc.footprint(b['x'], b['z'], b['width_m'], b['depth_m'], b['yaw_deg'])
            for xx in np.arange(x0, x1, 2.0):
                for zz in np.arange(z0, z1, 2.0):
                    dot(xx, zz, [200, 90, 40], 0)
        for x, z in route:
            dot(x, z, [255, 255, 255])
        dot(sx, sz, [255, 30, 30], 4)
        Image.fromarray(prev).save(plan)
    print(json.dumps({k_: report[k_] for k_ in ('variant', 'bridge', 'town_buildings', 'conditions', 'placements',
                                                 'dressing', 'wood_trees', 'start', 'carried')}, indent=1))
    return report


def carry(path, carry_from, edits):
    """Editor work carried forward: keys this script does not write, and added objects and markers
    (from this map's previous edits, and from `carry_from`'s, e.g. the v1 map)."""
    sources = [p for p in (path, (Path(carry_from)/'manifest_edits.json') if carry_from else None) if p and p.exists()]
    report = dict(sources=[str(p) for p in sources], added=0, not_carried=0, keys=[])
    for src in sources:
        old = json.loads(src.read_text())
        if src == path:
            for k, v in old.items():
                if k not in edits and k not in ('base_placements', 'base_markers', 'ops'):
                    edits[k] = v
                    report['keys'].append(k)
        for op in old.get('ops', []):
            if op.get('op') in ('add', 'marker_add'):
                if op not in edits['ops']:
                    edits['ops'].append(op)
                    report['added'] += 1
            else:
                report['not_carried'] += 1
    return report


if __name__ == '__main__':
    ap = argparse.ArgumentParser()
    ap.add_argument('--variant', choices=sorted(VARIANTS), default='playtest')
    ap.add_argument('--carry-from', type=Path)
    ap.add_argument('--plan', type=Path)
    a = ap.parse_args()
    build(a.variant, a.carry_from, a.plan)
