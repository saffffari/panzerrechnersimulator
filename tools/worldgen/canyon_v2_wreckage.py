#!/usr/bin/env python3
"""Canyon v2 wreckage layer: the aftermath of a destroyed army, as a manifest_edits.json overlay.

Adds knocked-out armour columns, retreat lines of lorries, carts and sledges with their dead
draught horses, overrun 45 mm gun positions, dissolved-unit kit carpets, shell craters with
scorched rims and drifted snow to the Canyon v2 workshop
(assets/worldgen/workshops/gaea-canyon-run-v2) without touching the generated base manifest:
every placement is an `add` op tagged `canyon_v2_wreckage` in manifest_edits.json, so the layer
can be regenerated, re-seeded or removed on its own, and the generator's hand-edit guard on the
base outputs stays intact.

Placements stay on the valley floor (height within a band of the route's own floor, bounded local
slope), off the drivable centreline (edge clearance >= the course's 12 m lane half-width), clear
of buildings, rocks, existing wrecks, garrison and encounter positions, the spawn and the goal.
Vehicles follow the local terrain gradient through the placement tilt. Nothing readable is added.

    python tools/worldgen/canyon_v2_wreckage.py [--root PATH] [--dest WORKSHOP] [--seed N]
                                              [--density F] [--remove] [--report]
    python tools/worldgen/canyon_v2_wreckage.py --cameras            # recon cameras along the route
    python tools/worldgen/canyon_v2_wreckage.py --shots PREFIX       # headless stills, clear + blizzard

Provenance (recorded in the `canyon_v2_wreckage` block beside the ops): attested recurring
conditions of the Kessel's roads, November 1942 - January 1943, not a named engagement. See
PROVENANCE below and docs/CANYON_V2_WRECKAGE_20261027.md.
"""
import argparse
import collections
import hashlib
import json
import math
import os
import subprocess
import sys
from pathlib import Path

import numpy as np

TAG = 'canyon_v2_wreckage'
VERSION = 1
TOOL = 'tools/worldgen/canyon_v2_wreckage.py'
DEFAULT_SEED = 0x20261027
DEST = 'assets/worldgen/workshops/gaea-canyon-run-v2'
LANE_HALF_M = 12.0          # the course's clear travel lane (gaea_canyon_v2.dress: lane=12)
FLOOR_BAND_M = 9.0          # placements stay within this height of the route's own floor
SPAWN_CLEAR_M = 60.0
GOAL_CLEAR_M = 40.0
WIND = (-0.60, 0.80)        # snow drifts form in the lee of this (x, z) wind direction

BF = 'assets/authoring/battlefield/'
CV = 'assets/authoring/canyon-v2/'
MP2 = 'assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/'
MP3 = 'assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/'
HIST = 'assets/authoring/historical_20260925/'

# key: (path, long axis ('x' | 'z'), length, width, footprint radius for mutual spacing)
ASSETS = {
    'kv1_burnt': (BF + 'kv1_wreck_burnt.glb', 'x', 7.1, 3.3, 3.6),
    'kv1_detail': (BF + 'kv1_wreck_detail.glb', 'x', 7.1, 3.3, 3.6),
    'pz3_burnt': (BF + 'pz3_wreck_burnt.glb', 'x', 8.1, 4.0, 4.0),
    'pz3_detail': (BF + 'pz3_wreck_detail.glb', 'x', 8.1, 4.0, 4.0),
    'lorry_burnt': (BF + 'lorry_wreck_burnt.glb', 'z', 6.5, 2.6, 3.2),
    'lorry_detail': (BF + 'lorry_wreck_detail.glb', 'z', 6.5, 2.3, 3.2),
    'cart': (CV + 'wreck_cart.glb', 'z', 4.3, 2.1, 2.2),
    'sledge': (BF + 'panje_sledge_wreck_detail.glb', 'z', 4.4, 1.2, 2.0),
    'sledge_over': (BF + 'panje_sledge_overturned_wreck_detail.glb', 'z', 4.4, 1.3, 2.0),
    'horse_a': (BF + 'draught_horse_a_wreck_detail.glb', 'z', 3.0, 2.0, 1.5),
    'horse_b': (BF + 'draught_horse_b_wreck_detail.glb', 'z', 3.0, 2.0, 1.5),
    'gun45': (CV + 'wreck_gun45.glb', 'z', 3.3, 1.4, 1.7),
    'parapet': (BF + 'gun_pit_parapet.glb', 'z', 4.6, 7.3, 3.7),
    'outpost': (CV + 'outpost.glb', 'z', 3.8, 6.3, 3.2),
    'crater': (BF + 'crater_scar.glb', 'z', 2.7, 2.7, 1.3),
    'scorched': (BF + 'scorched_earth.glb', 'z', 2.2, 2.2, 1.0),
    'drift': (BF + 'snow_drift.glb', 'z', 2.2, 2.2, 1.0),
    'chip': (BF + 'shell_chip.glb', 'z', 2.2, 2.2, 0.6),
    'rifle': (BF + 'dropped_rifle.glb', 'x', 1.1, 0.1, 0.4),
    'coat_a': (BF + 'greatcoat_heap_a.glb', 'x', 1.1, 0.7, 0.5),
    'coat_b': (BF + 'greatcoat_heap_b.glb', 'x', 1.1, 0.7, 0.5),
    'wheel': (BF + 'loose_wheel.glb', 'x', 1.3, 1.3, 0.6),
    'crate_a': (MP2 + 'Props/wooden_crate_2_a.glb', 'z', 0.9, 0.8, 0.5),
    'crate_b': (MP2 + 'Props/wooden_crate_2_b.glb', 'z', 0.9, 0.8, 0.5),
    'jerrycan': (MP2 + 'Props/jerrycan_mx_1.glb', 'z', 0.5, 0.3, 0.3),
    'barrel': (MP2 + 'Props/metal_barrel_hr_1.glb', 'z', 0.7, 0.7, 0.4),
    'tyre': (MP2 + 'Props/tire_1.glb', 'z', 0.8, 0.8, 0.4),
    'board': (MP2 + 'Props/wooden_board_2.glb', 'x', 1.2, 0.3, 0.5),
    'luggage': (MP2 + 'Props/luggage_mp_1.glb', 'x', 0.8, 0.2, 0.4),
    'ammo_box': (MP3 + 'Items & Weapons/ammo_box_mp_1.glb', 'x', 0.3, 0.2, 0.2),
    'plank': (HIST + 'gumrak_charred_plank_a48a6440323a.glb', 'x', 1.2, 0.2, 0.5),
    'scorched_luggage': (HIST + 'gumrak_scorched_luggage_d2bd048056bf.glb', 'x', 0.8, 0.2, 0.4),
    'scorched_crate': (HIST + 'gumrak_scorched_crate_acc43161a401.glb', 'z', 1.9, 1.9, 0.9),
}
for i in range(6):
    ASSETS[f'casualty_{i}'] = (CV + f'casualty_{i}.glb', 'z', 1.9, 1.1, 0.9)
TILTED = {'kv1_burnt', 'kv1_detail', 'pz3_burnt', 'pz3_detail', 'lorry_burnt', 'lorry_detail',
          'cart', 'sledge', 'sledge_over', 'horse_a', 'horse_b', 'gun45', 'parapet'}
KIT = ['crate_a', 'crate_b', 'jerrycan', 'barrel', 'board', 'luggage', 'plank', 'scorched_luggage',
       'ammo_box', 'tyre', 'coat_a', 'coat_b', 'rifle']

# Radii that existing base placements keep around them, by model file stem.
BASE_RADIUS = {'rock_0': 6.0, 'rock_1': 9.0, 'rock_2': 4.0, 'outpost': 4.0, 'wreck_cart': 2.5,
               'wreck_gun45': 2.0, 'kv1_wreck_burnt': 4.5, 'kv1_wreck_detail': 4.5,
               'pz3_wreck_burnt': 4.5, 'pz3_wreck_detail': 4.5, 'wall_hr_1': 2.5, 'fence_b_1': 1.6,
               'tree_8': 1.0, 'tree_9': 1.0, 'pine_tree_n_1': 1.4, 'pine_tree_n_2': 1.4,
               'pine_tree_n_3': 1.4, 'resting_tree_log': 1.4, 'tree_stump_1': 0.6,
               'snow_drift': 0.0, 'scorched_earth': 0.0, 'wheat_1': 0.0}
BASE_RADIUS_DEFAULT = 0.35      # small debris and kit: a wreck may lie among it
# Loose base dressing that a vehicle may displace: when a hull's footprint lands on it, the base
# placement is deleted by index (a tagged `delete` op) rather than left inside the wreck.
DISPLACEABLE = {'wooden_crate_2_a', 'wooden_crate_2_b', 'wooden_board_2', 'wooden_plank_1',
                'gumrak_charred_plank_a48a6440323a', 'bricks_stacked_mx_1', 'debris_bricks_mx_1',
                'debris_bricks_mx_2', 'gravel_pile_hr_1', 'bucket_mx_1', 'bench_mx_1', 'wooden_barrel_1',
                'metal_barrel_hr_1', 'jerrycan_mx_1', 'snow_drift', 'scorched_earth', 'wheat_1',
                'resting_tree_log', 'tree_stump_1'}

PROVENANCE = {
    'class': 'attested_condition',
    'scope': ('Fictional Gaea mechanics course dressed with recurring, well-attested conditions of the '
              'Stalingrad pocket\'s roads and balkas, late November 1942 to January 1943: columns '
              'knocked out and abandoned for want of fuel, draught horses dead in their traces or '
              'butchered, overrun anti-tank positions, scattered kit and the frozen dead. Sites, counts, '
              'vehicle types and poses are authored inference; no named engagement, unit, victim or '
              'atrocity is depicted. The only prepared tank wrecks are a KV-1 and a Panzer III, both '
              'types present on this front in winter 1942-43; the lorry, horses and sledges are '
              'inferred composite props, not replicas of a make.'),
    'sources': [
        'docs/ENVSTORY.md "The Frozen Horses" and docs/research/data/envstory.json (Bechler testimony; '
        'Sixth Army draught horses slaughtered and frozen; axes rebounding, pioneer saws).',
        'docs/research/PHOTO_REFERENCE_ANALYSIS_20260928.md [05-01] Strunnikov 1943, abandoned vehicle '
        'column in echelon on one side of a snow road, kit on the other side; [05-02] burned cargo lorry '
        'with slat sides and canvas bows; [05-05] silenced battery behind heaped snow-and-earth '
        'parapets with a loose wheel; [05-03] Romanian cavalry saddles (the horses are absent).',
        'docs/research/URBAN_INTRO_TABLEAUX_20260928.md rows "The frozen dead that could not be buried" '
        'and "Horses slaughtered for food" (Beevor ch. 20-21, Stolberg report BA-MA RW4/v.264; '
        'Pozdnyakova; Kopyt photograph of a dead horse, February 1943).',
        'docs/research/HISTORICAL_SCENE_SOURCE_AUDIT_20260925.md A1-06 (79th Infantry Division diary, '
        'https://stalingrad1942-1943.ru/node/2723): Gumrak/Karpovka road, burning stores and abandoned '
        'usable equipment, late November 1942.',
        'docs/research/SOVIET_AT_WEAPONS_BRIEF_20260928.md: 45 mm M1937 (53-K) as the standard gun at '
        'every level, crews of six, moved by horse draught; parapets per [05-05].',
        'docs/research/STALINGRAD_URBAN_FABRIC_20260928.md 6.4 "Craters, wrecks, trees": wrecks along '
        'one side of a road, knocked-out tanks, crater densities.',
    ],
    'not_claimed': ('No unit identity, date, casualty count or butcher site is asserted for any one '
                    'scene; the Tiger\'s presence and the compressed chronology are validated separately '
                    '(docs/research/SOVIET_AT_WEAPONS_BRIEF_20260928.md: no Tigers at Stalingrad).'),
    'readable_text': 'none added',
}


# ---------------------------------------------------------------------------------------------------
# deterministic noise (the project's counter hash, gen_world.py / canyon_course.py)
# ---------------------------------------------------------------------------------------------------
def fnv1a32(text):
    h = 0x811C9DC5
    for byte in text.encode('utf-8'):
        h ^= byte
        h = (h * 0x01000193) & 0xFFFFFFFF
    return h


def hash01(n):
    x = (n * 2654435761) & 0xFFFFFFFF
    x ^= x >> 15
    x = (x * 2246822519) & 0xFFFFFFFF
    x ^= x >> 13
    return ((x >> 8) & 0xFFFF) / 65536.0


class Noise:
    def __init__(self, seed):
        self.seed = seed & 0xFFFFFFFF

    def __call__(self, key, salt):
        return hash01((self.seed ^ (fnv1a32(key) * 0x9E3779B9) ^ (salt * 0x85EBCA77)) & 0xFFFFFFFF)

    def between(self, key, salt, a, b):
        return a + (b - a) * self(key, salt)

    def pick(self, key, salt, items):
        return items[min(int(self(key, salt) * len(items)), len(items) - 1)]


def r3(v):
    return round(float(v), 3)


# ---------------------------------------------------------------------------------------------------
# the workshop: terrain, route, obstacles
# ---------------------------------------------------------------------------------------------------
class Route:
    def __init__(self, points):
        self.p = np.asarray(points, dtype=float)
        seg = np.hypot(*np.diff(self.p, axis=0).T)
        self.s = np.concatenate([[0.0], np.cumsum(seg)])
        self.L = float(self.s[-1])
        self.a = self.p[:-1]
        self.d = np.diff(self.p, axis=0)
        self.len2 = np.maximum((self.d * self.d).sum(1), 1e-9)

    def at(self, f):
        """Centreline point and unit tangent at fraction f."""
        f = min(max(f, 0.0), 1.0)
        dist = f * self.L
        i = int(np.clip(np.searchsorted(self.s, dist), 1, len(self.s) - 1))
        t = (dist - self.s[i - 1]) / max(self.s[i] - self.s[i - 1], 1e-9)
        p = self.p[i - 1] + (self.p[i] - self.p[i - 1]) * t
        j0, j1 = max(i - 3, 0), min(i + 2, len(self.p) - 1)
        tv = self.p[j1] - self.p[j0]
        tv /= max(np.linalg.norm(tv), 1e-9)
        return float(p[0]), float(p[1]), float(tv[0]), float(tv[1])

    def world(self, f, lateral):
        """World (x, z) at fraction f and signed lateral offset (+ right of the travel direction)."""
        x, z, tx, tz = self.at(f)
        nx, nz = -tz, tx
        return x + nx * lateral, z + nz * lateral

    def nearest(self, x, z):
        """(distance to the centreline, fraction of the nearest point)."""
        q = np.array([x, z]) - self.a
        t = np.clip((q * self.d).sum(1) / self.len2, 0.0, 1.0)
        dist = np.linalg.norm(q - self.d * t[:, None], axis=1)
        i = int(dist.argmin())
        return float(dist[i]), float((self.s[i] + t[i] * math.sqrt(self.len2[i])) / self.L)


class Workshop:
    def __init__(self, root, dest):
        self.root, self.dest = Path(root), Path(dest)
        self.manifest = json.loads((self.dest / 'manifest.v1.json').read_text())
        self.edits = json.loads((self.dest / 'manifest_edits.json').read_text())
        self.H = np.load(self.dest / self.manifest['terrain']['heights']).astype(np.float64)
        self.spacing = float(self.manifest['terrain']['spacing'])
        self.half = (self.H.shape[0] - 1) * 0.5
        self.route = Route(self.edits['canyon_course']['route'])
        self.spawn = (self.edits['canyon_course']['start']['x'], self.edits['canyon_course']['start']['z'])
        self.goal = (self.edits['canyon_course']['goal']['x'], self.edits['canyon_course']['goal']['z'])
        # footprints of the standing buildings (axis-aligned by the course's 90 degree yaw grid)
        self.rects = []
        for b in self.edits.get('building_instances', []):
            q = int(round(b['yaw_deg'] / 90.0)) % 2
            w, d = (b['width_m'], b['depth_m']) if q == 0 else (b['depth_m'], b['width_m'])
            self.rects.append((b['x'] - w / 2, b['x'] + w / 2, b['z'] - d / 2, b['z'] + d / 2))
        # existing base placements (and earlier non-wreckage overlay adds) with their keep-out radii
        assets = self.manifest['assets']
        self.base = []
        for p in self.manifest['placements']:
            stem = Path(assets[p[0]]).stem
            self.base.append((float(p[1]), float(p[3]), BASE_RADIUS.get(stem, BASE_RADIUS_DEFAULT)))
        for op in self.edits.get('ops', []):
            if op.get('op') == 'add' and op.get('tag') != TAG:
                self.base.append((float(op['x']), float(op['z']), BASE_RADIUS.get(Path(op['asset']).stem, BASE_RADIUS_DEFAULT)))
        self.base_xyz = np.array([(x, z) for x, z, _ in self.base]) if self.base else np.zeros((0, 2))
        self.base_r = np.array([r for _, _, r in self.base]) if self.base else np.zeros(0)
        displaceable = [Path(assets[p[0]]).stem in DISPLACEABLE for p in self.manifest['placements']]
        self.base_displaceable = np.array(displaceable + [False] * (len(self.base) - len(displaceable)), dtype=bool)
        self.base_alive = np.ones(len(self.base), dtype=bool)
        # the defence must keep its ground: garrison posts and the encounter beats
        keep = [(g['x'], g['z']) for g in self.edits['canyon_course'].get('garrison', [])]
        for beat in self.edits.get('street_encounter', {}).get('beats', []):
            keep.extend((float(p[0]), float(p[1])) for p in beat.get('points', []))
        self.keep = np.array(keep) if keep else np.zeros((0, 2))
        # cache of the route floor height by fraction
        self.floor = {}

    def ground(self, x, z):
        gx, gz = x / self.spacing + self.half, z / self.spacing + self.half
        ix, iz = int(math.floor(gx)), int(math.floor(gz))
        if not (0 <= ix < self.H.shape[1] - 1 and 0 <= iz < self.H.shape[0] - 1):
            return float('nan')
        u, v = gx - ix, gz - iz
        H = self.H
        return float((1 - v) * ((1 - u) * H[iz, ix] + u * H[iz, ix + 1]) + v * ((1 - u) * H[iz + 1, ix] + u * H[iz + 1, ix + 1]))

    def slope(self, x, z, step=2.5):
        dx = (self.ground(x + step, z) - self.ground(x - step, z)) / (2 * step)
        dz = (self.ground(x, z + step) - self.ground(x, z - step)) / (2 * step)
        return dx, dz

    def route_floor(self, f):
        key = round(f, 3)
        if key not in self.floor:
            x, z, _, _ = self.route.at(key)
            self.floor[key] = self.ground(x, z)
        return self.floor[key]


# ---------------------------------------------------------------------------------------------------
# the layer
# ---------------------------------------------------------------------------------------------------
class Layer:
    def __init__(self, ws, noise, density):
        self.ws, self.rnd, self.density = ws, noise, density
        self.ops, self.scenes, self.placed = [], [], []    # placed: (x, z, r)
        self.counts = collections.Counter()
        self.rejected = collections.Counter()

    # -- geometry helpers ---------------------------------------------------------------------------
    @staticmethod
    def yaw_for(key, tx, tz):
        """Yaw (degrees) that lays the asset's long axis along the (tx, tz) direction."""
        axis = ASSETS[key][1]
        return math.degrees(math.atan2(-tz, tx) if axis == 'x' else math.atan2(tx, tz))

    @staticmethod
    def forward(key, yaw_deg):
        """World direction of the asset's long axis for a yaw."""
        a = math.radians(yaw_deg)
        return (math.cos(a), -math.sin(a)) if ASSETS[key][1] == 'x' else (math.sin(a), math.cos(a))

    def ok(self, x, z, r, lane=LANE_HALF_M, slope_max=0.30, floor_band=FLOOR_BAND_M, keep=7.0, loose=False, footprint=None):
        """Can an item of mutual-spacing radius r stand here?

        `footprint` = (fx, fz, half_length, half_width) tests the item's oriented rectangle against
        buildings and the base dressing instead of a circle, so a hull may stand among scattered
        kit and beside a tree without anything entering it.
        """
        ws = self.ws
        g = ws.ground(x, z)
        if math.isnan(g):
            self.rejected['off_map'] += 1
            return None
        dist, f = ws.route.nearest(x, z)
        if dist < lane + r:
            self.rejected['lane'] += 1
            return None
        if abs(g - ws.route_floor(f)) > floor_band:
            self.rejected['floor_band'] += 1
            return None
        sx, sz = ws.slope(x, z)
        if math.hypot(sx, sz) > slope_max:
            self.rejected['slope'] += 1
            return None
        if math.hypot(x - ws.spawn[0], z - ws.spawn[1]) < SPAWN_CLEAR_M + r or math.hypot(x - ws.goal[0], z - ws.goal[1]) < GOAL_CLEAR_M + r:
            self.rejected['spawn_goal'] += 1
            return None
        if footprint:
            fx, fz, hl, hw = footprint
            corners = [(x, z)] + [(x + fx * a * hl - fz * b * hw, z + fz * a * hl + fx * b * hw) for a in (-1, 1) for b in (-1, 1)]
            clear = 0.8
        else:
            corners = [(x, z)]
            clear = r + 1.5
        for x0, x1, z0, z1 in ws.rects:
            for cx, cz in corners:
                if max(x0 - cx, 0.0, cx - x1) ** 2 + max(z0 - cz, 0.0, cz - z1) ** 2 < clear ** 2:
                    self.rejected['building'] += 1
                    return None
        displaced = []
        if len(ws.base_xyz):
            if footprint:
                fx, fz, hl, hw = footprint
                q = ws.base_xyz - (x, z)
                u = q[:, 0] * fx + q[:, 1] * fz
                v = -q[:, 0] * fz + q[:, 1] * fx
                hit = (np.abs(u) < hl + ws.base_r + 0.3) & (np.abs(v) < hw + ws.base_r + 0.3) & ws.base_alive
                if np.any(hit & ~ws.base_displaceable):
                    self.rejected['base_prop'] += 1
                    return None
                displaced = [int(i) for i in np.flatnonzero(hit)]
            else:
                d2 = ((ws.base_xyz - (x, z)) ** 2).sum(1)
                if np.any((d2 < (ws.base_r + r) ** 2) & ws.base_alive):
                    self.rejected['base_prop'] += 1
                    return None
        if len(ws.keep) and np.any(((ws.keep - (x, z)) ** 2).sum(1) < (keep + r) ** 2):
            self.rejected['defence'] += 1
            return None
        if not loose:
            for px, pz, pr in self.placed:
                if (px - x) ** 2 + (pz - z) ** 2 < (pr + r) ** 2:
                    self.rejected['overlap'] += 1
                    return None
        return displaced

    def add(self, key, scene, x, z, yaw, scale=1.0, sink=0.0, r=None, tilt=True, lane=LANE_HALF_M, slope_max=0.30, loose=False):
        """Place one asset if the ground allows it; returns the op or None.

        `loose` items (ground scars, drifts, a parapet in front of its gun) are composed against
        earlier placements of this layer on purpose and skip the mutual-spacing test.
        """
        path, axis, length, width, radius = ASSETS[key]
        sc = scale if isinstance(scale, (int, float)) else max(scale)
        r = (radius * sc) if r is None else r
        footprint = None
        if length >= 2.5:
            fx, fz = self.forward(key, yaw)
            footprint = (fx, fz, length * sc * 0.5, width * sc * 0.5)
        displaced = self.ok(x, z, r, lane=lane, slope_max=slope_max, loose=loose, footprint=footprint)
        if displaced is None:
            return None
        for i in displaced:
            self.ws.base_alive[i] = False
            self.ops.append({'op': 'delete', 'i': i, 'tag': TAG, 'scene': scene, 'under': key})
            self.counts['displaced_base_props'] += 1
        y = self.ws.ground(x, z) + sink
        op = {'op': 'add', 'asset': path, 'x': r3(x), 'y': r3(y), 'z': r3(z), 'deg': round(yaw % 360.0, 2),
              's': ([r3(v) for v in scale] if isinstance(scale, (list, tuple)) else r3(scale)),
              'tag': TAG, 'scene': scene}
        if tilt and key in TILTED:
            fx, fz = self.forward(key, yaw)
            rx, rz = (-fz, fx) if axis == 'z' else (fz, -fx)   # the asset's local +X in the world
            # local +X / +Z lie along (rx, rz) / (fx, fz); pitch raises the local +Z nose, roll the +X side
            sx, sz = self.ws.slope(x, z, step=max(length, width) * 0.5)
            along = sx * fx + sz * fz
            across = sx * rx + sz * rz
            if axis == 'x':
                along, across = across, along
            pitch = max(-9.0, min(9.0, math.degrees(math.atan(along))))
            roll = max(-9.0, min(9.0, math.degrees(math.atan(across))))
            if abs(pitch) > 0.5 or abs(roll) > 0.5:
                op['tilt_deg'] = [round(pitch, 2), round(roll, 2)]
        self.ops.append(op)
        if not loose:
            self.placed.append((x, z, r))
        self.counts[key] += 1
        return op

    def horse(self, key, scene, x, z, yaw, lane=LANE_HALF_M - 1.0):
        """A dead draught horse where the traces put it, shuffled a little when kit is in the way."""
        fx, fz = self.forward(key, yaw)
        for du, dv in ((0, 0), (0, 1.0), (0, -1.0), (0.9, 0), (0.9, 1.2), (0.9, -1.2)):
            op = self.add(key, scene, x + fx * du - fz * dv, z + fz * du + fx * dv, yaw, 1.0, -0.05, lane=lane, slope_max=0.3)
            if op:
                return op
        return None

    def vehicle(self, key, scene, ff, lateral, yaw, sink, r_lane, slope_max, salt):
        """Place a vehicle beside the route, sliding sideways and along it when the first spot is taken."""
        rv = self.ws.route
        radius = ASSETS[key][4]
        # Houses stand as close as 10 m to the centreline along the settlements, so a stalled vehicle
        # slides along the shoulder and outward (never into the lane) until it finds open ground.
        for df in (0.0, 5.0, -5.0, 10.0, -10.0, 16.0, -16.0, 22.0, -22.0):
            for dl in (0.0, 3.0, 6.0, -2.0, 9.0):
                lat = lateral + math.copysign(dl, lateral)
                x, z = rv.world(ff + df / rv.L, lat)
                op = self.add(key, scene, x, z, yaw, 1.0, sink, r=radius, lane=LANE_HALF_M + r_lane - radius, slope_max=slope_max)
                if op:
                    return op, x, z
        return None, None, None

    def scatter(self, scene, cx, cz, keys, n, r0, r1, salt, lane=LANE_HALF_M, sink=-0.02, slope_max=0.42):
        """Scatter n small items around a point within an annulus; returns how many landed."""
        landed = 0
        for k in range(n * 4):
            if landed >= n:
                break
            a = self.rnd(scene, salt + k * 7) * math.tau
            rr = r0 + (r1 - r0) * math.sqrt(self.rnd(scene, salt + k * 7 + 1))
            key = self.rnd.pick(scene, salt + k * 7 + 2, keys)
            x, z = cx + math.cos(a) * rr, cz + math.sin(a) * rr
            if self.add(key, scene, x, z, self.rnd(scene, salt + k * 7 + 3) * 360.0, 1.0, sink, lane=lane, slope_max=slope_max):
                landed += 1
        return landed

    def casualties(self, scene, cx, cz, n, r0, r1, salt, lane=LANE_HALF_M):
        landed = 0
        for k in range(n * 5):
            if landed >= n:
                break
            a = self.rnd(scene, salt + k * 5) * math.tau
            rr = r0 + (r1 - r0) * math.sqrt(self.rnd(scene, salt + k * 5 + 1))
            x, z = cx + math.cos(a) * rr, cz + math.sin(a) * rr
            # no floating figures: the body spans two metres
            hs = [self.ws.ground(x + dx, z + dz) for dx, dz in [(-.8, 0), (.8, 0), (0, -.8), (0, .8)]]
            if any(math.isnan(h) for h in hs) or max(hs) - min(hs) > .30:
                continue
            key = f'casualty_{(k + salt) % 6}'
            if self.add(key, scene, x, z, self.rnd(scene, salt + k * 5 + 2) * 360.0, 1.0, -0.045, lane=lane, slope_max=0.3):
                landed += 1
        return landed

    def drift(self, scene, key, x, z, yaw, salt, lane=LANE_HALF_M):
        """A wind-packed drift in the lee of a placed vehicle."""
        length, width = ASSETS[key][2], ASSETS[key][3]
        fx, fz = self.forward(key, yaw)
        # the lee side: downwind of the hull, offset along the hull by a little
        lee = (length * 0.25 + width * 0.5 + 0.9)
        shift = (self.rnd(scene, salt) - 0.5) * length * 0.6
        dx, dz = x + WIND[0] * lee + fx * shift, z + WIND[1] * lee + fz * shift
        s = (1.6 + self.rnd(scene, salt + 1) * 1.6, 1.4 + self.rnd(scene, salt + 2) * 1.2, 1.0 + self.rnd(scene, salt + 3) * 0.5)
        return self.add('drift', scene, dx, dz, math.degrees(math.atan2(fx, fz)), list(s), -0.05, r=0.3, tilt=False, lane=lane, slope_max=0.5, loose=True)

    def crater(self, scene, x, z, salt, scale=None, lane=LANE_HALF_M):
        s = scale or (1.6 + self.rnd(scene, salt) * 1.8)
        op = self.add('crater', scene, x, z, self.rnd(scene, salt + 1) * 360.0, s, -0.03, tilt=False, lane=lane, slope_max=0.45)
        if op and self.rnd(scene, salt + 2) < 0.6:
            self.add('scorched', scene, x + (self.rnd(scene, salt + 3) - .5) * s, z + (self.rnd(scene, salt + 4) - .5) * s,
                     self.rnd(scene, salt + 5) * 360.0, s * 1.3, -0.02, r=0.2, tilt=False, lane=lane, slope_max=0.5, loose=True)
        return op

    def count(self, n):
        return max(0, int(round(n * self.density)))

    # -- scenes -------------------------------------------------------------------------------------
    def armour_column(self, j, f, side, retry=True):
        """A platoon's worth of tanks knocked out in column along one side of the road."""
        scene = f'{TAG}.armour.{j:02d}'
        rv = self.ws.route.at
        soviet = j % 7 in (1, 3, 6)
        n = self.count(3 + int(self.rnd(scene, 1) * 4))
        lateral = side * (LANE_HALF_M + 3.0 + self.rnd(scene, 2) * 8.0)
        placed = []
        ff = f
        for k in range(n):
            key_f = scene + f'.{k}'
            ff += (14.0 + self.rnd(key_f, 3) * 10.0) / self.ws.route.L
            _, _, tx, tz = rv(ff)
            lat = lateral + (self.rnd(key_f, 4) - .5) * 5.0
            burnt = self.rnd(key_f, 5) < 0.6
            key = ('kv1' if soviet else 'pz3') + ('_burnt' if burnt else '_detail')
            slewed = k == int(self.rnd(scene, 6) * n)
            yaw = self.yaw_for(key, tx, tz) + (self.rnd(key_f, 7) - .5) * (150.0 if slewed else 36.0) + (180.0 if self.rnd(key_f, 8) < .5 else 0.0)
            r_lane = ASSETS[key][2] * 0.5 if slewed else ASSETS[key][3] * 0.5 + 0.6
            op, x, z = self.vehicle(key, scene, ff, lat, yaw, -0.10, r_lane, 0.28, k)
            if not op:
                continue
            placed.append((key, x, z, yaw, burnt))
            if burnt:
                self.add('scorched', scene, x + (self.rnd(key_f, 9) - .5) * 2, z + (self.rnd(key_f, 10) - .5) * 2,
                         self.rnd(key_f, 11) * 360.0, 3.6 + self.rnd(key_f, 12) * 1.4, -0.03, r=0.2, tilt=False, slope_max=0.5, loose=True)
            if self.rnd(key_f, 13) < 0.65:
                a = self.rnd(key_f, 14) * math.tau
                rr = 7.0 + self.rnd(key_f, 15) * 8.0
                self.crater(scene, x + math.cos(a) * rr, z + math.sin(a) * rr, 20 + k * 9)
            self.drift(scene, key, x, z, yaw, 40 + k * 5)
            if self.rnd(key_f, 16) < 0.5:
                self.drift(scene, key, x, z, yaw, 60 + k * 5)
            self.casualties(scene, x, z, int(self.rnd(key_f, 17) * 3), 4.0, 9.0, 80 + k * 11)
            self.scatter(scene, x, z, ['jerrycan', 'crate_a', 'ammo_box', 'tyre', 'board'], int(self.rnd(key_f, 18) * 3), 3.5, 7.5, 120 + k * 13)
        if not placed and retry:
            return self.armour_column(j, f, -side, retry=False)
        self.scenes.append(dict(id=scene, kind='armour_column', fraction=r3(f), side=side,
                                nationality='soviet' if soviet else 'german', vehicles=len(placed)))

    def retreat_line(self, j, f, side):
        """Soft-skin column in echelon on one side; carts, sledges, dead horses and kit on the other."""
        scene = f'{TAG}.retreat.{j:02d}'
        rv = self.ws.route
        n_lorry = self.count(2 + int(self.rnd(scene, 1) * 3))
        ff = f
        lorries = 0
        for k in range(n_lorry):
            key_f = scene + f'.l{k}'
            ff += (9.0 + self.rnd(key_f, 2) * 5.0) / rv.L
            _, _, tx, tz = rv.at(ff)
            lat = side * (LANE_HALF_M + 2.4 + self.rnd(key_f, 3) * 4.0)
            key = 'lorry_burnt' if self.rnd(key_f, 4) < 0.45 else 'lorry_detail'
            yaw = self.yaw_for(key, tx, tz) + side * (12.0 + self.rnd(key_f, 5) * 26.0) + (180.0 if self.rnd(key_f, 6) < .25 else 0.0)
            op, x, z = self.vehicle(key, scene, ff, lat, yaw, -0.06, 2.2, 0.26, k)
            if not op:
                continue
            lorries += 1
            if key == 'lorry_burnt':
                self.add('scorched', scene, x, z, yaw, 3.2, -0.03, r=0.2, tilt=False, slope_max=0.5, loose=True)
            self.drift(scene, key, x, z, yaw, 30 + k * 4)
            self.scatter(scene, x, z, ['jerrycan', 'crate_b', 'barrel', 'luggage', 'board', 'tyre'], 1 + int(self.rnd(key_f, 7) * 3), 3.0, 6.0, 50 + k * 9)
        # the other side of the road: a cart or two with the horses dead in their traces, kit on the snow
        n_cart = self.count(1 + int(self.rnd(scene, 8) * 3))
        ff = f + 6.0 / rv.L
        carts = horses = 0
        kit_sites = []
        for k in range(n_cart):
            key_f = scene + f'.c{k}'
            ff += (8.0 + self.rnd(key_f, 2) * 7.0) / rv.L
            _, _, tx, tz = rv.at(ff)
            key = self.rnd.pick(key_f, 4, ['cart', 'cart', 'sledge', 'sledge_over'])
            yaw = self.yaw_for(key, tx, tz) + (self.rnd(key_f, 5) - .5) * 50.0
            op, x, z = self.vehicle(key, scene, ff, -side * (LANE_HALF_M + 2.0 + self.rnd(key_f, 3) * 6.0), yaw, -0.04, 1.2, 0.3, k)
            if not op:
                continue
            carts += 1
            fx, fz = self.forward(key, yaw)
            for h in range(1 + int(self.rnd(key_f, 6) * 1.6)):
                hx, hz = x + fx * (4.3 + h * 1.3) + (-fz) * (h * 1.1 - .5), z + fz * (4.3 + h * 1.3) + fx * (h * 1.1 - .5)
                hkey = 'horse_a' if self.rnd(key_f, 7 + h) < .5 else 'horse_b'
                if self.horse(hkey, scene, hx, hz, yaw + (self.rnd(key_f, 9 + h) - .5) * 40.0):
                    horses += 1
            self.drift(scene, key, x, z, yaw, 70 + k * 4)
            kit_sites.append((x, z, 3 + int(self.rnd(key_f, 12) * 5), 90 + k * 17))
        for x, z, n, salt in kit_sites:
            self.scatter(scene, x, z, KIT, n, 2.5, 8.0, salt, lane=LANE_HALF_M + 0.3)
        self.casualties(scene, *rv.world(f + 20.0 / rv.L, -side * 18.0), 1 + int(self.rnd(scene, 13) * 3), 2.0, 12.0, 140)
        if self.rnd(scene, 14) < 0.7:
            x, z, _, _ = rv.at(f + 25.0 / rv.L)
            x, z = rv.world(f + 25.0 / rv.L, side * (26.0 + self.rnd(scene, 15) * 20.0))
            self.crater(scene, x, z, 150)
        self.scenes.append(dict(id=scene, kind='retreat_line', fraction=r3(f), side=side, lorries=lorries, carts=carts, horses=horses))

    def gun_position(self, j, f, side):
        """An overrun 45 mm position: parapet, the gun, its crew, ammunition and counter-battery craters."""
        scene = f'{TAG}.gun.{j:02d}'
        rv = self.ws.route
        x, z, tx, tz = rv.at(f)
        lat = side * (22.0 + self.rnd(scene, 1) * 16.0)
        x, z = rv.world(f, lat)
        # the gun lays back along the approach (toward where the Tiger comes from), slewed a little
        facing = self.yaw_for('gun45', -tx, -tz) + (self.rnd(scene, 2) - .5) * 40.0 + side * 18.0
        fx, fz = self.forward('gun45', facing)
        gun = None
        for dl in (0.0, 5.0, -5.0, 10.0, 15.0):
            x, z = rv.world(f, lat + side * dl)
            gun = self.add('gun45', scene, x, z, facing + (self.rnd(scene, 3) - .5) * 20.0, 1.0, -0.05, r=1.7, slope_max=0.3)
            if gun:
                break
        parapet = gun and self.add('parapet', scene, x + fx * 2.2, z + fz * 2.2, facing, 1.0, -0.08, r=3.6, slope_max=0.26, loose=True)
        if not gun:
            self.scenes.append(dict(id=scene, kind='gun_position', fraction=r3(f), side=side, placed=False))
            return
        # ammunition and the crew's kit behind the gun; the crew around it
        bx, bz = x - fx * 3.0, z - fz * 3.0
        self.scatter(scene, bx, bz, ['ammo_box', 'ammo_box', 'crate_a', 'crate_b', 'jerrycan', 'board'], 4 + int(self.rnd(scene, 4) * 4), 0.8, 4.0, 20)
        self.casualties(scene, x, z, 2 + int(self.rnd(scene, 5) * 3), 1.6, 6.5, 40)
        if self.rnd(scene, 6) < 0.6:
            self.add('wheel', scene, x - fx * 1.5 + fz * 2.4, z - fz * 1.5 - fx * 2.4, self.rnd(scene, 7) * 360.0, 1.0, -0.02, tilt=False, slope_max=0.5)
        self.scatter(scene, x, z, ['rifle', 'rifle', 'coat_a', 'coat_b'], 2 + int(self.rnd(scene, 8) * 3), 1.5, 6.0, 60)
        for k in range(1 + int(self.rnd(scene, 9) * 2)):
            a = self.rnd(scene, 80 + k) * math.tau
            rr = 5.0 + self.rnd(scene, 90 + k) * 9.0
            self.crater(scene, x + math.cos(a) * rr, z + math.sin(a) * rr, 100 + k * 7)
        self.drift(scene, 'gun45', x, z, facing, 120)
        self.scenes.append(dict(id=scene, kind='gun_position', fraction=r3(f), side=side, placed=True, parapet=bool(parapet)))

    def strike_field(self, j, f):
        """A stonk: several craters with scorched rims in a loose group off the road."""
        scene = f'{TAG}.strikes.{j:02d}'
        rv = self.ws.route
        side = -1 if self.rnd(scene, 1) < .5 else 1
        cx, cz = rv.world(f, side * (LANE_HALF_M + 4.0 + self.rnd(scene, 2) * 36.0))
        n = self.count(3 + int(self.rnd(scene, 3) * 4))
        craters = 0
        for k in range(n * 3):
            if craters >= n:
                break
            a = self.rnd(scene, 10 + k * 3) * math.tau
            rr = math.sqrt(self.rnd(scene, 11 + k * 3)) * 24.0
            if self.crater(scene, cx + math.cos(a) * rr, cz + math.sin(a) * rr, 100 + k * 7):
                craters += 1
        if self.rnd(scene, 4) < 0.35:
            self.casualties(scene, cx, cz, 1 + int(self.rnd(scene, 5) * 2), 2.0, 14.0, 200)
        self.scatter(scene, cx, cz, ['chip', 'chip', 'board', 'plank'], int(self.rnd(scene, 6) * 4), 2.0, 16.0, 240, slope_max=0.5)
        self.scenes.append(dict(id=scene, kind='strike_field', fraction=r3(f), side=side, craters=craters))

    def dissolved_unit(self, j, f, side):
        """Where a unit came apart: dropped rifles, greatcoats, boxes, a sledge, the dead."""
        scene = f'{TAG}.dissolved.{j:02d}'
        rv = self.ws.route
        cx, cz = rv.world(f, side * (LANE_HALF_M + 6.0 + self.rnd(scene, 1) * 22.0))
        _, _, tx, tz = rv.at(f)
        rifles = self.scatter(scene, cx, cz, ['rifle'], self.count(8 + int(self.rnd(scene, 2) * 9)), 1.0, 16.0, 20, slope_max=0.5)
        coats = self.scatter(scene, cx, cz, ['coat_a', 'coat_b'], self.count(5 + int(self.rnd(scene, 3) * 6)), 1.0, 15.0, 60, slope_max=0.5)
        kit = self.scatter(scene, cx, cz, ['crate_a', 'luggage', 'scorched_luggage', 'jerrycan', 'ammo_box', 'board'], self.count(3 + int(self.rnd(scene, 4) * 4)), 2.0, 14.0, 100)
        dead = self.casualties(scene, cx, cz, self.count(3 + int(self.rnd(scene, 5) * 4)), 2.0, 14.0, 140)
        key = 'sledge_over' if self.rnd(scene, 6) < .5 else 'sledge'
        yaw = self.yaw_for(key, tx, tz) + (self.rnd(scene, 7) - .5) * 90.0
        sx, sz = cx + (self.rnd(scene, 8) - .5) * 10.0, cz + (self.rnd(scene, 9) - .5) * 10.0
        if self.add(key, scene, sx, sz, yaw, 1.0, -0.04, slope_max=0.3):
            fx, fz = self.forward(key, yaw)
            self.horse('horse_a' if self.rnd(scene, 10) < .5 else 'horse_b', scene, sx + fx * 4.4, sz + fz * 4.4, yaw + (self.rnd(scene, 11) - .5) * 40.0, lane=LANE_HALF_M)
            self.drift(scene, key, sx, sz, yaw, 160)
        self.scenes.append(dict(id=scene, kind='dissolved_unit', fraction=r3(f), side=side, rifles=rifles, greatcoats=coats, kit=kit, dead=dead))

    def frozen_horses(self, j, f, side):
        """A supply column halted forever: carts and sledges in line, horses dead in harness, one butchered."""
        scene = f'{TAG}.horses.{j:02d}'
        rv = self.ws.route
        n = self.count(5)
        ff = f
        carts = horses = 0
        kit_sites = []
        for k in range(n):
            key_f = scene + f'.{k}'
            ff += (10.5 + self.rnd(key_f, 1) * 3.0) / rv.L
            _, _, tx, tz = rv.at(ff)
            lat = side * (LANE_HALF_M + 3.5 + self.rnd(key_f, 2) * 3.0)
            key = self.rnd.pick(key_f, 3, ['cart', 'cart', 'cart', 'sledge'])
            yaw = self.yaw_for(key, tx, tz) + (self.rnd(key_f, 4) - .5) * 16.0
            op, x, z = self.vehicle(key, scene, ff, lat, yaw, -0.04, 1.2, 0.28, k)
            if not op:
                continue
            carts += 1
            fx, fz = self.forward(key, yaw)
            hx, hz = x + fx * 4.4, z + fz * 4.4
            if self.horse('horse_a' if k % 2 else 'horse_b', scene, hx, hz, yaw + (self.rnd(key_f, 5) - .5) * 24.0):
                horses += 1
                if k == 1:
                    # the feeding site: a small fire scar, a box, trodden snow drawn in by the scorched ring
                    self.add('scorched', scene, hx - fz * 2.6, hz + fx * 2.6, 0.0, 1.1, -0.02, r=0.2, tilt=False, slope_max=0.5, loose=True)
                    self.scatter(scene, hx - fz * 2.6, hz + fx * 2.6, ['crate_b', 'jerrycan', 'board'], 2, 0.8, 2.4, 70)
            self.drift(scene, key, x, z, yaw, 90 + k * 4)
            kit_sites.append((x, z, int(self.rnd(key_f, 6) * 3), 110 + k * 9))
        for x, z, n, salt in kit_sites:
            self.scatter(scene, x, z, ['luggage', 'board', 'coat_a', 'barrel'], n, 2.2, 6.0, salt)
        self.casualties(scene, *rv.world(f + 14.0 / rv.L, side * 24.0), 1 + int(self.rnd(scene, 7) * 2), 1.0, 10.0, 150)
        self.scenes.append(dict(id=scene, kind='frozen_horses', fraction=r3(f), side=side, carts=carts, horses=horses))

    def lone_vehicle(self, j, f, side):
        """A single vehicle left where it stopped: out of fuel, ditched or hit; kit dropped beside it."""
        scene = f'{TAG}.lone.{j:02d}'
        rv = self.ws.route
        _, _, tx, tz = rv.at(f)
        key = self.rnd.pick(scene, 1, ['lorry_detail', 'lorry_burnt', 'cart', 'cart', 'sledge', 'sledge_over',
                                        'pz3_burnt', 'kv1_burnt', 'pz3_detail', 'lorry_detail', 'cart'])
        lat = side * (LANE_HALF_M + 2.5 + self.rnd(scene, 2) * 14.0)
        slew = (self.rnd(scene, 3) - .5) * 90.0
        yaw = self.yaw_for(key, tx, tz) + slew
        r_lane = ASSETS[key][2] * 0.5 if abs(slew) > 25 else ASSETS[key][3] * 0.5 + 0.6
        op, x, z = self.vehicle(key, scene, f, lat, yaw, -0.08 if key.startswith(('pz3', 'kv1')) else -0.05, r_lane, 0.28, 0)
        if not op:
            self.scenes.append(dict(id=scene, kind='lone_vehicle', fraction=r3(f), side=side, placed=False))
            return
        if key.endswith('burnt'):
            self.add('scorched', scene, x, z, yaw, 3.4, -0.03, r=0.2, tilt=False, slope_max=0.5, loose=True)
        if key in ('cart', 'sledge', 'sledge_over') and self.rnd(scene, 4) < 0.7:
            fx, fz = self.forward(key, yaw)
            self.horse('horse_a' if self.rnd(scene, 5) < .5 else 'horse_b', scene, x + fx * 4.4, z + fz * 4.4, yaw + (self.rnd(scene, 6) - .5) * 30.0)
        self.drift(scene, key, x, z, yaw, 20)
        self.scatter(scene, x, z, KIT, 2 + int(self.rnd(scene, 7) * 4), 2.5, 7.0, 40)
        self.casualties(scene, x, z, int(self.rnd(scene, 8) * 3), 2.5, 8.0, 60)
        if self.rnd(scene, 9) < 0.5:
            a = self.rnd(scene, 10) * math.tau
            self.crater(scene, x + math.cos(a) * 9.0, z + math.sin(a) * 9.0, 80)
        self.scenes.append(dict(id=scene, kind='lone_vehicle', fraction=r3(f), side=side, placed=True, vehicle=key))

    def roadside_scatter(self, j, f):
        """The continuous litter of a retreat along both shoulders: kit, rifles, coats, the dead, scars."""
        scene = f'{TAG}.roadside.{j:03d}'
        rv = self.ws.route
        for side in (-1, 1):
            if self.rnd(scene, side + 2) < 0.35:
                continue
            x, z = rv.world(f, side * (LANE_HALF_M + 1.0 + self.rnd(scene, side + 4) * 16.0))
            roll = self.rnd(scene, side + 6)
            if roll < 0.55:
                self.scatter(scene, x, z, KIT, 1 + int(self.rnd(scene, side + 8) * 3), 0.0, 5.0, 20 + side * 7)
            elif roll < 0.72:
                self.casualties(scene, x, z, 1 + int(self.rnd(scene, side + 8) * 2), 0.0, 6.0, 40 + side * 7)
            elif roll < 0.86:
                self.crater(scene, x, z, 60 + side * 7)
            else:
                self.add('scorched', scene, x, z, self.rnd(scene, side + 9) * 360.0, 1.4 + self.rnd(scene, side + 10) * 2.0, -0.02, r=0.3, tilt=False, slope_max=0.5)

    def road_edge_drifts(self):
        """Loose drifts along the road's shoulders, so the snow itself reads as having moved."""
        scene = f'{TAG}.drifts'
        rv = self.ws.route
        n = self.count(110)
        for k in range(n):
            f = 0.05 + (k + self.rnd(scene, k)) * 0.90 / n
            side = -1 if k % 2 else 1
            x, z, tx, tz = rv.at(f)
            x, z = rv.world(f, side * (LANE_HALF_M + 1.2 + self.rnd(scene, 100 + k) * 2.5))
            s = (2.0 + self.rnd(scene, 200 + k) * 2.4, 1.3 + self.rnd(scene, 300 + k) * 1.4, 1.1)
            self.add('drift', scene, x, z, math.degrees(math.atan2(tx, tz)) + (self.rnd(scene, 400 + k) - .5) * 30.0, list(s), -0.05, r=0.3, tilt=False, lane=LANE_HALF_M, slope_max=0.5)

    def series(self, kind, spacing_m, start=0.05, end=0.97):
        """Site fractions every `spacing_m` metres of route (jittered), scaled by the density."""
        L = self.ws.route.L
        n = max(1, self.count((end - start) * L / spacing_m))
        return [start + (j + 0.25 + 0.5 * self.rnd(f'{TAG}.{kind}.site.{j}', 1)) * (end - start) / n for j in range(n)]

    def build(self):
        """A continuous aftermath along the whole run: the big set pieces interleave every 150-250 m,
        lone vehicles fill the gaps and the shoulders carry the litter of the retreat throughout."""
        series = [
            ('armour', self.series('armour', 240.0, 0.06), 'armour_column'),
            ('retreat', self.series('retreat', 205.0, 0.045), 'retreat_line'),
            ('gun', self.series('gun', 430.0, 0.10), 'gun_position'),
            ('dissolved', self.series('dissolved', 650.0, 0.15), 'dissolved_unit'),
            ('horses', self.series('horses', 1500.0, 0.22), 'frozen_horses'),
            ('lone', self.series('lone', 115.0, 0.05), 'lone_vehicle'),
        ]
        for kind, fractions, method in series:
            for j, f in enumerate(fractions):
                side = -1 if (j + len(kind) + int(self.rnd(f'{TAG}.{kind}.side.{j}', 1) * 2)) % 2 else 1
                getattr(self, method)(j, f, side)
        for j, f in enumerate(self.series('strikes', 160.0, 0.05)):
            self.strike_field(j, f)
        for j, f in enumerate(self.series('roadside', 24.0, 0.045)):
            self.roadside_scatter(j, f)
        self.road_edge_drifts()


# ---------------------------------------------------------------------------------------------------
# cameras and headless stills
# ---------------------------------------------------------------------------------------------------
CAMERA_FRACTIONS = [('start', 0.08), ('approach', 0.20), ('settlement', 0.42), ('factory', 0.60),
                    ('ridge', 0.80), ('descent', 0.90), ('final', 0.96)]


def recon_cameras(ws):
    """Recon cameras on the route itself, 6 m up, looking 70 m along the travel direction."""
    rv = ws.route
    cams = []
    for name, f in CAMERA_FRACTIONS:
        ex, ez, _, _ = rv.at(f - 30.0 / rv.L)
        tx, tz, _, _ = rv.at(f + 40.0 / rv.L)
        yaw = math.degrees(math.atan2(tx - ex, -(tz - ez)))
        cams.append(dict(name=name, fraction=f, x=round(ex, 1), y=round(ws.ground(ex, ez) + 6.0, 1), z=round(ez, 1),
                         yaw=round(yaw, 1), pitch=-7.0, fog=0.0))
    return cams


def run_shots(ws, prefix, renderer):
    """Headless stills for every recon camera, clear sunset and blizzard, through the launcher's profile."""
    root = ws.root
    exe = root / 'target' / 'release' / ('panzerrechnersystem.exe' if os.name == 'nt' else 'panzerrechnersystem')
    if not exe.is_file():
        raise SystemExit(f'release binary missing: {exe}')
    runtime = root / 'artifacts' / 'runtime'
    env = {k: v for k, v in os.environ.items() if not k.startswith('KESSEL_')}
    env.setdefault('HOME', os.environ.get('USERPROFILE', str(Path.home())))
    for name in ('state', 'cache', 'config'):
        (runtime / name).mkdir(parents=True, exist_ok=True)
        env[f'XDG_{name.upper()}_HOME'] = str(runtime / name)
    env.update(KESSEL_ASSET_ROOT=str(root), KESSEL_RENDERER=renderer, KESSEL_GRAPHICS_PRESET='high',
               KESSEL_INTERNAL_RES='800x450', KESSEL_DITHER_PATTERN='screen_bayer8', KESSEL_SOURCE_VISIBILITY='maps',
               KESSEL_TRANSPORT_QUALITY='full', KESSEL_RECON_MANIFEST=str(ws.dest / 'manifest.v1.json'),
               KESSEL_SHOT_CANYON='1', KESSEL_SKY_UTC='1942-12-21T12:40:00Z')
    written = []
    for cam in recon_cameras(ws):
        for weather, fog in (('clear', 0.0), ('blizzard', 0.004)):
            out = f'{prefix}_{cam["name"]}_{weather}.png'
            e = dict(env, KESSEL_SHOT_WEATHER='clear' if weather == 'clear' else 'storm')
            args = [str(exe), 'screenshot', out, str(cam['x']), str(cam['y']), str(cam['z']), str(cam['yaw']), str(cam['pitch']), str(fog)]
            log = root / 'captures' / f'{prefix}_{cam["name"]}_{weather}.log'
            with open(log, 'w') as fh:
                res = subprocess.run(args, cwd=root, env=e, stdout=fh, stderr=subprocess.STDOUT)
            text = log.read_text(errors='replace')
            demand = next((line.strip() for line in text.splitlines() if '[rt] TLAS demand' in line), '')
            pages = next((line.strip() for line in text.splitlines() if 'placements in' in line), '')
            print(json.dumps(dict(shot=out, exit=res.returncode, tlas=demand, world=pages)), flush=True)
            if res.returncode == 0:
                written.append(out)
    return written


# ---------------------------------------------------------------------------------------------------
# apply / remove / report
# ---------------------------------------------------------------------------------------------------
def sha256(data):
    return hashlib.sha256(data).hexdigest()


def strip(edits):
    before = len(edits.get('ops', []))
    edits['ops'] = [op for op in edits.get('ops', []) if op.get('tag') != TAG]
    edits.pop(TAG, None)
    return before - len(edits['ops'])


def report(edits):
    ops = [op for op in edits.get('ops', []) if op.get('tag') == TAG]
    counts = collections.Counter(Path(op['asset']).name for op in ops if op.get('op') == 'add')
    counts['displaced_base_props'] = sum(1 for op in ops if op.get('op') == 'delete')
    return dict(sorted(counts.items(), key=lambda kv: (-kv[1], kv[0])))


def apply(root, dest, seed, density, remove=False):
    ws = Workshop(root, dest)
    edits_path = ws.dest / 'manifest_edits.json'
    original = edits_path.read_bytes()
    backup = ws.root / 'artifacts' / 'authoring' / 'canyon-v2-wreckage' / 'manifest_edits.before.json'
    if not backup.exists():
        backup.parent.mkdir(parents=True, exist_ok=True)
        backup.write_bytes(original)
    removed = strip(ws.edits)
    if ws.edits.get('base_placements') != len(ws.manifest['placements']):
        raise SystemExit('manifest_edits.json base_placements does not match the manifest; refusing to overlay a stale base')
    result = dict(removed_previous_ops=removed)
    if not remove:
        layer = Layer(ws, Noise(seed), density)
        layer.build()
        ws.edits['ops'].extend(layer.ops)
        counts = report(ws.edits)
        ws.edits[TAG] = dict(
            schema='kessel.canyon-v2-wreckage', tool=TOOL, tool_version=VERSION, seed=seed, density=density,
            authored='2026-10-27', ops=len(layer.ops), lane_half_m=LANE_HALF_M, floor_band_m=FLOOR_BAND_M,
            placement_rules=('valley floor within the route floor band, bounded slope, edge clearance beyond the '
                             'travel lane, clear of buildings, base props, garrison and encounter points, spawn and goal; '
                             'vehicles tilt with the terrain gradient (<= 9 degrees)'),
            scenes=layer.scenes, counts=counts, rejected=dict(layer.rejected),
            props_receipt=BF + 'wreckage-props.receipt.json', provenance=PROVENANCE)
        result.update(ops=len(layer.ops), scenes=len(layer.scenes), counts=counts, rejected=dict(layer.rejected))
    text = json.dumps(ws.edits, indent=1, ensure_ascii=False, sort_keys=True) + '\n'
    edits_path.write_text(text, encoding='utf-8')
    receipt = dict(schema='panzer.canyon-v2-wreckage.receipt', tool=TOOL, version=VERSION, seed=seed, density=density,
                   removed=remove, source_edits_sha256=sha256(original), edits_sha256=sha256(text.encode('utf-8')),
                   manifest_sha256=sha256((ws.dest / 'manifest.v1.json').read_bytes()), result=result)
    (ws.dest / 'wreckage.receipt.json').write_text(json.dumps(receipt, indent=1, sort_keys=True) + '\n')
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2], help='asset root (the checkout holding assets/ and the release binary)')
    parser.add_argument('--dest', type=Path, default=None, help='workshop folder (default: ROOT/' + DEST + ')')
    parser.add_argument('--seed', type=lambda v: int(v, 0), default=DEFAULT_SEED)
    parser.add_argument('--density', type=float, default=1.0, help='scales the site counts (1.0 = authored)')
    parser.add_argument('--remove', action='store_true', help='strip the layer and its block from manifest_edits.json')
    parser.add_argument('--report', action='store_true', help='print per-asset counts of the current layer and exit')
    parser.add_argument('--cameras', action='store_true', help='print the recon cameras as screenshot arguments and exit')
    parser.add_argument('--shots', metavar='PREFIX', help='render the recon cameras headless (clear + blizzard) into captures/')
    parser.add_argument('--renderer', default='shader', choices=['shader', 'raymarch'])
    args = parser.parse_args()
    dest = args.dest or (args.root / DEST)
    if args.report:
        print(json.dumps(report(json.loads((dest / 'manifest_edits.json').read_text())), indent=1))
        return
    if args.cameras:
        for cam in recon_cameras(Workshop(args.root, dest)):
            print(f"{cam['name']:12s} {cam['x']} {cam['y']} {cam['z']} {cam['yaw']} {cam['pitch']} {cam['fog']}")
        return
    if args.shots:
        written = run_shots(Workshop(args.root, dest), args.shots, args.renderer)
        print(json.dumps(dict(written=written)))
        return
    receipt = apply(args.root, dest, args.seed, args.density, args.remove)
    print(json.dumps(receipt['result'], indent=1))


if __name__ == '__main__':
    main()
