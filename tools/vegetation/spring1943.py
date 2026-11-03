#!/usr/bin/env python3
"""Spring 1943 vegetation: trees, shrubs and war-damaged trees for a May steppe river town in
Ukraine / southern Russia, generated procedurally as low-poly GLBs.

    python3 tools/vegetation/spring1943.py [OUT_DIR]       # default $KESSEL_ASSET_ROOT/assets/vegetation/spring1943
    python3 tools/vegetation/spring1943.py OUT_DIR --only tree_oak_a,shrub_hazel_b

Representation (chosen for the raymarch backend, docs/VEGETATION_SPRING1943.md):
- foliage is geometry, not alpha-tested cards. The raymarch G-buffer alpha-tests only the grass
  kind and its shadow/BVH programs treat every other triangle as opaque, so a textured leaf card
  would shadow as a solid quad. Each "spray" is a small irregular six-triangle polygon (0.2-0.9 m,
  a twig's worth of leaves) whose outline is the cut-out; sprays gather in clumps on the crown's
  shell with sky between the clumps, every clump fed by a twig from a scaffold limb, every limb
  from the trunk, so what casts a shadow is exactly what is drawn.
- materials are constant glTF factors (the loader merges them into a nearest one-row atlas):
  leaf reflectance from PROSPECT-D (tools/vegetation/leaf_optics.py, the same model and lanes as
  the meadow's grass, src/leaf_optics.rs) with spring-leaf pigment estimates; bark, charcoal,
  fresh splinters, soil and petals from measured-range estimates named in MATERIALS below.
  Leaf transmittance is NOT carried: ordinary GLB surfaces shade as opaque (one face reflects),
  an approximation logged in docs/PHYSICAL_TRUTH_AUDIT.md.
- frame: engine Y up, metres, origin at the trunk's ground contact (the trunk runs 0.25 m below
  to sit into slopes). Equivalent to the Blender bakes' glTF Y-up export
  (engine = (blender_x, blender_z, -blender_y)).
- names: standing trees start with `tree_` so src/trees.rs fields them as destructible live trees
  (trunk contact measured from this mesh at the bow's height); `fallen_`, `stump_` and `shrub_`
  are static dressing.

Everything is deterministic: seeds are CRC32s of the asset name; no global RNG state.
"""
import json
import math
import os
import random
import sys
import zlib

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from glb_writer import write_glb  # noqa: E402
from leaf_optics import Leaf, lanes  # noqa: E402

TOOL = 'tools/vegetation/spring1943.py'
VERSION = 1

# ── materials ───────────────────────────────────────────────────────────────────────────────────
# Leaf pigments: young May leaves, estimates inside Jacquemoud & Ustin (2019) ranges (Cab lower
# than summer, thin blades). Three tones per species: Cab x0.8 / x1.0 / x1.2.
LEAVES = {
    'birch': Leaf(1.5, 26.0, 7.0, 0.0, 0.0, 0.009, 0.0040),
    'poplar': Leaf(1.6, 36.0, 9.0, 0.0, 0.0, 0.012, 0.0050),
    'willow': Leaf(1.9, 28.0, 8.0, 0.0, 0.0, 0.010, 0.0055),   # silky-haired: more scattering (N)
    'linden': Leaf(1.5, 30.0, 8.0, 0.0, 0.0, 0.011, 0.0045),
    'chestnut': Leaf(1.6, 38.0, 10.0, 0.0, 0.0, 0.013, 0.0060),
    'oak': Leaf(1.6, 22.0, 7.0, 1.5, 0.0, 0.009, 0.0050),      # young oak leaves carry anthocyanin
    'pear': Leaf(1.7, 36.0, 9.0, 0.0, 0.0, 0.012, 0.0060),
    'apple': Leaf(1.6, 30.0, 8.0, 0.0, 0.0, 0.011, 0.0050),
    'cherry': Leaf(1.6, 33.0, 8.0, 0.6, 0.0, 0.011, 0.0050),
    'hazel': Leaf(1.5, 28.0, 7.0, 0.0, 0.0, 0.010, 0.0045),
    'elder': Leaf(1.5, 34.0, 8.0, 0.0, 0.0, 0.012, 0.0045),
    'hawthorn': Leaf(1.6, 32.0, 8.0, 0.0, 0.0, 0.010, 0.0050),
    'dogrose': Leaf(1.6, 34.0, 8.0, 0.3, 0.0, 0.010, 0.0050),
    'blackthorn': Leaf(1.7, 36.0, 9.0, 0.0, 0.0, 0.010, 0.0055),
    'willowscrub': Leaf(1.9, 26.0, 7.0, 0.0, 0.0, 0.010, 0.0055),
}
SCORCHED = Leaf(2.0, 3.0, 3.0, 0.0, 1.6, 0.001, 0.0060)   # heat-killed, browned, dried
WILTED = Leaf(1.8, 8.0, 4.0, 0.0, 0.5, 0.003, 0.0055)     # cut off from water a few days
LEAF_ROUGHNESS = 0.45   # waxy cuticle (src/leaf_optics.rs uses 0.4 for grass)

# Non-leaf albedos, linear RGB. Estimates from published reflectance ranges: birch periderm is
# bright (~0.45-0.55 visible), furrowed bark of oak/poplar/linden 0.08-0.15, charcoal 0.03-0.05,
# freshly split sapwood 0.35-0.5, moist chernozem 0.06-0.10, white petals 0.6-0.7.
MATERIALS = {
    'bark_birch': ((0.50, 0.49, 0.46), 0.85),
    'bark_birch_dark': ((0.055, 0.050, 0.046), 0.95),
    'bark_poplar': ((0.115, 0.105, 0.090), 0.95),
    'bark_willow': ((0.125, 0.112, 0.092), 0.95),
    'bark_linden': ((0.105, 0.092, 0.078), 0.92),
    'bark_chestnut': ((0.095, 0.078, 0.062), 0.92),
    'bark_oak': ((0.085, 0.074, 0.062), 0.95),
    'bark_pear': ((0.070, 0.060, 0.052), 0.95),
    'bark_apple': ((0.110, 0.090, 0.072), 0.92),
    'bark_cherry': ((0.115, 0.062, 0.050), 0.75),
    'bark_hazel': ((0.120, 0.090, 0.068), 0.85),
    'bark_elder': ((0.135, 0.120, 0.100), 0.92),
    'bark_hawthorn': ((0.095, 0.085, 0.075), 0.92),
    'bark_dogrose': ((0.080, 0.110, 0.045), 0.80),
    'bark_blackthorn': ((0.040, 0.036, 0.034), 0.85),
    'bark_willowscrub': ((0.140, 0.100, 0.055), 0.80),
    'charcoal': ((0.034, 0.031, 0.029), 0.95),
    'bark_scorched': ((0.055, 0.043, 0.034), 0.95),
    'wood_splinter': ((0.46, 0.37, 0.25), 0.85),
    'wood_stripped': ((0.30, 0.25, 0.18), 0.90),
    'soil': ((0.085, 0.068, 0.052), 1.00),
    'root': ((0.070, 0.058, 0.046), 0.95),
    'petal_white': ((0.66, 0.65, 0.60), 0.60),
    'petal_apple': ((0.70, 0.56, 0.60), 0.60),
    'petal_candle': ((0.64, 0.61, 0.52), 0.60),
    'petal_elder': ((0.60, 0.58, 0.46), 0.65),
}


def leaf_material(key, tone):
    leaf = {'scorched': SCORCHED, 'wilted': WILTED}.get(key) or LEAVES[key]
    if key not in ('scorched', 'wilted'):
        leaf = leaf._replace(cab=leaf.cab * (0.8, 1.0, 1.2)[tone])
    rho, _tau = lanes(leaf)
    return rho, LEAF_ROUGHNESS


# ── species ─────────────────────────────────────────────────────────────────────────────────────
# height range m; bole = crown base / height; crown = crown radius / height; shape of the crown
# envelope; excurrent = how far up the crown the trunk runs as a leader (0 forks at the crown
# base, 1 to the top); angle = limb angle from vertical (deg); bow > 0 limbs arch and droop at the
# tips, < 0 they sag and turn up; weep = sprays hang; spray = spray size m at 15 m; elong = spray
# aspect; clumps / per = clumps and sprays per clump; limbs; trunk_d at breast height for 15 m.
# Leaf length (m) and base width / length per species, about 3 x the real blade (birch 6 cm,
# oak 10 cm, lime 9 cm, a horse chestnut leaflet 15 cm): one leaf spans a few pixels at 20-40 m and
# the triangle budget still fills the crown (leaf area = triangles x length^2 x width / 2).
LEAF_SIZE = {
    'birch': (0.18, 0.60), 'poplar': (0.25, 0.75), 'willow': (0.26, 0.24), 'linden': (0.27, 0.80),
    'chestnut': (0.38, 0.38), 'oak': (0.30, 0.50), 'pear': (0.18, 0.55), 'apple': (0.20, 0.55),
    'cherry': (0.24, 0.45), 'hazel': (0.24, 0.80), 'elder': (0.22, 0.40), 'hawthorn': (0.14, 0.70),
    'dogrose': (0.12, 0.60), 'blackthorn': (0.12, 0.50), 'willowscrub': (0.22, 0.25),
}
LEAVES_PER_TWIG = 4
SHRUB_TWIGS_PER_LEAF_GROUP = 4   # twigs of leaves per former spray of a shrub clump

TREES = {
    'birch': dict(height=(14, 18), bole=0.30, crown=0.20, shape='ovoid', excurrent=0.92, angle=42, bow=0.35,
                  weep=0.75, spray=0.85, elong=1.6, clumps=36, per=12, limbs=10, trunk_d=0.32, lean=0.04,
                  wander=0.10, lobe=0.15, bark='bark_birch'),
    'poplar': dict(height=(20, 26), bole=0.28, crown=0.30, shape='irregular', excurrent=0.78, angle=34, bow=-0.15,
                   weep=0.0, spray=1.0, elong=1.2, clumps=42, per=12, limbs=8, trunk_d=0.95, lean=0.06,
                   wander=0.18, lobe=0.35, bark='bark_poplar'),
    'willow': dict(height=(12, 16), bole=0.20, crown=0.38, shape='round', excurrent=0.45, angle=32, bow=0.25,
                   weep=0.35, spray=0.85, elong=1.9, clumps=38, per=12, limbs=8, trunk_d=0.7, lean=0.14,
                   wander=0.20, lobe=0.25, stems=(1, 3), bark='bark_willow'),
    'linden': dict(height=(15, 20), bole=0.24, crown=0.28, shape='ovoid', excurrent=0.62, angle=50, bow=0.15,
                   weep=0.0, spray=1.0, elong=1.1, clumps=40, per=12, limbs=9, trunk_d=0.55, lean=0.02,
                   wander=0.08, lobe=0.12, bark='bark_linden'),
    'chestnut': dict(height=(15, 19), bole=0.22, crown=0.34, shape='round', excurrent=0.45, angle=55, bow=-0.20,
                     weep=0.0, spray=1.1, elong=1.0, clumps=38, per=11, limbs=9, trunk_d=0.6, lean=0.02,
                     wander=0.10, lobe=0.15, bark='bark_chestnut', candles=0.6),
    'oak': dict(height=(15, 21), bole=0.24, crown=0.42, shape='spread', excurrent=0.35, angle=62, bow=0.0,
                weep=0.0, spray=1.0, elong=1.15, clumps=42, per=12, limbs=8, trunk_d=0.8, lean=0.04,
                wander=0.40, lobe=0.30, bark='bark_oak'),
    'pear': dict(height=(8, 12), bole=0.18, crown=0.24, shape='cone', excurrent=0.85, angle=34, bow=-0.10,
                 weep=0.0, spray=1.0, elong=1.2, clumps=30, per=11, limbs=8, trunk_d=0.3, lean=0.05,
                 wander=0.20, lobe=0.20, bark='bark_pear'),
    'apple': dict(height=(4.2, 6.0), bole=0.30, crown=0.45, shape='spread', excurrent=0.20, angle=60, bow=0.10,
                  weep=0.0, spray=1.0, elong=1.1, clumps=28, per=11, limbs=6, trunk_d=0.30, lean=0.08,
                  wander=0.30, lobe=0.25, bark='bark_apple', blossom=('petal_apple', 0.10)),
    'cherry': dict(height=(5.5, 8.0), bole=0.30, crown=0.40, shape='round', excurrent=0.50, angle=45, bow=0.05,
                   weep=0.0, spray=1.0, elong=1.2, clumps=28, per=11, limbs=6, trunk_d=0.24, lean=0.05,
                   wander=0.20, lobe=0.20, bark='bark_cherry'),
}

SHRUBS = {
    # height m, width m, stems, spread rad from vertical, arch (tip droop), stem_d m, clumps per
    # stem, sprays per clump, spray m, elong, blossom (material, share)
    'hazel': dict(height=(3.0, 4.5), stems=(7, 11), spread=0.45, arch=0.15, stem_d=0.05, clumps=2, per=5,
                  spray=0.40, elong=1.0, bark='bark_hazel'),
    'elder': dict(height=(2.8, 4.0), stems=(4, 7), spread=0.55, arch=0.35, stem_d=0.07, clumps=2, per=6,
                  spray=0.42, elong=1.3, bark='bark_elder', blossom=('petal_elder', 0.10)),
    'hawthorn': dict(height=(2.2, 3.8), stems=(5, 8), spread=0.60, arch=0.20, stem_d=0.06, clumps=3, per=4,
                     spray=0.30, elong=1.0, bark='bark_hawthorn', blossom=('petal_white', 0.35)),
    'dogrose': dict(height=(1.4, 2.4), stems=(6, 10), spread=0.80, arch=0.85, stem_d=0.025, clumps=2, per=4,
                    spray=0.28, elong=1.4, bark='bark_dogrose'),
    'blackthorn': dict(height=(1.6, 3.0), stems=(8, 12), spread=0.50, arch=0.10, stem_d=0.035, clumps=2, per=4,
                       spray=0.24, elong=1.0, bark='bark_blackthorn'),
    'willowscrub': dict(height=(2.2, 4.0), stems=(9, 14), spread=0.35, arch=0.10, stem_d=0.03, clumps=2, per=3,
                        spray=0.32, elong=2.6, bark='bark_willowscrub', leaf='willowscrub'),
}


# ── geometry ────────────────────────────────────────────────────────────────────────────────────
def unit(v):
    n = np.linalg.norm(v)
    return v / n if n > 1e-9 else np.array([0.0, 1.0, 0.0])


UP = np.array([0.0, 1.0, 0.0])


class Mesh:
    def __init__(self):
        self.parts = {}

    def _part(self, mat):
        return self.parts.setdefault(mat, ([], [], []))

    def tris(self):
        return sum(len(i) // 3 for _p, _n, i in self.parts.values())

    def tube(self, pts, radii, sides, mats, cap=None, rng=None):
        """A tapered prism along a polyline; `mats` a material or one per segment. Segments carry
        their own rings so a segment can change material. cap: None, 'flat' or 'jagged' (a broken
        end in fresh wood)."""
        pts = [np.asarray(p, dtype=float) for p in pts]
        if isinstance(mats, str):
            mats = [mats] * (len(pts) - 1)
        tang = []
        for i in range(len(pts)):
            a, b = pts[max(i - 1, 0)], pts[min(i + 1, len(pts) - 1)]
            tang.append(unit(b - a))
        ref = np.array([1.0, 0.0, 0.0]) if abs(tang[0][0]) < 0.9 else np.array([0.0, 0.0, 1.0])
        u = unit(np.cross(tang[0], ref))
        frames = []
        for i, t in enumerate(tang):
            u = unit(u - t * np.dot(u, t))
            frames.append((u, np.cross(t, u)))
        ring = []
        for i, p in enumerate(pts):
            u, v = frames[i]
            ring.append([(p + radii[i] * (math.cos(a) * u + math.sin(a) * v), math.cos(a) * u + math.sin(a) * v)
                         for a in (2 * math.pi * k / sides for k in range(sides))])
        for s in range(len(pts) - 1):
            P, N, I = self._part(mats[s])
            base = len(P)
            for k in range(sides):
                P.append(ring[s][k][0]); N.append(ring[s][k][1])
            for k in range(sides):
                P.append(ring[s + 1][k][0]); N.append(ring[s + 1][k][1])
            for k in range(sides):
                k2 = (k + 1) % sides
                I += [base + k, base + k2, base + sides + k2, base + k, base + sides + k2, base + sides + k]
        if cap:
            t = tang[-1]
            r = radii[-1]
            P, N, I = self._part('wood_splinter' if cap == 'jagged' else mats[-1])
            base = len(P)
            for k in range(sides):
                lift = (rng.uniform(0.0, 2.2) * r) if cap == 'jagged' else 0.0
                P.append(ring[-1][k][0] + t * lift); N.append(t)
            P.append(pts[-1] + t * (rng.uniform(0.3, 1.2) * r if cap == 'jagged' else 0.0)); N.append(t)
            for k in range(sides):
                I += [base + k, base + (k + 1) % sides, base + sides]

    def cone(self, a, b, r, sides, mat):
        """An open cone from a ring of radius r at a to the apex b (splinters, candles, roots)."""
        a, b = np.asarray(a, float), np.asarray(b, float)
        t = unit(b - a)
        ref = np.array([1.0, 0.0, 0.0]) if abs(t[0]) < 0.9 else np.array([0.0, 0.0, 1.0])
        u = unit(np.cross(t, ref)); v = np.cross(t, u)
        P, N, I = self._part(mat)
        base = len(P)
        for k in range(sides):
            ang = 2 * math.pi * k / sides
            d = math.cos(ang) * u + math.sin(ang) * v
            P.append(a + r * d); N.append(unit(d + t * 0.3))
        P.append(b); N.append(t)
        for k in range(sides):
            I += [base + k, base + (k + 1) % sides, base + sides]

    def spray(self, c, n, along, size, elong, mat, rng, cup=0.16):
        """A twig's leaves: an irregular hexagon, cupped, its outline the cut-out."""
        n = unit(n)
        a = unit(along - n * np.dot(along, n))
        b = np.cross(n, a)
        P, N, I = self._part(mat)
        base = len(P)
        P.append(np.asarray(c, float)); N.append(n)
        for k in range(6):
            ang = 2 * math.pi * k / 6 + rng.uniform(-0.35, 0.35)
            rad = size * 0.5 * rng.uniform(0.55, 1.0)
            x, y = math.cos(ang) * rad * elong, math.sin(ang) * rad
            q = c + a * x + b * y - n * cup * size * (rad / (size * 0.5)) ** 2
            P.append(q); N.append(unit(n + 0.45 * unit(a * x + b * y)))
        for k in range(6):
            I += [base, base + 1 + k, base + 1 + (k + 1) % 6]

    def twig(self, p, n, length, width, leaves, mat, rng, droop=0.0):
        """A shoot's leaves: `leaves` single-triangle leaves alternating along a short shoot from p,
        each `length` long and `width` x length wide at its base, tip outward (one triangle each)."""
        n = unit(n)
        ref = np.array([1.0, 0.0, 0.0]) if abs(n[0]) < 0.9 else np.array([0.0, 0.0, 1.0])
        a = unit(np.cross(n, ref)); b = np.cross(n, a)
        P, N, I = self._part(mat)
        phase = rng.uniform(0, 2 * math.pi)
        # Leaves alternate along a short shoot rather than radiating from one point (no stars).
        shoot = unit(math.cos(phase) * a + math.sin(phase) * b + n * 0.3)
        for k in range(leaves):
            th = phase + math.pi * (k % 2) + (k // 2) * 0.9 + rng.uniform(-0.6, 0.6)
            d = unit(math.cos(th) * a + math.sin(th) * b + shoot * 0.8 + n * rng.uniform(-0.3, 0.5) - UP * droop)
            side = unit(np.cross(n, d))
            ll = length * rng.uniform(0.7, 1.25)
            base = p + shoot * length * 0.45 * k + d * 0.015
            w = 0.5 * width * ll
            lift = n * 0.12 * ll   # the blade arches off the twig
            base_i = len(P)
            P += [base + side * w, base - side * w, p + d * ll + lift]
            nn = unit(n + d * 0.25)
            N += [nn, nn, nn]
            I += [base_i, base_i + 1, base_i + 2]

    def disc(self, c, n, radius, rim, thick, mat, rng):
        """An irregular slab (a root plate): two fans and a side band."""
        n = unit(n)
        ref = np.array([1.0, 0.0, 0.0]) if abs(n[0]) < 0.9 else np.array([0.0, 0.0, 1.0])
        u = unit(np.cross(n, ref)); v = np.cross(n, u)
        rads = [radius * rng.uniform(0.7, 1.05) for _ in range(rim)]
        P, N, I = self._part(mat)
        for side, sgn in ((0, 1.0), (1, -1.0)):
            base = len(P)
            P.append(c + n * sgn * thick * 0.5); N.append(n * sgn)
            for k in range(rim):
                ang = 2 * math.pi * k / rim
                P.append(c + rads[k] * (math.cos(ang) * u + math.sin(ang) * v) + n * sgn * thick * 0.35); N.append(n * sgn)
            for k in range(rim):
                I += [base, base + 1 + k, base + 1 + (k + 1) % rim]
        base = len(P)
        for k in range(rim):
            ang = 2 * math.pi * k / rim
            d = math.cos(ang) * u + math.sin(ang) * v
            P.append(c + rads[k] * d + n * thick * 0.35); N.append(d)
            P.append(c + rads[k] * d - n * thick * 0.35); N.append(d)
        for k in range(rim):
            k2 = (k + 1) % rim
            I += [base + 2 * k, base + 2 * k + 1, base + 2 * k2 + 1, base + 2 * k, base + 2 * k2 + 1, base + 2 * k2]

    def transform(self, fn_p, fn_n):
        for mat, (P, N, I) in self.parts.items():
            for i in range(len(P)):
                P[i] = fn_p(P[i]); N[i] = fn_n(N[i])

    def bounds(self):
        allp = np.array([p for P, _n, _i in self.parts.values() for p in P])
        return allp.min(axis=0), allp.max(axis=0)


def envelope(shape, t):
    t = min(max(t, 0.0), 1.0)
    if shape == 'ovoid':
        return math.sin(math.pi * t ** 0.85) ** 0.75
    if shape == 'round':
        return math.sqrt(max(0.0, 1.0 - (2 * t - 1) ** 2)) ** 0.8
    if shape == 'spread':
        x = (t - 0.4) / (0.6 if t > 0.4 else 0.4)
        return math.sqrt(max(0.0, 1.0 - x * x)) ** 0.7
    if shape == 'cone':
        return min(1.0, 2.0 * (1.0 - t)) * math.sqrt(min(1.0, t * 5.0))
    if shape == 'irregular':
        return math.sin(math.pi * t ** 0.9) ** 0.7
    raise ValueError(shape)


def seeded(name):
    return random.Random(zlib.crc32(name.encode()))


def leaf_key(species):
    return species


# ── trees ───────────────────────────────────────────────────────────────────────────────────────
def build_tree(name, species, scale=1.0, damage=None, mesh=None, keep_leaves=True):
    """Returns (mesh, info). damage: None, 'burnt', 'shattered'."""
    sp = TREES[species]
    rng = seeded(name)
    m = mesh or Mesh()
    H = rng.uniform(*sp['height']) * scale
    hb = sp['bole'] * H
    R = sp['crown'] * H
    size_k = math.sqrt(H / 15.0)
    spray_m = max(0.42, 0.21 * R) * sp['spray']   # a spray scales with the crown it fills
    r_trunk = 0.5 * sp['trunk_d'] * (H / (0.5 * sum(sp['height']))) * rng.uniform(0.9, 1.1)
    bark = sp['bark']
    lean_az = rng.uniform(0, 2 * math.pi)
    lean = np.array([math.cos(lean_az), 0.0, math.sin(lean_az)]) * sp['lean'] * rng.uniform(0.5, 1.2)
    leader_top = hb + (H - hb) * sp['excurrent']
    wph = [rng.uniform(0, 6.3) for _ in range(4)]

    def axis(h):
        w = sp['wander'] * min(h / H, 1.0)
        return np.array([lean[0] * h + w * math.sin(h * 0.45 + wph[0]) * 1.2 + w * 0.6 * math.sin(h * 1.1 + wph[1]),
                         h,
                         lean[2] * h + w * math.sin(h * 0.5 + wph[2]) * 1.2 + w * 0.6 * math.sin(h * 1.3 + wph[3])])

    def r_at(h):
        return max(r_trunk * (1.0 - 0.78 * min(max(h, 0.0) / max(leader_top, 0.1), 1.0)), 0.035)

    # damage plans
    burnt = damage == 'burnt'
    shattered = damage == 'shattered'
    snap_h = H * rng.uniform(0.32, 0.68) if shattered else None
    char_h = H * rng.uniform(0.35, 0.75) if burnt else None
    fire_az = rng.uniform(0, 2 * math.pi)

    def bark_for(h, limb=False):
        if burnt:
            if h < char_h:
                return 'charcoal' if rng.random() < 0.85 else 'bark_scorched'
            return 'bark_scorched' if rng.random() < 0.6 else bark
        if shattered and h > snap_h * 0.45 and rng.random() < 0.35:
            return 'wood_stripped'
        if bark == 'bark_birch':
            # Dark fissured base and scattered dark bands on the white periderm.
            if h < 1.2 + 0.6 * rng.random():
                return 'bark_birch_dark'
            return 'bark_birch_dark' if (limb and rng.random() < 0.35) or rng.random() < 0.12 else 'bark_birch'
        return bark

    # trunk(s): willows fork low into 1-3 stems
    stems = rng.randint(*sp.get('stems', (1, 1)))
    trunk_top = snap_h if shattered else leader_top
    fork_h = min(H * 0.17, trunk_top * 0.6) if stems > 1 else None
    nseg = 6
    top_h = min(trunk_top, fork_h) if fork_h else trunk_top
    hs = [-0.25 + (top_h + 0.25) * i / nseg for i in range(nseg + 1)]
    pts = [axis(h) for h in hs]
    radii = [r_at(max(h, 0.0)) * (1.3 if i == 0 else 1.0) for i, h in enumerate(hs)]
    end_cap = 'jagged' if (shattered and not fork_h) else ('flat' if not fork_h else None)
    m.tube(pts, radii, 7 if r_trunk > 0.3 else 6, [bark_for(h) for h in hs[:-1]], cap=end_cap, rng=rng)
    stem_paths = []
    if fork_h:
        for s in range(stems):
            az = lean_az + (s - (stems - 1) / 2) * 1.6 + rng.uniform(-0.3, 0.3)
            out = np.array([math.cos(az), 0.0, math.sin(az)])
            sh = [fork_h + (trunk_top - fork_h) * i / 4 for i in range(5)]
            spts = [axis(fork_h) + out * ((h - fork_h) * 0.35) + UP * 0 for h in sh]
            spts = [np.array([p[0], h, p[2]]) for p, h in zip(spts, sh)]
            srad = [r_at(h) * (0.8 if stems > 1 else 1.0) for h in sh]
            m.tube(spts, srad, 5, [bark_for(h) for h in sh[:-1]], cap='jagged' if shattered else 'flat', rng=rng)
            stem_paths.append(spts)
    if shattered:
        # The splintered top: spikes of fresh wood round the break.
        tops = [p[-1] for p in stem_paths] or [pts[-1]]
        for top in tops:
            rr = r_at(trunk_top)
            for k in range(rng.randint(4, 7)):
                a = 2 * math.pi * k / 6 + rng.uniform(-0.4, 0.4)
                d = np.array([math.cos(a), 0.0, math.sin(a)])
                base = top + d * rr * 0.6 - UP * rng.uniform(0.1, 0.5)
                tip = top + d * rr * rng.uniform(0.5, 1.6) + UP * rng.uniform(0.4, 1.4) * max(rr / 0.25, 0.6)
                m.cone(base, tip, rr * rng.uniform(0.25, 0.45), 3, 'wood_splinter')

    def nearest_axis_point(h):
        if stem_paths:
            # the stem whose top is nearest in azimuth decides later; use main axis below fork
            return axis(min(h, fork_h))
        return axis(min(h, trunk_top))

    # clumps on the crown shell
    lobes = [(rng.uniform(0.3, 1.0), rng.uniform(0, 6.3), k) for k in (1, 2, 3)]

    def lobe(az):
        return 1.0 + sp['lobe'] * sum(a * math.cos(k * az + ph) for a, ph, k in lobes) / 1.5

    clumps = []
    target = int(sp['clumps'] * (0.85 + 0.3 * rng.random()))
    tries = 0
    while len(clumps) < target and tries < target * 40:
        tries += 1
        t = rng.uniform(0.04, 0.97)
        e = envelope(sp['shape'], t)
        if rng.random() > max(e, 0.12):
            continue
        az = rng.uniform(0, 2 * math.pi)
        rr = rng.uniform(0.55, 1.0) ** 0.6
        h = hb + (H - hb) * t
        rad = R * e * lobe(az) * rr
        c = axis(min(h, leader_top)) + np.array([math.cos(az), 0.0, math.sin(az)]) * rad
        c[1] = h
        clumps.append(dict(c=c, t=t, az=az, rr=rr, out=np.array([math.cos(az), 0.0, math.sin(az)])))
    # scaffold limbs: k-means of the clumps' directions
    L = sp['limbs']
    cents = [clumps[i]['c'].copy() for i in rng.sample(range(len(clumps)), min(L, len(clumps)))]
    for _ in range(8):
        groups = [[] for _ in cents]
        for i, cl in enumerate(clumps):
            rel = cl['c']
            j = min(range(len(cents)), key=lambda j: np.sum(((rel - cents[j]) * np.array([1.0, 0.7, 1.0])) ** 2))
            groups[j].append(i)
        cents = [np.mean([clumps[i]['c'] for i in g], axis=0) if g else cents[j] for j, g in enumerate(groups)]
    per_limb = len(clumps) / max(len(cents), 1)
    limb_tips = {}
    kept_clumps = []
    for j, g in enumerate(groups):
        if not g:
            continue
        tgt = cents[j]
        # pull the limb's end inside the shell so twigs radiate to the clumps
        ax = nearest_axis_point(tgt[1])
        horiz = np.array([tgt[0] - ax[0], 0.0, tgt[2] - ax[2]])
        hd = np.linalg.norm(horiz)
        tgt = ax + horiz * 0.72 + UP * (tgt[1] - ax[1])
        attach_h = tgt[1] - hd * 0.72 / math.tan(math.radians(sp['angle']))
        attach_h = min(max(attach_h, hb * 0.85), (trunk_top if not stem_paths else trunk_top) * 0.97)
        if stem_paths:
            sp_path = min(stem_paths, key=lambda p: np.linalg.norm((p[-1] - tgt) * np.array([1, 0, 1])))
            hh = min(max(attach_h, sp_path[0][1]), sp_path[-1][1])
            f = (hh - sp_path[0][1]) / max(sp_path[-1][1] - sp_path[0][1], 1e-3)
            base = sp_path[0] + (sp_path[-1] - sp_path[0]) * f
            if attach_h < fork_h:
                base = axis(attach_h)
        else:
            base = axis(attach_h)
        if shattered and attach_h > snap_h - 0.2:
            continue   # the limb went with the crown
        d = tgt - base
        dist = np.linalg.norm(d)
        mid = base + d * 0.5 + UP * sp['bow'] * dist * 0.35
        if sp['wander'] > 0.25:
            mid = mid + np.array([rng.uniform(-1, 1), rng.uniform(-0.5, 0.5), rng.uniform(-1, 1)]) * dist * 0.12
        q1 = base + (mid - base) * 0.5 + UP * sp['bow'] * dist * 0.05
        path = [base, q1, mid, tgt]
        r0 = min(r_at(attach_h) * 0.62 * math.sqrt(len(g) / per_limb), r_at(attach_h) * 0.85)
        r0 = max(r0, 0.05)
        rad = [r0, r0 * 0.72, r0 * 0.45, max(r0 * 0.22, 0.03)]
        if shattered:
            # broken limb: keep a stub, sometimes hanging
            f = rng.uniform(0.15, 0.6)
            cut = base + (mid - base) * (f * 2) if f < 0.5 else mid + (tgt - mid) * ((f - 0.5) * 2)
            mats = [bark_for(attach_h, True)]
            m.tube([base, cut], [r0, r0 * (1 - 0.5 * f)], 4, mats, cap='jagged', rng=rng)
            if rng.random() < 0.35:
                hang = cut + unit(np.array([d[0], 0, d[2]])) * 0.3 - UP * rng.uniform(1.0, 2.2) * size_k
                hang[1] = max(hang[1], 2.2)   # clear of the bow's height (src/trees.rs trunk section)
                m.tube([cut, hang], [r0 * 0.45, 0.03], 3, bark_for(attach_h, True), cap='jagged', rng=rng)
                if rng.random() < 0.5 and keep_leaves:
                    for _ in range(rng.randint(12, 30)):
                        c = hang + np.array([rng.uniform(-.5, .5), rng.uniform(-.3, .5), rng.uniform(-.5, .5)])
                        m.twig(c, unit(np.array([rng.uniform(-1, 1), 0.8, rng.uniform(-1, 1)])),
                               LEAF_SIZE[species][0], LEAF_SIZE[species][1], rng.randint(3, 5), ('wilted', 0), rng)
            elif rng.random() < 0.3 and keep_leaves:
                # a few leaves holding on at a stub
                for _ in range(rng.randint(12, 36)):
                    c = cut + np.array([rng.uniform(-.6, .6), rng.uniform(-.1, .6), rng.uniform(-.6, .6)])
                    m.twig(c, unit(np.array([rng.uniform(-1, 1), 1.0, rng.uniform(-1, 1)])),
                           LEAF_SIZE[species][0], LEAF_SIZE[species][1], rng.randint(3, 5),
                           (leaf_key(species), rng.randint(0, 2)), rng)
            continue
        m.tube(path, rad, 5 if r0 > 0.12 else 4, [bark_for(p[1], True) for p in path[:-1]], rng=rng)
        for i in g:
            kept_clumps.append((i, path))

    # twigs and leaves: leaf triangles in proportion to the crown radius, 4,000-11,000
    leaf_len, leaf_w = LEAF_SIZE[species]
    leaf_tris = min(11000, max(4000, int(1400 * R)))
    twigs_per_clump = max(6, leaf_tris // LEAVES_PER_TWIG // max(len(kept_clumps), 1))
    weep = sp['weep']
    leafless = 0.0
    if burnt:
        leafless = rng.uniform(0.6, 0.85)
    for i, path in kept_clumps:
        cl = clumps[i]
        c = cl['c']
        # nearest point on the limb's outer half
        best = min((path[k] + (path[k + 1] - path[k]) * f for k in (1, 2) for f in (0.0, 0.5, 1.0)),
                   key=lambda p: np.linalg.norm(p - c))
        twig_r = 0.028 * size_k
        tw_mat = bark_for(c[1], True) if not burnt else ('charcoal' if c[1] < char_h else 'bark_scorched')
        if np.linalg.norm(c - best) > 0.35:
            m.tube([best, c], [twig_r * 1.4, twig_r * 0.6], 3, tw_mat, rng=rng)
        side = math.cos(cl['az'] - fire_az)
        bare = burnt and (rng.random() < leafless + 0.25 * side - 0.25 * cl['t'])
        if bare:
            # bare twigs where the leaves burnt off
            for _ in range(3):
                d = unit(cl['out'] * 0.8 + UP * rng.uniform(0.2, 1.0) + np.array([rng.uniform(-.6, .6), 0, rng.uniform(-.6, .6)]))
                m.tube([c, c + d * rng.uniform(0.6, 1.3) * size_k], [twig_r * 0.6, 0.008], 3, tw_mat, rng=rng)
            continue
        cr = spray_m * 1.3 * rng.uniform(0.85, 1.2)
        n_sprays = twigs_per_clump + rng.randint(-3, 3)

        def tone_of():
            return 0 if (cl['rr'] > 0.85 and rng.random() < 0.6) else (2 if rng.random() < 0.3 else 1)
        n_twigs = n_sprays if not burnt else max(3, n_sprays // 3)
        for _ in range(n_twigs):
            d = unit(np.array([rng.gauss(0, 1), rng.gauss(0, 0.8), rng.gauss(0, 1)]) + cl['out'] * 1.0 + UP * 0.3)
            p = c + d * cr * rng.uniform(0.45, 1.0) ** 0.5   # most leaves on the clump's skin
            if weep > 0:
                p[1] -= abs(rng.gauss(0, 1.0)) * weep * cr * 1.1
            if p[1] < 1.35 and math.hypot(p[0], p[2]) < 1.05:
                continue   # keep the trunk clear at the bow's height (src/trees.rs measures it)
            nrm = unit(d * 0.7 + UP * 0.35 + np.array([rng.uniform(-.5, .5), rng.uniform(-.3, .3), rng.uniform(-.5, .5)]))
            mat = (leaf_key(species), tone_of())
            if burnt:
                u = rng.random()
                if cl['t'] > 0.6 and u < 0.3:
                    mat = (leaf_key(species), 2)
                elif u < 0.75:
                    mat = ('scorched', 0)
                else:
                    mat = ('wilted', 0)
            m.twig(p, nrm, leaf_len, leaf_w, rng.randint(3, 5), mat, rng, droop=weep * 0.8)
        n_sprays = max(4, n_sprays // 4)   # scales the blossom and candle counts below
        if not burnt and 'blossom' in sp and rng.random() < 0.8:
            pm, share = sp['blossom']
            for _ in range(max(1, int(n_sprays * share * 2))):
                p = c + cl['out'] * cr * 0.55 + np.array([rng.gauss(0, .35), rng.gauss(0, .3) + 0.15, rng.gauss(0, .35)]) * cr
                m.spray(p, unit(cl['out'] * 0.5 + UP), np.array([1.0, 0, 0]), 0.25, 1.0, pm, rng)   # a blossom truss
        if not burnt and sp.get('candles') and rng.random() < sp['candles']:
            for _ in range(rng.randint(2, 4)):
                p = c + cl['out'] * cr * 0.5 + np.array([rng.gauss(0, .4), rng.gauss(0, .2) + 0.2, rng.gauss(0, .4)]) * cr
                m.cone(p, p + unit(UP + cl['out'] * 0.3) * 0.28 * size_k, 0.07 * size_k, 3, 'petal_candle')
    lo, hi = m.bounds()
    info = dict(height_m=round(float(hi[1]), 2), crown_width_m=round(float(max(hi[0] - lo[0], hi[2] - lo[2])), 2),
                trunk_d_m=round(2 * r_at(1.2), 2), _r_break=r_at(0.7))
    if shattered:
        info['snap_height_m'] = round(snap_h, 2)
    return m, info


def build_fallen(name, species, mode, scale=1.0):
    """A whole tree on the ground: 'uprooted' (root plate torn out) or 'snapped' (broken at
    the base; the matching `stump_` asset is built from the same name)."""
    rng = seeded(name + ':fall')
    m = Mesh()
    sp = TREES[species]
    _, info = build_tree(name, species, scale=scale, mesh=m)
    r_base = info.pop('_r_break')
    if mode == 'uprooted':
        rp = max(1.1, r_base * 4.5) * rng.uniform(0.9, 1.2)
        m.disc(np.array([0.0, -0.3, 0.0]), UP, rp, 11, 0.5, 'soil', rng)
        for k in range(6):
            a = 2 * math.pi * k / 6 + rng.uniform(-0.4, 0.4)
            d = np.array([math.cos(a), 0.0, math.sin(a)])
            m.cone(np.array([0, -0.3, 0]) + d * rp * 0.7, np.array([0, -0.6, 0]) + d * rp * rng.uniform(1.1, 1.5),
                   0.06 + r_base * 0.15, 3, 'root')
        lift = rp * 0.55
        theta = math.radians(rng.uniform(80, 86))
    else:
        lift = r_base * 0.9
        theta = math.radians(rng.uniform(86, 90))
    yaw = 0.0
    c, s = math.cos(theta), math.sin(theta)
    base_cut = 0.0 if mode == 'uprooted' else 0.7   # the snapped trunk starts at the break

    def rot(p):
        x, y, z = p
        y = y - base_cut
        return np.array([x * c + y * s, -x * s + y * c + lift, z])

    def rotn(n):
        x, y, z = n
        return np.array([x * c + y * s, -x * s + y * c, z])

    # Remove what lies below the break for a snapped tree (the stump stays standing elsewhere).
    if mode == 'snapped':
        for mat, (P, N, I) in m.parts.items():
            for i in range(len(P)):
                if P[i][1] < base_cut and math.hypot(P[i][0], P[i][2]) < 1.5:
                    P[i] = np.array([P[i][0], base_cut, P[i][2]])
        sides = 6
        P, N, I = m._part('wood_splinter')
        base = len(P)
        for k in range(sides):
            a = 2 * math.pi * k / sides
            P.append(np.array([math.cos(a) * r_base, base_cut - rng.uniform(0, 1.6) * r_base, math.sin(a) * r_base]))
            N.append(-UP)
        P.append(np.array([0.0, base_cut - r_base, 0.0])); N.append(-UP)
        for k in range(sides):
            I += [base + k, base + sides, base + (k + 1) % sides]
    m.transform(rot, rotn)
    # The crown collapses on its side: limbs under it break, those above sag (half the standing
    # crown's width), and what the crown pressed into the ground lies on it.
    for mat, (P, N, I) in m.parts.items():
        for i in range(len(P)):
            if P[i][1] > lift + 0.5 and P[i][0] > 1.0:
                P[i] = np.array([P[i][0], lift + 0.5 + (P[i][1] - lift - 0.5) * 0.5, P[i][2]])
            if P[i][1] < 0.04:
                P[i] = np.array([P[i][0], 0.02 + 0.06 * ((i * 2654435761) % 997) / 997.0, P[i][2]])
    lo, hi = m.bounds()
    info = dict(length_m=round(float(hi[0] - lo[0]), 2), width_m=round(float(hi[2] - lo[2]), 2),
                height_m=round(float(hi[1]), 2), tree_height_m=info['height_m'], trunk_d_m=round(2 * r_base, 2))
    return m, info


def build_stump(name, species, scale=1.0):
    """The standing stump of `fallen_<species>_snapped_*` (same name -> same trunk)."""
    rng = seeded(name + ':stump')
    sp = TREES[species]
    r = build_tree(name, species, scale=scale)[1]['_r_break']   # the fallen tree's own trunk
    m = Mesh()
    h = 0.7   # build_fallen's break height
    m.tube([np.array([0, -0.25, 0]), np.array([0, h, 0])], [r * 1.3, r], 7,
           sp['bark'] if sp['bark'] != 'bark_birch' else 'bark_birch_dark', cap='jagged', rng=rng)
    for k in range(rng.randint(3, 5)):
        a = 2 * math.pi * k / 5 + rng.uniform(-.4, .4)
        d = np.array([math.cos(a), 0, math.sin(a)])
        m.cone(np.array([0, h - 0.1, 0]) + d * r * 0.55, np.array([0, h, 0]) + d * r * rng.uniform(.3, 1.0) + UP * rng.uniform(.3, .9),
               r * 0.3, 3, 'wood_splinter')
    lo, hi = m.bounds()
    return m, dict(height_m=round(float(hi[1]), 2), diameter_m=round(2 * r, 2))


# ── shrubs ──────────────────────────────────────────────────────────────────────────────────────
def build_shrub(name, species, scale=1.0, burnt=False):
    sp = SHRUBS[species]
    rng = seeded(name)
    m = Mesh()
    H = rng.uniform(*sp['height']) * scale
    leaf = sp.get('leaf', species)
    n = rng.randint(*sp['stems'])
    for s in range(n):
        az = 2 * math.pi * s / n + rng.uniform(-0.4, 0.4)
        out = np.array([math.cos(az), 0.0, math.sin(az)])
        tilt = sp['spread'] * rng.uniform(0.35, 1.0)
        L = H * rng.uniform(0.75, 1.05) / max(math.cos(tilt * 0.7), 0.4)
        base = out * rng.uniform(0.03, 0.25) * scale - UP * 0.1
        d1 = unit(UP * math.cos(tilt) + out * math.sin(tilt))
        tilt2 = tilt + sp['arch'] * rng.uniform(0.6, 1.2) * 1.4
        d2 = unit(UP * math.cos(tilt2) + out * math.sin(tilt2))
        mid = base + d1 * L * 0.55
        tip = mid + d2 * L * 0.45
        if tip[1] < 0.15:
            tip[1] = 0.15
        r0 = 0.5 * sp['stem_d'] * scale * rng.uniform(0.8, 1.2)
        mat = ('charcoal' if rng.random() < 0.7 else 'bark_scorched') if burnt else sp['bark']
        m.tube([base, mid, tip], [r0, r0 * 0.6, 0.006], 3, mat, rng=rng)
        for k in range(sp['clumps']):
            f = rng.uniform(0.15, 1.0)   # leafy down to the ground
            c = base + (mid - base) * (f * 2) if f < 0.5 else mid + (tip - mid) * ((f - 0.5) * 2)
            if burnt and rng.random() < 0.6:
                continue
            leaf_len, leaf_w = LEAF_SIZE[leaf]
            for _ in range(SHRUB_TWIGS_PER_LEAF_GROUP * sp['per'] + rng.randint(-2, 2)):
                p = c + np.array([rng.gauss(0, .5), rng.gauss(0, .35), rng.gauss(0, .5)]) * sp['spray'] * 1.9 * scale
                p[1] = max(p[1], 0.12)
                nrm = unit(out * 0.6 + UP * 0.7 + np.array([rng.uniform(-.6, .6), rng.uniform(-.3, .3), rng.uniform(-.6, .6)]))
                tone = rng.choice((0, 1, 1, 2))
                mt = (leaf, tone)
                if burnt:
                    mt = ('scorched', 0) if rng.random() < 0.7 else ('wilted', 0)
                m.twig(p, nrm, leaf_len * scale ** 0.3, leaf_w, rng.randint(3, 5), mt, rng)
            if not burnt and 'blossom' in sp and rng.random() < sp['blossom'][1] * 2:
                for _ in range(rng.randint(1, 3)):
                    p = c + out * 0.2 + np.array([rng.gauss(0, .3), 0.15 + rng.gauss(0, .2), rng.gauss(0, .3)]) * sp['spray'] * 1.5
                    m.spray(p, unit(UP + out * 0.4), np.array([1.0, 0, 0]), 0.18, 1.0, sp['blossom'][0], rng)
    lo, hi = m.bounds()
    return m, dict(height_m=round(float(hi[1]), 2), width_m=round(float(max(hi[0] - lo[0], hi[2] - lo[2])), 2))


# ── catalogue ───────────────────────────────────────────────────────────────────────────────────
def catalogue():
    """(name, builder, kind, description) for every asset."""
    out = []
    for sp, variants in (('birch', 'abc'), ('poplar', 'abc'), ('willow', 'abc'), ('linden', 'ab'), ('chestnut', 'ab'),
                         ('oak', 'abc'), ('pear', 'ab'), ('apple', 'abc'), ('cherry', 'ab')):
        for v in variants:
            scale = dict(a=1.0, b=0.82, c=1.12)[v]
            out.append((f'tree_{sp}_{v}', lambda n=f'tree_{sp}_{v}', s=sp, k=scale: build_tree(n, s, k), 'tree', f'{sp}, intact, May foliage'))
    for sp, variants in (('birch', 'ab'), ('poplar', 'ab'), ('willow', 'a'), ('linden', 'a'), ('oak', 'ab'), ('apple', 'a'), ('cherry', 'a')):
        for v in variants:
            n = f'tree_{sp}_burnt_{v}'
            out.append((n, lambda n=n, s=sp: build_tree(n, s, 1.0 if n[-1] == 'a' else 0.85, damage='burnt'), 'tree',
                        f'{sp}, burnt: charred bark to {"" }the fire height, scorched brown leaves, bare twigs'))
    for sp, variants in (('birch', 'ab'), ('poplar', 'ab'), ('willow', 'a'), ('linden', 'ab'), ('oak', 'ab'), ('chestnut', 'a'), ('pear', 'a')):
        for v in variants:
            n = f'tree_{sp}_shattered_{v}'
            out.append((n, lambda n=n, s=sp: build_tree(n, s, 1.0 if n[-1] == 'a' else 0.9, damage='shattered'), 'tree',
                        f'{sp}, shell-shattered: trunk snapped, splintered top, limbs broken and stripped'))
    for sp, mode, v in (('oak', 'uprooted', 'a'), ('birch', 'uprooted', 'a'), ('poplar', 'snapped', 'a'), ('willow', 'snapped', 'a'),
                        ('linden', 'snapped', 'a'), ('birch', 'snapped', 'a'), ('apple', 'uprooted', 'a')):
        n = f'fallen_{sp}_{mode}_{v}'
        out.append((n, lambda n=n, s=sp, md=mode: build_fallen(n, s, md, 0.9), 'fallen', f'{sp}, fallen ({mode}), leaves still green'))
        if mode == 'snapped':
            st = f'stump_{sp}_snapped_{v}'
            out.append((st, lambda n=n, s=sp: build_stump(n, s, 0.9), 'stump', f'the stump left by {n}'))
    for sp in SHRUBS:
        for v in 'ab':
            n = f'shrub_{sp}_{v}'
            out.append((n, lambda n=n, s=sp, k=(1.0 if v == 'a' else 0.75): build_shrub(n, s, k), 'shrub', f'{sp} shrub, May'))
    for sp, v in (('hazel', 'a'), ('hawthorn', 'b')):
        n = f'shrub_burnt_{v}'
        out.append((n, lambda n=n, s=sp: build_shrub(n, s, 0.9, burnt=True), 'shrub', f'burnt shrub ({sp} form): charred stems, scorched leaves'))
    return out


def export(name, mesh, out_dir, info, kind, desc):
    mat_index, materials, prims = {}, [], []
    for key, (P, N, I) in sorted(mesh.parts.items(), key=lambda kv: str(kv[0])):
        if isinstance(key, tuple):
            base, rough = leaf_material(*key)
            mname = f'leaf_{key[0]}_{key[1]}' if key[0] not in ('scorched', 'wilted') else f'leaf_{key[0]}'
        else:
            base, rough = MATERIALS[key]
            mname = key
        if mname not in mat_index:
            mat_index[mname] = len(materials)
            materials.append(dict(name=mname, base=base, roughness=rough))
        prims.append((mat_index[mname], np.array(P), np.array(N), np.array(I, dtype=np.uint32)))
    path = os.path.join(out_dir, name + '.glb')
    write_glb(path, name, prims, materials, extras=dict(tool=TOOL, version=VERSION))
    lo, hi = mesh.bounds()
    rec = dict(name=name, file=name + '.glb', kind=kind, description=desc, triangles=mesh.tris(),
               bounds_min=[round(float(v), 2) for v in lo], bounds_max=[round(float(v), 2) for v in hi],
               destructible=kind == 'tree', materials=[m['name'] for m in materials], **info)
    return rec


def main(argv):
    args = [a for a in argv if not a.startswith('--')]
    only = None
    if '--only' in argv:
        only = set(argv[argv.index('--only') + 1].split(','))
        args = [a for a in args if a != argv[argv.index('--only') + 1]]
    root = os.environ.get('KESSEL_ASSET_ROOT', '/windows/shop/panzerrechnersystem')
    out_dir = args[0] if args else os.path.join(root, 'assets/vegetation/spring1943')
    os.makedirs(out_dir, exist_ok=True)
    records = []
    for name, build, kind, desc in catalogue():
        if only and name not in only:
            continue
        mesh, info = build()
        info.pop('_r_break', None)
        rec = export(name, mesh, out_dir, info, kind, desc)
        records.append(rec)
        print(f"{name:32s} {rec['triangles']:5d} tris  h {rec.get('height_m', 0):5.2f} m  {kind}")
    if not only:
        leaf_table = {k: dict(rho_680_550_440=[round(v, 4) for v in lanes(l)[0]],
                              tau_680_550_440=[round(v, 4) for v in lanes(l)[1]], prospect=l._asdict())
                      for k, l in list(LEAVES.items()) + [('scorched', SCORCHED), ('wilted', WILTED)]}
        doc = dict(schema='panzer.vegetation-catalog', version=VERSION, tool=TOOL, frame='engine Y-up metres, origin at ground contact',
                   note='tree_* are destructible live trees (src/trees.rs); fallen_/stump_/shrub_ are static dressing. '
                        'Leaf albedo = PROSPECT-D reflectance (tau not rendered on GLB surfaces).',
                   leaf_optics=leaf_table, assets=records)
        with open(os.path.join(out_dir, 'spring1943.catalog.json'), 'w') as f:
            json.dump(doc, f, indent=1)
            f.write('\n')
    print(f'{len(records)} assets -> {out_dir}')


if __name__ == '__main__':
    main(sys.argv[1:])
