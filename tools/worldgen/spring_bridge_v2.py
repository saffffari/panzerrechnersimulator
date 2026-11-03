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
SQUARE_R = 16.0

VARIANTS = {
    'playtest': dict(dest='spring-bridge-v2', houses=14, natural=1.0, rubble=1.0, field_craters=320, battle=1.0, lot_gap=(0.0, 3.0), outer_gap=(4.0, 10.0), dressing=1.0,
                     kerbs=True, trees=16000, street_tree_m=13.0, craters=16, alleys=False, low_clouds=18),
    'render': dict(dest='spring-bridge-lite-v2', houses=10, natural=0.25, rubble=0.4, field_craters=140, battle=0.4, lot_gap=(3.0, 8.0), outer_gap=(10.0, 20.0), dressing=0.35,
                   kerbs=False, trees=5000, street_tree_m=24.0, craters=12, alleys=False, low_clouds=8),
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
    'stone_s': NATP+'stone_5.glb', 'stone_m': NATP+'stone_4.glb', 'stone_l': NATP+'stone_1.glb',
    'boulder_a': NATP+'stone_2.glb', 'boulder_b': NATP+'stone_3.glb',
    'branch': NATP+'tree_log_1.glb', 'stump': NATP+'tree_stump_1.glb',
    'fern_a': NATP+'fern_1.glb', 'fern_b': NATP+'fern_2.glb', 'mushroom': NATP+'mushroom_n_1.glb',
    'scrap': PZ+'Props/scrap_metal_mx_1.glb', 'scrap2': PZ+'Props/scrap_metal_mx_1_1.glb',
    'tire2': PZ+'Props/tire_2.glb', 'plank4': PZ+'Props/wooden_plank_4.glb', 'brick3': PZ+'Debris & Misc/brick_mx_3.glb',
    'cement_bag': PZ+'Props/cement_bag_mp_1.glb', 'pole': PZ+'Props/pipe_mx_1.glb', 'lamp_head': PZ+'Light Sources/lamp_mx_2_on.glb',
    'shattered_a': 'assets/worlds/berlin1945/tree_shattered_a.glb', 'shattered_b': 'assets/worlds/berlin1945/tree_shattered_b.glb',
    'dead_raised': 'assets/warprops/dead/dead_sol_arm_raised.glb', 'dead_slump': 'assets/warprops/dead/dead_sol_seated_slump.glb',
    'canister': 'assets/warprops/supply_canister.glb', 'wire_stake': 'assets/warprops/wire_stake.glb', 'bricks_stacked1': PZ+'Debris & Misc/bricks_stacked_mx_1.glb',
    'charred': cc.ASSETS['charred_plank'], 'scorch': cc.ASSETS['scorch'],
}
# Trees and shrubs: the spring 1943 set (tools/vegetation/spring1943.py), real mature sizes, May
# foliage; the species by place, the way a Ukrainian river town grows them.
VEG = 'assets/vegetation/spring1943/'
STREET_TREES = ['tree_linden_a', 'tree_linden_b', 'tree_chestnut_a', 'tree_chestnut_b']
BANK_TREES = ['tree_willow_a', 'tree_willow_b', 'tree_willow_c', 'tree_willow_c', 'tree_poplar_a', 'tree_poplar_c']
BANK_SCRUB = ['shrub_willowscrub_a', 'shrub_willowscrub_b', 'shrub_elder_a']
ORCHARD = ['tree_apple_a', 'tree_apple_b', 'tree_apple_c', 'tree_cherry_a', 'tree_cherry_b', 'tree_pear_a', 'tree_pear_b']
GARDEN_SHRUBS = ['shrub_elder_b', 'shrub_dogrose_a', 'shrub_dogrose_b', 'shrub_hazel_b']
HEDGE = ['shrub_hawthorn_a', 'shrub_hawthorn_b', 'shrub_blackthorn_a', 'shrub_blackthorn_b', 'shrub_dogrose_a',
         'shrub_elder_a', 'shrub_hazel_a']
WOOD_MIX = [('tree_oak_a', 3), ('tree_oak_b', 2), ('tree_oak_c', 2), ('tree_birch_a', 2), ('tree_birch_b', 2),
            ('tree_birch_c', 2), ('tree_linden_a', 2), ('tree_linden_b', 1), ('tree_poplar_b', 1),
            ('shrub_hazel_a', 2), ('shrub_hazel_b', 2), ('shrub_hawthorn_a', 1), ('shrub_elder_a', 1)]
FALLEN = [('fallen_oak_uprooted_a', None), ('fallen_birch_uprooted_a', None), ('fallen_apple_uprooted_a', None),
          ('fallen_poplar_snapped_a', 'stump_poplar_snapped_a'), ('fallen_willow_snapped_a', 'stump_willow_snapped_a'),
          ('fallen_linden_snapped_a', 'stump_linden_snapped_a'), ('fallen_birch_snapped_a', 'stump_birch_snapped_a')]


def veg_damaged(name, key):
    """The shelled form of a tree or shrub (shattered or burnt, same species where the set has it)."""
    if name.startswith('shrub_'):
        return 'shrub_burnt_a' if 'hazel' in name or 'elder' in name else 'shrub_burnt_b'
    species = name.split('_')[1]
    forms = [f.stem for f in sorted((ASSET_ROOT/VEG).glob(f'tree_{species}_*.glb')) if 'shattered' in f.stem or 'burnt' in f.stem]
    if not forms:
        forms = ['tree_oak_shattered_a', 'tree_poplar_shattered_a', 'tree_linden_shattered_b']
    return forms[int(rnd(key, 41)*len(forms))]


def veg_path(name):
    return VEG+name+'.glb'
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
        add(a+(sq-a)*np.linspace(0, 1, int(np.linalg.norm(sq-a)/4.0)+2)[:, None], 4.5, 'main', 'cobble')
        add(bezier(sq, sq+np.array([s*160.0, -s*30.0]), far), 4.5, 'main', 'dirt')
    # Quays along each bank, north and south of the bridge's abutment.
    zs = np.arange(cz-120.0, cz+121.0, 6.0)
    for s in (-1, 1):
        off = s*(RIVER_HALF_W+BANK_RUN+9.0)
        for side in (-1, 1):
            z = zs[(zs-cz)*side >= BRIDGE_HALF_W+16.0]
            add(np.column_stack([centre_x(z)+off, z]), 3.5, 'quay', 'cobble')
    # Lanes fanning out of each square at irregular angles, bending as they go.
    # Two short lanes out of each bridgehead, becoming field tracks.
    fans = {-1: [(38, 140, 0.30), (-118, 120, -0.30)],
            1: [(28, 150, -0.28), (-96, 120, 0.25)]}
    for s in (-1, 1):
        sq = np.array([cx+s*(reach+SQUARE_R), cz])
        for k, (deg, length, bend) in enumerate(fans[s]):
            a = math.radians(deg)
            out = np.array([s*math.cos(a), math.sin(a)])
            normal = np.array([-out[1], out[0]])
            start = sq+out*SQUARE_R
            mid = start+out*length*0.5+normal*length*bend*0.5
            end = start+out*length
            add(bezier(start, mid, end), 3.0, 'lane', 'dirt')
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
    buildings, rects, candidates = [], [], []
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
                inner = True
                # Small houses: the grammar's smallest brick house (12 x 8, two or three
                # storeys), timber cottages and small workshops.
                typ = pick(key, [('merchant_brick_house', .55), ('wooden_house', .30), ('workshop', .15)])
                w = {'merchant_brick_house': 12.0, 'workshop': 12.0, 'wooden_house': 8.0}[typ]
                dep = 8.0
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
                setback = st['half']+0.4+overhang+0.4*rnd(key, 3)
                c = base+normal*(setback+(fw if along_x else fd)*0.5)
                rect = cc.footprint(float(c[0]), float(c[1]), w, dep, yaw)
                why = ('street' if rect_hits_streets(rect, pts, half) else
                       'house' if not all(cc.rect_gap(rect, r) >= 0.0 for r in rects) else
                       'square' if not all(np.hypot(*(np.array([(rect[0]+rect[1])/2, (rect[2]+rect[3])/2])-q)) > SQUARE_R+max(w, dep)*0.5+2.0 for q in squares) else
                       'river' if not abs(float(c[0])-float(centre_x(np.array([c[1]]))[0])) > RIVER_HALF_W+BANK_RUN+12.0+max(w, dep)*0.5 else
                       'bridge' if (abs(c[1]-cz) < BRIDGE_HALF_W+14.0+dep and abs(c[0]-cx) < reach+10.0) else 'edge' if not (abs(c[0]) < CROP_HALF-60 and abs(c[1]) < CROP_HALF-60) else 'ok')
                REJECT[(st['kind'], why)] = REJECT.get((st['kind'], why), 0)+1
                ok = (not rect_hits_streets(rect, pts, half)
                      and all(np.hypot(*(np.array([(rect[0]+rect[1])/2, (rect[2]+rect[3])/2])-q)) > SQUARE_R+max(w, dep)*0.5+2.0 for q in squares)
                      and abs(float(c[0])-float(centre_x(np.array([c[1]]))[0])) > RIVER_HALF_W+BANK_RUN+12.0+max(w, dep)*0.5
                      and not (abs(c[1]-cz) < BRIDGE_HALF_W+14.0+dep and abs(c[0]-cx) < reach+10.0)
                      and abs(c[0]) < CROP_HALF-60 and abs(c[1]) < CROP_HALF-60)
                if ok:
                    storeys = {'merchant_brick_house': 2+int(rnd(key, 4)*2), 'workshop': 1, 'wooden_house': 1}[typ]
                    cellar = typ == 'merchant_brick_house' and rnd(key, 5) < .5
                    candidates.append((dist, key, rect, dict(typology=typ, seed=int(cc.uid.fnv1a32(key)), width_m=w,
                        depth_m=dep, storeys=storeys, cellar=bool(cellar), condition=pick(key+'c', CONDITIONS),
                        x=cc.r3(float(c[0])), z=cc.r3(float(c[1])), yaw_deg=yaw)))
                s_m += w if along_x else dep              # lot after lot, wall to wall
                k += 1
    # A tiny town: the lots nearest the bridge, packed, until the town has its houses.
    for dist, key, rect, b in sorted(candidates, key=lambda t: t[0]):
        if len(buildings) >= variant['houses']:
            break
        if all(cc.rect_gap(rect, r) >= 0.0 for r in rects):
            n += 1
            bank = 'w' if b['x'] < cx else 'e'
            buildings.append(dict(id=f'canyon.hamlet.bridge2_{bank}_{n:03d}', **b))
            rects.append(rect)
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
    dead = ['dead_down', 'dead_side', 'dead_curled', 'dead_down', 'dead_side']
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
    def tree(key, x, z, pool=STREET_TREES, damage=0.0):
        if in_river(x, z, 4.0) or not free(x, z, 2.5):
            return
        if abs(z-cz) < BRIDGE_HALF_W+8 and abs(x-cx) < BRIDGE_HALF_L+RAMP_M:
            return
        name = pool[int(rnd(key, 3)*len(pool))]
        # In the town the street trees took the fighting: some snapped, some burnt with the houses.
        if rnd(key, 8) < damage:
            name = veg_damaged(name, key)
        P.put(None, x, z, rnd(key, 1)*360, 0.85+0.3*rnd(key, 2), sink=0.05, tag='town_trees', path=veg_path(name))
    for si, st in enumerate(streets):
        if st['kind'] in ('lane', 'alley'):
            p = st['pts']
            for k in range(0, len(p), max(int(variant['street_tree_m']/4.0), 1)):
                for side in (-1, 1):
                    if k+1 >= len(p):
                        continue
                    d = (p[k+1]-p[k])/max(np.linalg.norm(p[k+1]-p[k]), 1e-6)
                    q = p[k]+np.array([-d[1], d[0]])*side*(st['half']+1.6)
                    r = np.hypot(q[0]-cx, q[1]-cz)
                    if r > 200 and rnd(f'st{si}.{k}.{side}', 4) < 0.7:
                        tree(f'st{si}.{k}.{side}', float(q[0]), float(q[1]), damage=0.35 if r < 320 else 0.1)
                    elif r > 200 and rnd(f'st{si}.{k}.{side}', 9) < 0.5:
                        # Hedgerow along the lane where no tree stands.
                        tree(f'hg{si}.{k}.{side}', float(q[0]), float(q[1]), pool=HEDGE, damage=0.08)
    for side in (-1, 1):
        for z in np.arange(-CROP_HALF+40, CROP_HALF-40, 9.0 if dens > 0.5 else 20.0):
            key = f'bank{side}.{int(z)}'
            off = RIVER_HALF_W+BANK_RUN+3.0+3.0*rnd(key, 5)
            if abs(z-cz) < 330 and dens > 0.5:
                off = RIVER_HALF_W+BANK_RUN+4.5          # between the quay and the water in town
            x = float(centre_x(np.array([z]))[0])+side*off
            if rnd(key, 6) < 0.75:
                tree(key, x, float(z), pool=BANK_TREES if rnd(key, 7) < 0.7 else BANK_SCRUB,
                     damage=0.3 if abs(z-cz) < 330 else 0.05)
    for b, (x0, x1, z0, z1) in zip(buildings, rects):
        if b['typology'] == 'wooden_house' and rnd(b['id'], 30) < 0.8*dens+0.2:
            for k in range(1+int(rnd(b['id'], 31)*3)):
                key = f"{b['id']}.g{k}"
                tree(key, (x0+x1)/2+(rnd(key, 4)-0.5)*16, (z1 if rnd(key, 5) < .5 else z0)+(rnd(key, 6)-0.5)*20,
                     pool=ORCHARD if rnd(key, 7) < 0.7 else GARDEN_SHRUBS, damage=0.25)


def scatter_ground(P, buildings, streets, cx, cz, centre_x, variant, trees_xz):
    """Sticks and stones and small natural objects over the whole square, and the war's litter:
    clumped by world-fixed noise, clear of the river, the streets, the houses and the bridge."""
    pts, half = street_samples(streets)
    rects = [cc.footprint(b['x'], b['z'], b['width_m'], b['depth_m'], b['yaw_deg']) for b in buildings]
    reach = BRIDGE_HALF_L+RAMP_M
    def ok(x, z, r=0.6, streets_ok=False):
        if abs(x) > CROP_HALF-10 or abs(z) > CROP_HALF-10:
            return False
        if abs(x-float(centre_x(np.array([z]))[0])) < RIVER_HALF_W+BANK_RUN+2.0:
            return False
        if abs(z-cz) < BRIDGE_HALF_W+6 and abs(x-cx) < reach:
            return False
        if not streets_ok and rect_hits_streets((x-r, x+r, z-r, z+r), pts, half, margin=-0.3):
            return False
        return all(not (x0-r < x < x1+r and z0-r < z < z1+r) for x0, x1, z0, z1 in rects)
    def clump(x, z, scale, salt):
        # world-fixed clumping: a coarse hash lattice smoothly interpolated
        gx, gz = x/scale, z/scale
        ix, iz = math.floor(gx), math.floor(gz)
        fx, fz = gx-ix, gz-iz
        v = [[rnd(f'cl{salt}.{ix+i}.{iz+j}', 1) for i in (0, 1)] for j in (0, 1)]
        top = v[0][0]*(1-fx)+v[0][1]*fx
        bot = v[1][0]*(1-fx)+v[1][1]*fx
        return top*(1-fz)+bot*fz
    n_nat = variant['natural']
    k = 0
    # Stones: small ones everywhere in clumps, larger ones rarer, the odd boulder.
    for kind, count, scale_lo, scale_hi, sink in (('stone_s', 9000, 0.6, 1.6, 0.35), ('stone_m', 3500, 0.5, 1.4, 0.4),
                                                   ('stone_l', 700, 0.25, 0.6, 0.45), ('boulder_a', 60, 0.25, 0.5, 0.5),
                                                   ('boulder_b', 60, 0.25, 0.5, 0.5)):
        placed = 0
        tries = 0
        while placed < int(count*n_nat) and tries < count*8:
            tries += 1
            key = f'stone.{kind}.{tries}'
            x = (rnd(key, 1)-0.5)*2*(CROP_HALF-20)
            z = (rnd(key, 2)-0.5)*2*(CROP_HALF-20)
            if rnd(key, 3) > clump(x, z, 60.0, 'stone')**2.2 or not ok(x, z, 0.8):
                continue
            sc = scale_lo+(scale_hi-scale_lo)*rnd(key, 4)
            P.put(kind, x, z, rnd(key, 5)*360, sc, sink=sink*sc, tag='stones')
            placed += 1
    # Fallen branches, stumps, ferns and mushrooms where trees stand; branches scattered thinly elsewhere.
    trees = trees_xz if len(trees_xz) else np.zeros((1, 2))
    for kind, count, near, lo, hi, sink in (('branch', 4200, 9.0, 0.12, 0.30, 0.05), ('stump', 500, 7.0, 0.6, 1.1, 0.1),
                                             ('fern_a', 900, 6.0, 0.6, 1.0, 0.05), ('fern_b', 500, 6.0, 0.5, 0.9, 0.05),
                                             ('mushroom', 600, 4.0, 0.6, 1.3, 0.02)):
        for i in range(int(count*n_nat)):
            key = f'wood.{kind}.{i}'
            if kind == 'branch' and rnd(key, 9) < 0.25:
                x = (rnd(key, 1)-0.5)*2*(CROP_HALF-20)
                z = (rnd(key, 2)-0.5)*2*(CROP_HALF-20)
            else:
                t = trees[int(rnd(key, 1)*len(trees)) % len(trees)]
                a = rnd(key, 2)*math.tau
                r = near*(0.3+0.7*rnd(key, 3))
                x, z = float(t[0])+math.cos(a)*r, float(t[1])+math.sin(a)*r
            if not ok(x, z, 0.4):
                continue
            sc = lo+(hi-lo)*rnd(key, 4)
            P.put(kind, x, z, rnd(key, 5)*360, sc, sink=sink, tag='woodland_litter')
            k += 1
    # The war's litter: rubble heaped round every damaged house, the streets strewn, kit along the roads.
    n_rub = variant['rubble']
    litter = [('brick3', .22), ('brick', .12), ('debris_bricks', .12), ('debris_bricks2', .08), ('bricks_stacked1', .06),
              ('plank4', .10), ('plank', .06), ('board', .05), ('scrap', .06), ('scrap2', .04), ('gravel', .04),
              ('cement_bag', .03), ('tire2', .02)]
    for b, (x0, x1, z0, z1) in zip(buildings, rects):
        if b['condition'] == 'intact':
            continue
        heavy = {'partly_collapsed': 34, 'shelled': 24, 'burned': 18}[b['condition']]
        for i in range(int(heavy*n_rub)):
            key = f"rub.{b['id']}.{i}"
            side = int(rnd(key, 1)*4)
            t = rnd(key, 2)
            out = 0.3+6.0*rnd(key, 3)**1.6           # most close under the walls
            x, z = [(x0+(x1-x0)*t, z0-out), (x0+(x1-x0)*t, z1+out), (x0-out, z0+(z1-z0)*t), (x1+out, z0+(z1-z0)*t)][side]
            if not ok(x, z, 0.15, streets_ok=True):
                continue
            name = pick(key, [('charred', .5), ('plank4', .2), ('brick3', .3)]) if b['condition'] == 'burned' else pick(key, litter)
            P.put(name, x, z, rnd(key, 4)*360, 0.7+0.7*rnd(key, 5), sink=0.02, tag='rubble')
    for si, st in enumerate(streets):
        p = st['pts']
        for j in range(0, len(p), 1):
            for q in range(2 if st['kind'] in ('main', 'quay') else 1):
                key = f'strew.{si}.{j}.{q}'
                if rnd(key, 1) > 0.55*n_rub:
                    continue
                d = (p[min(j+1, len(p)-1)]-p[max(j-1, 0)])
                d = d/max(np.linalg.norm(d), 1e-6)
                off = (rnd(key, 2)-0.5)*2*st['half']*1.1
                x, z = float(p[j][0]-d[1]*off), float(p[j][1]+d[0]*off)
                if not ok(x, z, 0.1, streets_ok=True):
                    continue
                P.put(pick(key, litter), x, z, rnd(key, 3)*360, 0.6+0.8*rnd(key, 4), sink=0.02, tag='street_litter')


def battlefield(P, cx, cz, centre_x, route, variant, craters, town_rects):
    """The remains of the fighting before the crossing (attested recurring conditions of the 1943
    river battles, not a named engagement): a broken assault line across the field west of the
    town, a knocked-out anti-tank gun on a rise with its crew, a burnt column on the east road,
    knocked-out armour across the fields, and the kit the infantry left."""
    b = variant['battle']
    def clear(x, z):
        return (abs(x) < CROP_HALF-20 and abs(z) < CROP_HALF-20
                and abs(x-float(centre_x(np.array([z]))[0])) > RIVER_HALF_W+BANK_RUN+8
                and not any(x0-3 < x < x1+3 and z0-3 < z < z1+3 for x0, x1, z0, z1 in town_rects))
    # Lying poses only: the raised-arm and seated poses read as living figures in open ground.
    dead = ['dead_down', 'dead_side', 'dead_curled', 'dead_down', 'dead_side']
    # The assault line: rows of riflemen fallen facing the town, thickest where it broke.
    for row in range(3):
        for k in range(int(26*b)):
            key = f'line{row}.{k}'
            t = (k+rnd(key, 1))/26.0
            x = cx-430+row*45+(rnd(key, 2)-0.5)*20+t*40
            z = cz-260+t*520
            if not clear(x, z) or rnd(key, 3) > 0.25+0.6*math.exp(-((z-cz+40)/140)**2):
                continue
            P.put(dead[int(rnd(key, 4)*len(dead))], x, z, 90+(rnd(key, 5)-0.5)*120, 1.0, tag='dead')
            if rnd(key, 6) < 0.4:
                P.put('helmet', x+(rnd(key, 7)-0.5)*3, z+(rnd(key, 8)-0.5)*3, rnd(key, 9)*360, 1.0, tag='kit')
    # The anti-tank gun position on the rise south-west of the road, its crew round it.
    gx, gz = cx-640, cz-330
    if clear(gx, gz):
        P.put('gun_wreck', gx, gz, 70, 1.0, tag='wreck')
        P.put('sandbags', gx+3, gz-2, 160, 1.0, tag='defence'); P.put('sandbags', gx-3, gz-3, 200, 1.0, tag='defence')
        for k in range(4):
            P.put(dead[k], gx+(rnd(f'gun{k}', 1)-0.5)*8, gz+(rnd(f'gun{k}', 2)-0.5)*8, rnd(f'gun{k}', 3)*360, 1.0, tag='dead')
        for k in range(3):
            P.put('ammo_burst', gx+4+k*1.2, gz+2, rnd(f'gunb{k}', 1)*360, 1.0, tag='kit')
    # The burnt column on the east road: lorries, wagons, horses, their drivers.
    east = [p for p in route if cx+250 < p[0] < cx+1000]
    for k, name in enumerate(['truck_burned', 'wagon_wreck', 'truck_hit', 'wagon_wreck', 'truck_burned', 'wagon_wreck'][:max(3, int(6*b))]):
        if not east:
            break
        q = east[min(int(len(east)*(0.15+0.12*k)), len(east)-1)]
        x, z = float(q[0])+(rnd(f'col{k}', 1)-0.5)*4, float(q[1])+(5.0 if k % 2 else -5.0)
        if clear(x, z):
            P.put(name, x, z, rnd(f'col{k}', 2)*30-15+90, 1.0, tag='wreck')
            if name == 'wagon_wreck':
                P.put('horse_dead', x+3.0, z+(2.5 if k % 2 else -2.5), rnd(f'col{k}', 3)*360, 1.0, tag='wreck')
            if rnd(f'col{k}', 4) < 0.7:
                P.put(dead[k % len(dead)], x-3.0, z+1.5, rnd(f'col{k}', 5)*360, 1.0, tag='dead')
    # Knocked-out armour across the fields, some in the crater lines, each with its dead.
    for k in range(int(16*b)):
        key = f'ko{k}'
        if craters and rnd(key, 9) < 0.5:
            c = craters[int(rnd(key, 1)*len(craters))]
            x, z = c[0]+6+rnd(key, 2)*8, c[1]+(rnd(key, 3)-0.5)*10
        else:
            x, z = (rnd(key, 1)-0.5)*2*(CROP_HALF-120), (rnd(key, 2)-0.5)*2*(CROP_HALF-120)
        if not clear(x, z):
            continue
        P.put(['wreck_kv1', 'wreck_pz3', 'wreck_kv1', 'truck_hit'][k % 4], x, z, rnd(key, 4)*360, 1.0, tag='wreck')
        for d in range(int(rnd(key, 5)*3)):
            P.put(dead[(k+d) % len(dead)], x+(rnd(f'{key}d{d}', 1)-0.5)*10, z+(rnd(f'{key}d{d}', 2)-0.5)*10,
                  rnd(f'{key}d{d}', 3)*360, 1.0, tag='dead')
    # The infantry's scattered dead and their kit across the whole field, and field graves.
    for k in range(int(140*b)):
        key = f'fall{k}'
        x, z = (rnd(key, 1)-0.5)*2*(CROP_HALF-60), (rnd(key, 2)-0.5)*2*(CROP_HALF-60)
        if not clear(x, z) or rnd(key, 3) > 0.6:
            continue
        P.put(dead[int(rnd(key, 4)*len(dead))], x, z, rnd(key, 5)*360, 1.0, tag='dead')
        for j in range(int(rnd(key, 6)*3)):
            P.put(pick(f'{key}k{j}', [('helmet', .4), ('rifle_grave', .1), ('canister', .2), ('ammo_burst', .15), ('stretcher', .15)]),
                  x+(rnd(f'{key}k{j}', 1)-0.5)*5, z+(rnd(f'{key}k{j}', 2)-0.5)*5, rnd(f'{key}k{j}', 3)*360, 1.0, tag='kit')
    for k in range(int(8*b)):
        key = f'shat{k}'
        x, z = (rnd(key, 1)-0.5)*2*(CROP_HALF-80), (rnd(key, 2)-0.5)*2*(CROP_HALF-80)
        if clear(x, z):
            name = veg_damaged(['tree_oak_a', 'tree_poplar_a', 'tree_linden_a', 'tree_birch_a', 'tree_willow_a'][k % 5], key)
            P.put(None, x, z, rnd(key, 3)*360, 0.85+0.3*rnd(key, 4), sink=0.1, tag='shattered_trees', path=veg_path(name))
    # Where the eye goes: the road. The thaw gives back the winter's dead where they fell and no
    # burial party reached (an attested recurring condition of the 1942-43 Eastern Front spring:
    # corpses surfacing from the snow along roads and in the fields, collected weeks or months
    # later); they lie on the verges and in the ditches of the approach, in the open short grass of
    # the road's edge, kit strewn round them, a stretcher party that did not get back.
    pr = np.asarray(route, dtype=np.float64)
    if len(pr) > 2:
        along = np.concatenate([[0.0], np.cumsum(np.hypot(*np.diff(pr, axis=0).T))])
        for s in np.arange(10.0, along[-1]-10.0, 22.0/max(b, 0.25)):
            key = f'verge{s:.0f}'
            i = int(np.searchsorted(along, s))
            if i <= 0 or i >= len(pr):
                continue
            q, d = pr[i], pr[i]-pr[i-1]
            d = d/max(np.hypot(*d), 1e-6)
            if abs(q[0]-cx) < BRIDGE_HALF_L+RAMP_M+10 and abs(q[1]-cz) < 40:
                continue
            near_bridge = math.exp(-((q[0]-cx)/420.0)**2)   # thickest where the crossing was fought for
            if rnd(key, 1) > 0.25+0.55*near_bridge:
                continue
            side = 1.0 if rnd(key, 2) < 0.5 else -1.0
            off = 3.6+rnd(key, 3)*6.0
            x, z = float(q[0]-d[1]*side*off), float(q[1]+d[0]*side*off)
            if not clear(x, z):
                continue
            yaw = math.degrees(math.atan2(d[0], d[1]))+(rnd(key, 4)-0.5)*140
            P.put(dead[int(rnd(key, 5)*len(dead))], x, z, yaw, 1.0, tag='dead_verge')
            if rnd(key, 6) < 0.3:     # a second man, fallen beside the first
                P.put(dead[int(rnd(key, 7)*len(dead))], x+(rnd(key, 8)-0.5)*4, z+(rnd(key, 9)-0.5)*4, rnd(key, 10)*360, 1.0, tag='dead_verge')
            if rnd(key, 11) < 0.12:
                P.put('stretcher', x+1.5*d[0], z+1.5*d[1], yaw+90, 1.0, tag='kit')
            for j in range(int(rnd(key, 12)*3)):
                P.put(pick(f'{key}k{j}', [('helmet', .45), ('canister', .25), ('ammo_burst', .15), ('rifle_grave', .15)]),
                      x+(rnd(f'{key}k{j}', 1)-0.5)*4, z+(rnd(f'{key}k{j}', 2)-0.5)*4, rnd(f'{key}k{j}', 3)*360, 1.0, tag='kit')
    # The shell holes near the road: men caught by the barrage lie on the torn rims, bare earth
    # round them where the sward was blown away.
    for x0, z0, r in craters:
        key = f'rim{x0:.0f},{z0:.0f}'
        dmin = np.hypot(*(pr-[x0, z0]).T).min() if len(pr) else 1e9
        if dmin > 160 or rnd(key, 1) > 0.45*b+0.1:
            continue
        a = rnd(key, 2)*math.tau
        x, z = x0+math.cos(a)*r*1.05, z0+math.sin(a)*r*1.05
        if clear(x, z):
            P.put(dead[int(rnd(key, 3)*len(dead))], x, z, math.degrees(a)+90+(rnd(key, 4)-0.5)*60, 1.0, tag='dead_crater')
    # A row of field graves dug in haste on the camp side, rifles and helmets for markers.
    gx, gz = cx-520, cz+60
    for k in range(int(9*b)+3):
        x, z = gx+k*2.2+(rnd(f'grave{k}', 1)-0.5)*0.6, gz+(rnd(f'grave{k}', 2)-0.5)*0.8
        if clear(x, z):
            P.put('rifle_grave' if k % 3 else 'grave_cross', x, z, 90+(rnd(f'grave{k}', 3)-0.5)*20, 1.0, tag='graves')
            if rnd(f'grave{k}', 4) < 0.5:
                P.put('helmet', x+0.4, z+0.3, rnd(f'grave{k}', 5)*360, 1.0, tag='graves')
    # Trees brought down by shellfire, leaves still green on them, beside their stumps.
    for k in range(int(30*b)):
        key = f'felled{k}'
        x, z = (rnd(key, 1)-0.5)*2*(CROP_HALF-80), (rnd(key, 2)-0.5)*2*(CROP_HALF-80)
        if clear(x, z):
            fallen, stump = FALLEN[int(rnd(key, 3)*len(FALLEN))]
            yaw = rnd(key, 4)*360
            P.put(None, x, z, yaw, 0.9+0.2*rnd(key, 5), sink=0.05, tag='felled_trees', path=veg_path(fallen))
            if stump:
                P.put(None, x, z, yaw, 1.0, sink=0.02, tag='felled_trees', path=veg_path(stump))


def night_lights(P, H, grid, cx, cz, deck_y, town, craters, variant):
    """Lights for the night (environment_authoring's authored lights). The front was blacked out:
    the bridge's lamp is a shielded blackout lamp on its pole at the camp side (warm, low); the
    rest are fires (houses and wrecks still burning off across the fields) and a curtained window
    or two in the houses that stand."""
    lights = []
    def light(id_, x, z, h, temp, tint, power, radius, rng, flicker):
        lights.append(dict(id=id_, kind='point', position=[cc.r3(x), cc.r3(grid.sample(H, x, z)+h), cc.r3(z)],
                           direction=[0, -1, 0], temperature_k=temp, tint_rgb=tint, radiant_intensity=power,
                           source_radius_m=radius, range_m=rng, cone_angle_deg=60.0, flicker=flicker, enabled=True))
    # The bridge lamp: a pole at the west abutment, the camp's side, a shielded lamp head on top.
    lx, lz = cx-BRIDGE_HALF_L-4.0, cz+BRIDGE_HALF_W+1.5
    ground = deck_y
    P.put('pole', lx, lz, 0.0, [1.6, 1.6, 15.0], sink=-(deck_y-grid.sample(H, lx, lz))-2.25, tag='lamp')
    P.m['placements'][-1].append([90.0, 0.0])
    P.put('lamp_head', lx, lz, 90.0, 1.4, sink=-(deck_y-grid.sample(H, lx, lz))-4.3, tag='lamp')
    lights.append(dict(id='bridge_lamp', kind='point', position=[cc.r3(lx), cc.r3(deck_y+4.2), cc.r3(lz)], direction=[0, -1, 0],
                       temperature_k=2600.0, tint_rgb=[1.0, 0.86, 0.66], radiant_intensity=18.0, source_radius_m=0.08,
                       range_m=22.0, cone_angle_deg=70.0, flicker=0.02, enabled=True))
    # Fires: burned houses still smouldering, and a few wrecks burning off across the fields.
    burning = [b for b in town if b['condition'] == 'burned'][:3]
    for k, b in enumerate(burning):
        light(f'house_fire_{k}', b['x'], b['z'], 2.5, 1850.0, [1.0, 0.72, 0.46], 95.0, 0.5, 30.0, 0.35)
    far = sorted(craters, key=lambda c: -math.hypot(c[0]-cx, c[1]-cz))[:6]
    for k, (x, z, r) in enumerate(far[:max(2, int(5*variant['battle']))]):
        light(f'field_fire_{k}', x+3, z, 1.2, 1800.0, [1.0, 0.70, 0.44], 120.0, 0.6, 40.0, 0.4)
    # A curtained window or two in the houses that stand.
    standing = [b for b in town if b['condition'] in ('intact', 'shelled')][:2]
    for k, b in enumerate(standing):
        light(f'window_{k}', b['x'], b['z'], 3.0, 2700.0, [1.0, 0.84, 0.62], 6.0, 0.3, 10.0, 0.05)
    return lights


def ground_paint(colour, grass, axis_tex, streets, rects, squares, centre_tex, cx, cz, craters=()):
    """Streets (cobbles in the centre, dirt lanes), squares, yards; the river and its banks."""
    d_tex = axis_tex[None, :]-centre_tex[:, None]
    ad = np.abs(d_tex)
    # Inside the town the strip fields give way to gardens and meadow between the houses.
    rr = np.hypot(axis_tex[None, :]-cx, axis_tex[:, None]-cz)
    town = np.clip((190.0-rr)/60.0, 0, 1)
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
    # Shell holes: torn earth, the sward blown away to the rim and thrown soil beyond it.
    for x, z, r in craters:
        stamp(x, z, r*1.25, [0.17, 0.13, 0.09], 1.0)
        stamp(x, z, r*1.9, [0.24, 0.19, 0.13], 0.45)
    # Earth under the sward: the meadow (its presence is the ground class's red) covers it; where
    # the grass thins the loam shows, never a painted green.
    # A May sward is continuous: wherever grass grows at all it grows thick (the meadow's density
    # is its own); ploughed strips, roads, craters and the river stay bare.
    grass[:] = np.where(grass > 0.06, np.maximum(grass, 0.75), grass)
    g = np.clip(grass*2.2, 0, 1)[..., None]
    loam = np.array([0.20, 0.16, 0.11], dtype=np.float32)*(0.85+0.3*fine[..., None])
    colour[:] = colour*(1-g)+loam*g


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
    cc.ASSETS['tree_tall'] = veg_path('tree_linden_a')
    cc.ASSETS['tree_small'] = veg_path('tree_apple_a')
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
    town_r = 300.0
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
    # Shelling across the landscape: walking barrages leave lines of craters, harassing fire
    # single ones; clear of the river, the bridge and the houses. Recorded for the ground paint.
    field_craters = []
    lines = variant['field_craters']//10
    for li in range(lines):
        key = f'barrage{li}'
        x0 = (rnd(key, 1)-0.5)*2*(CROP_HALF-150); z0 = (rnd(key, 2)-0.5)*2*(CROP_HALF-150)
        a = rnd(key, 3)*math.tau; n = 5+int(rnd(key, 4)*8)
        for k in range(n):
            ck = f'{key}.{k}'
            x = x0+math.cos(a)*k*(14+10*rnd(ck, 1))+(rnd(ck, 2)-0.5)*10
            z = z0+math.sin(a)*k*(14+10*rnd(ck, 1))+(rnd(ck, 3)-0.5)*10
            field_craters.append((x, z, ck))
    for k in range(variant['field_craters']-len(field_craters)):
        ck = f'single{k}'
        field_craters.append(((rnd(ck, 1)-0.5)*2*(CROP_HALF-60), (rnd(ck, 2)-0.5)*2*(CROP_HALF-60), ck))
    kept_craters = []
    for x, z, ck in field_craters:
        if in_river(x, z, 15) or (abs(z-cz) < BRIDGE_HALF_W+12 and abs(x-cx) < BRIDGE_HALF_L+RAMP_M+12):
            continue
        if any(x0_-6 < x < x1_+6 and z0_-6 < z < z1_+6 for x0_, x1_, z0_, z1_ in town_rects):
            continue
        r = 1.6+3.4*rnd(ck, 5)**1.5           # mostly field-gun and mortar holes, some heavy
        crater(H, axis, x, z, r, 0.5+0.35*r)
        kept_craters.append((x, z, r))
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
    # The woods in the spring set's species (mature, real size), an understorey of hazel and
    # hawthorn; within reach of the shelling a wood's trees are shattered or burnt.
    weights = np.cumsum([w for _, w in WOOD_MIX])/sum(w for _, w in WOOD_MIX)
    crater_xz = np.array([[x, z] for x, z, _ in kept_craters]) if kept_craters else np.zeros((0, 2))
    for p in manifest['placements'][-trees:]:
        key = f'wood{p[1]:.1f},{p[3]:.1f}'
        name = WOOD_MIX[int(np.searchsorted(weights, rnd(key, 1)))][0]
        near = np.hypot(*(crater_xz-[p[1], p[3]]).T).min() if len(crater_xz) else 1e9
        if near < 35.0 and rnd(key, 2) < 0.6*(1.0-near/35.0)+0.15:
            name = veg_damaged(name, key)
        p[0] = P.asset(veg_path(name))
        p[5] = cc.r3(0.8+0.35*rnd(key, 3))
    tree_paths = {i for i, a in enumerate(manifest['assets']) if any(t in a for t in ('tree_', 'pine_tree', 'shrub_'))}
    trees_xz = np.array([[q[1], q[3]] for q in manifest['placements'] if q[0] in tree_paths])
    scatter_ground(P, town, streets, cx, cz, centre_x, variant, trees_xz)
    battlefield(P, cx, cz, centre_x, route, variant, kept_craters, town_rects)
    for x, z, r in kept_craters:
        if rnd(f'crs{x:.0f}{z:.0f}', 1) < 0.35:
            P.put('scorch', x, z, rnd(f'crs{x:.0f}{z:.0f}', 2)*360, r*0.9, sink=0.0, tag='crater_scorch')
    edits['authored_lights'] = night_lights(P, H, grid, cx, cz, deck_y, town, kept_craters, variant)
    # The course: route, start, goal, encounter, all on the cropped square.
    course = edits['canyon_course']
    inside = (np.abs(route[:, 0]) < CROP_HALF-60) & (np.abs(route[:, 1]) < CROP_HALF-60)
    idx = np.where(inside)[0]
    route = route[idx.min():idx.max()+1]
    course['route'] = [[cc.r3(x), cc.r3(z)] for x, z in route]
    seg = np.hypot(*np.diff(route, axis=0).T)
    course['length_m'] = cc.r3(float(seg.sum()))
    start_x = cx-420.0          # in the Tiger, the town and the bridge ahead (the owner, 2 Nov)
    s = int(np.argmin(np.abs(route[:, 0]-start_x)+1e3*(route[:, 0] > cx)))
    heading = math.degrees(math.atan2(cx-route[s][0], cz-route[s][1]))   # facing the bridge
    course['start'] = dict(x=cc.r3(route[s, 0]), z=cc.r3(route[s, 1]), heading_deg=cc.r3(heading))
    # The camp stands just over the first rise behind the start, in its lee, out of the town's
    # sight (the owner, 2 Nov): walk the road back west to the first crest, then 70 m beyond.
    back = route[:s+1][::-1]
    hs = np.array([grid.sample(H, float(x), float(z)) for x, z in back])
    along_back = np.concatenate([[0.0], np.cumsum(np.hypot(*np.diff(back, axis=0).T))])
    crest = next((i for i in range(4, len(hs)-4) if along_back[i] > 60 and hs[i] >= hs[i-4:i+5].max() - 1e-3), None)
    if crest is None:
        crest = int(np.searchsorted(along_back, 250.0))
    k = min(int(np.searchsorted(along_back, along_back[crest]+70.0)), len(back)-1)
    course['camp_at'] = [cc.r3(float(back[k][0])), cc.r3(float(back[k][1]))]
    print('[spring-bridge-v2] camp:', course['camp_at'], f'{along_back[k]:.0f} m behind the start, crest at {along_back[crest]:.0f} m')
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
        # Nothing of the defence stands behind the Tiger's start: the enemy holds the town and
        # the far bank.
        zone['candidates'] = [remap[i] for i in zone['candidates'] if i in remap and sites[remap[i]]['x'] > start_x+250]
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
                 width_m=2*BRIDGE_HALF_W if (abs(a[0]-cx) < BRIDGE_HALF_L+20 or abs(b[0]-cx) < BRIDGE_HALF_L+20) else 120.0)
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
    # Riflemen dug in on the approaches as well as in the houses: posts along the road and the
    # field edges on both banks, at least 40 m apart (each infantry group needs its own post,
    # src/canyon_game.rs), clear of the river, the houses and the bridge ramps.
    field_posts = []
    for k, (x, z) in enumerate(route[::6]):
        if not (start_x+220 < x < cx+900) or abs(x-cx) < BRIDGE_HALF_L+RAMP_M+10:
            continue
        for side in (-1, 1):
            off = side*(18.0+30.0*rnd(f'fp{k}.{side}', 1))
            px, pz = float(x), float(z)+off
            if in_river(px, pz, 25.0) or any(x0-6 < px < x1+6 and z0-6 < pz < z1+6 for x0, x1, z0, z1 in town_rects):
                continue
            if all(math.hypot(px-q['x'], pz-q['z']) >= 40.0 for q in posts+field_posts):
                field_posts.append(dict(x=cc.r3(px), z=cc.r3(pz), heading_deg=270.0 if px > cx else 90.0, host='field'))
    course['garrison'] = posts[:24]+field_posts
    print('[spring-bridge-v2] garrison posts:', len(posts[:24]), 'in the houses,', len(field_posts), 'on the approaches')
    course.update(variant=variant['dest'], tool='tools/worldgen/spring_bridge_v2.py', finish='mission', director=False,
                  opening_platoons=False, headline='The bridge is held', camp=True, camp_start='tank')
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
                                  deck_thickness_m=1.2, piers=piers, name='the_bridge',
                                  truss=dict(over_m=6.0, under_m=2.5, panel_m=7.0))]
    # Ground, cropped with everything else.
    # A May meadow is a continuous sward; in a war spring most strips went unsown and run to weeds,
    # only a few were ploughed before the front came through.
    colour, grass = sp.ground(rng, H, ravine*(np.abs(X-centre[:, None]) > FLOOD_HALF), route, buildings, woods,
                              clump_floor=0.82, bare=(42, 47))
    axis_tex = np.linspace(-EXTENT/2, EXTENT/2, TEX, dtype=np.float32)
    centre_tex = np.interp(axis_tex, axis, centre).astype(np.float32)
    ground_paint(colour, grass, axis_tex, streets, town_rects, squares, centre_tex, cx, cz, kept_craters)
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
    # The far hills go blue through Rayleigh and the aerosol's spectral slope along the view ray
    # (the campaign's spring aerosol). A May continental block (AOD 0.2, Angstrom 1.6, Chubarova
    # et al. 2016) waits: on 2 Nov it darkened the horizon sky below the zenith, which is unphysical.
    edits['environment_lighting'] = dict(enabled=True, fog_density=0.00045, overcast=0.06, storm=0.0,
        schema='kessel.environment-authoring', utc='1943-05-12T14:10:00Z', version=2)
    # Low cloud and mist banks inside the map (src/low_clouds.rs): liquid-water volumes lit and
    # shadowed through the raymarch transport, drifting with the wind (the owner, 2 Nov 2026).
    edits['low_clouds'] = dict(seed=11, count=variant['low_clouds'],
        area=[-CROP_HALF+100, -CROP_HALF+100, CROP_HALF-100, CROP_HALF-100], base_m=[0, 60], size_m=[50, 260],
        depth_m=[15, 70], lwc_g_m3=[0.08, 0.3], droplet_radius_um=8, drift=1.0)
    # Any tree still from the PSX library (the grammars' own) becomes a spring tree of the set.
    import re
    weights = np.cumsum([w for _, w in WOOD_MIX])/sum(w for _, w in WOOD_MIX)
    psx_tree = re.compile(r'/(tree_\d+|pine_tree_n_\d+)\.glb$')
    for q in manifest['placements']:
        if psx_tree.search(manifest['assets'][q[0]]):
            key = f'psx{q[1]:.1f},{q[3]:.1f}'
            q[0] = P.asset(veg_path(WOOD_MIX[int(np.searchsorted(weights, rnd(key, 1)))][0]))
            q[5] = cc.r3(0.8+0.35*rnd(key, 3))
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
