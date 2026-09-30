#!/usr/bin/env python3
"""canyon_course.py — the CANYON playtest course (2026-09-29).

A lone Tiger must punch north through a steep-walled ravine held by Soviet troops in depth and reach
the head of the ravine. The course is a playtest workshop for Act 1's Stalingrad winter
(``act_id`` 1), written deterministically into ``assets/worldgen/workshops/canyon-course/``:

- **The ravine** (terrain): about 2.75 km along its winding floor, the floor 150-390 m wide with
  three chokepoints, two wide arenas, a start basin, a final basin and six side pockets; the floor
  rises and falls some 10 m and carries a frozen stream channel. The walls are loess-like cliffs:
  a talus foot, then 64-76 degree faces to a rim 25-45 m above the floor. The Tiger's documented
  gradient (35 deg) and the static tipping limit its measured track gauge gives (src/tank_support.rs)
  cannot take any face steeper than 58.6 deg in any direction, so the walls cannot be climbed; a
  test proves every path from the start to the plateau meets such a face. The ravine's head is a
  ramp of about 10 deg up to the plateau: the goal.
- **Buildings** (``building_instances`` for src/building_grammar.rs): a burned workers' settlement
  of wooden houses and brick houses in the first arena, a factory edge of workshops and workers'
  blocks in the second, a ruined strongpoint in the final basin, two lone ruins at the first two
  chokes; flat pads, razed-plot rubble mounds (terrain), craters.
- **Cover**: brick yard walls (breakable), burnt and knocked-out KV-1 and Pz III wrecks (static
  source-surface collision), stripped trees (hinge-fall), stumps, logs, debris, planks, crates and
  snow drifts, from the reviewed kit the urban intro district uses.
- **Defenders in depth** (the ``street_encounter`` block the game already measures, and the
  ``canyon_course`` block): the rifle battalion's 45 mm guns, one per choke, each chosen among this
  tool's candidates by the game's field-of-fire probe on the built course; a two-tank T-34 platoon
  patrolling the factory arena and a three-tank platoon in the final basin (they counter-attack on
  what they hear and see); riflemen with the tanks and in the settlement's ruins. Density rises
  toward the goal.

Provenance: the ravine is a composite of the balkas that cut Stalingrad's factory district from the
steppe to the Volga (the Dolgiy, Krutoy and Banny ravines, and the Tsaritsa gorge in the centre):
steep-sided winter ravines with workers' settlements and factory edges along them, used for
dugouts, gun positions and approach routes (docs/research/STALINGRAD_URBAN_FABRIC_20260928.md §2;
the balka in world.rs's act-1 map). Its depth, width and length are a playtest course's, not one
ravine's measurements; nothing here claims a particular engagement. No words appear in play.

Usage:
    python3 tools/worldgen/canyon_course.py                  # write the workshop
    python3 tools/worldgen/canyon_course.py --plan captures/canyon_plan.png
    python3 tools/worldgen/canyon_course.py --dest /tmp/x    # write elsewhere (tests)

Deterministic: the project's counter hash keyed by stable strings; no time, no dict-order effects.
"""
import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import urban_intro_district as uid  # noqa: E402  (the district tool's grammar tables and helpers)

ROOT = Path(__file__).resolve().parents[2]
TOOL = "tools/worldgen/canyon_course.py"
TOOL_VERSION = 1
SEED = 0x20260929
DEST_REL = "assets/worldgen/workshops/canyon-course"
RECEIPT_NAME = "canyon-course.receipt.json"
COURSE_KEY = "canyon_course"
COURSE_SCHEMA = "kessel.canyon-course"
OUTPUT_FILES = ["manifest.v1.json", "manifest_edits.json", "heights.npy"]

# ── the terrain grid ─────────────────────────────────────────────────────────────────────────────
RES = 1024
SPACING = 3.0                      # finer than Act 1's 4.29 m: the cliffs and the pads need it
HALF = (RES - 1) * 0.5 * SPACING   # 1534.5 m
TERRAIN_TEXTURE = "assets/terrain/act1_rgb/act1_ground_hires.png"   # winter shading never samples it

# ── the ravine ───────────────────────────────────────────────────────────────────────────────────
Z_SOUTH = 1300.0     # floor's closed south end (s = 0)
Z_NORTH = -1300.0    # the ramp's top on the plateau (s = L)
# floor width (wall foot to wall foot) at fractions of the length: start basin, choke 1, arena 1
# (the settlement), choke 2, arena 2 (the factory edge), choke 3, the final basin, the ramp
WIDTH_PROFILE = [(0.00, 170.0), (0.06, 235.0), (0.14, 205.0), (0.21, 150.0), (0.28, 285.0),
                 (0.36, 390.0), (0.44, 300.0), (0.50, 150.0), (0.57, 265.0), (0.64, 360.0),
                 (0.72, 280.0), (0.79, 150.0), (0.85, 245.0), (0.90, 265.0), (0.945, 115.0),
                 (1.00, 70.0)]
CHOKES = (0.21, 0.50, 0.79)
# side pockets: (fraction, side (-1 west of the travel, +1 east), depth m, length m)
POCKETS = [(0.11, -1, 50.0, 120.0), (0.26, 1, 60.0, 140.0), (0.40, -1, 45.0, 110.0),
           (0.55, 1, 55.0, 130.0), (0.69, -1, 60.0, 150.0), (0.83, 1, 50.0, 120.0)]
# side gullies: tributary ravines cut into the plateau (fraction, side, length m, depth m, width m);
# at the rim each leaves the main wall at least GULLY_KEEP_M high, a hanging gully over a cliff
GULLIES = [(0.09, 1, 260.0, 14.0, 70.0), (0.17, -1, 200.0, 12.0, 60.0), (0.33, 1, 320.0, 16.0, 90.0),
           (0.46, -1, 240.0, 12.0, 70.0), (0.61, 1, 280.0, 15.0, 80.0), (0.74, -1, 300.0, 16.0, 90.0),
           (0.87, -1, 220.0, 12.0, 60.0), (0.52, 1, 180.0, 10.0, 50.0)]
GULLY_KEEP_M = 22.0
SHOULDER_M = (5.0, 22.0)          # the rim's rounded shoulder: drop and e-folding width
# rockfall cones at the wall foot (fraction, side, radius m, height m): cover on the floor
CONES = [(0.05, -1, 22.0, 5.0), (0.125, 1, 18.0, 4.0), (0.23, -1, 20.0, 5.5), (0.305, 1, 16.0, 3.5),
         (0.415, -1, 24.0, 6.0), (0.475, 1, 20.0, 5.0), (0.565, -1, 18.0, 4.5), (0.655, 1, 22.0, 5.5),
         (0.705, -1, 16.0, 4.0), (0.775, 1, 20.0, 5.0), (0.815, -1, 18.0, 4.5), (0.895, 1, 20.0, 5.0)]
RAMP_START = 0.935
WALL_EASE_CELLS = 0.8             # Gaussian easing of the walls' heights (sigma, grid cells)
PLATEAU_M = 40.0                  # mean plateau height; the floor lies 25-45 m below it
CLIFF_DEG = (66.0, 78.0)          # face angle range along the walls
TALUS_W = (4.0, 11.0)             # talus foot width
TALUS_H = (1.5, 4.5)              # talus foot height
# The Tiger's limits (src/tank_support.rs): pitch 35 deg along its travel, static tipping about the
# downhill track's outer edge (1.93 m) with the centre of gravity 1.30 m up [I]: 56.0 deg of roll.
# Any face whose gradient exceeds sqrt(tan^2 35 + tan^2 56) = tan 58.6 deg stops it in any direction.
TIGER_PITCH_DEG = 35.0
TIGER_TIP_DEG = math.degrees(math.atan(1.93 / 1.30))
UNCLIMBABLE_DEG = math.degrees(math.atan(math.hypot(math.tan(math.radians(TIGER_PITCH_DEG)),
                                                    math.tan(math.radians(TIGER_TIP_DEG)))))
SPAWN_FRAC = 0.03
GOAL_RADIUS_M = 85.0          # the ravine's head: every cell where the ramp's walls are lower than
                              # a 3 m cell can hold at the unclimbable angle lies inside it

# ── buildings ────────────────────────────────────────────────────────────────────────────────────
# groups: (id, fraction range, lateral band |d| from the route centreline, typology weights, target)
GROUPS = [
    ("hamlet", (0.075, 0.145), (22.0, 90.0),
     {"wooden_house": 0.80, "merchant_brick_house": 0.20}, 6),
    ("settlement", (0.285, 0.435), (18.0, 150.0),
     {"wooden_house": 0.66, "merchant_brick_house": 0.26, "workshop": 0.08}, 26),
    ("factory", (0.585, 0.715), (20.0, 150.0),
     {"workshop": 0.58, "apartment_block": 0.22, "merchant_brick_house": 0.20}, 14),
    ("strongpoint", (0.840, 0.925), (40.0, 110.0),
     {"merchant_brick_house": 0.50, "workshop": 0.30, "apartment_block": 0.20}, 9),
]
# ruined field and yard walls standing alone on the floor (a razed farmstead's, a factory yard's):
# clusters of brick bays, breakable cover to fight from and through
WALL_CLUSTERS = 26
LONE_RUINS = [("choke1", 0.195, -1, "merchant_brick_house"), ("choke2", 0.485, 1, "workshop")]
ROUTE_LANE_HALF_M = 16.0       # buildings keep this far off the route's centreline
WALL_CLEAR_M = 14.0            # and this far inside the wall foot
BUILDING_GAP_M = 5.0
SPAWN_CLEAR_M = 80.0
PAD_APRON_M = 8.0
RAZED_PLOTS = 4                # rubble mounds of buildings shelled flat (terrain, R1 §6.3 volumes)
CRATERS = 26

# ── the defence ──────────────────────────────────────────────────────────────────────────────────
# One 45 mm gun per choke (the rifle battalion's anti-tank guns, docs/AT_GUNS_20260928.md), sited by
# the game's field-of-fire probe among candidates on the ground ahead of the choke and in its pockets.
GUN_ZONES = [("choke1", (0.13, 0.27), (0.20, 0.38)),
             ("choke2", (0.42, 0.56), (0.49, 0.64)),
             ("choke3", (0.71, 0.85), (0.78, 0.93))]
GUN_ROUTE_CLEAR_M = 24.0       # candidates stand off the route the Tiger drives
GUN_OFF_BEAT_M = 14.0          # and off the platoons' beats (the game refuses closer than 12 m)
# Platoon A (three T-34s, the leader's radio, winter_encounter_ai::PLATOON_TANKS[0]) holds the final
# basin and counter-attacks; platoon B (two) patrols the factory arena.
BEATS = [("A", [(0.852, -32.0), (0.852, 32.0), (0.915, 36.0), (0.915, -36.0)]),
         ("B", [(0.600, -30.0), (0.600, 30.0), (0.690, 34.0), (0.690, -34.0)])]
# riflemen posted in the ruins (the game fills them from the riflemen not riding platoon A's tanks nor
# serving a gun): two in the settlement, two in the factory edge, two in the strongpoint
GARRISON = [(0.300, "settlement"), (0.380, "settlement"), (0.620, "factory"), (0.680, "factory"),
            (0.860, "strongpoint"), (0.900, "strongpoint")]

MP2 = uid.MP2
NAT = uid.NAT
BATTLE = uid.BATTLE
ASSETS = {
    "yard_wall": uid.ASSET["yard_wall"],
    "wreck_kv1": uid.ASSET["wreck_kv1"], "wreck_pz3": uid.ASSET["wreck_pz3"],
    "wreck_kv1_whole": uid.ASSET["wreck_kv1_whole"], "wreck_pz3_whole": uid.ASSET["wreck_pz3_whole"],
    "tree_tall": uid.ASSET["tree_tall"], "tree_small": uid.ASSET["tree_small"],
    "pine": NAT + "pine_tree_n_1.glb", "pine2": NAT + "pine_tree_n_2.glb",
    "stump": uid.ASSET["stump"], "log": uid.ASSET["log"],
    "debris": uid.ASSET["debris"], "debris2": uid.ASSET["debris2"],
    "gravel": uid.ASSET["gravel"], "brick_stack": uid.ASSET["brick_stack"],
    "plank": uid.ASSET["plank"], "board": uid.ASSET["board"], "crate": uid.ASSET["crate"],
    "charred_plank": uid.ASSET["charred_plank"], "scorch": uid.ASSET["scorch"],
    "snow_drift": uid.ASSET["snow_drift"],
}
HARD = {"yard_wall", "wreck_kv1", "wreck_pz3", "wreck_kv1_whole", "wreck_pz3_whole"}

# The course's instant: a grey January forenoon in the Kessel (the ephemeris puts the Sun about
# 16 deg up, south-south-east, shining along the ravine); the crewman's eye adapts.
LIGHTING = {"enabled": True, "fog_density": 0.0022, "overcast": 0.78, "storm": 0.12,
            "schema": "kessel.environment-authoring", "utc": "1943-01-20T08:40:00Z", "version": 2}
DISPLAY = {"height": 720, "ldr_dither": 0.65, "ordered_dither": 0.55, "palette_levels": 31.0,
           "retro_strength": 1.0, "width": 1280}

PROVENANCE = {
    "ravine": ("A composite of the balkas that cut Stalingrad's factory district from the steppe to the Volga "
               "(Dolgiy, Krutoy, Banny) and the Tsaritsa gorge in the centre: steep-sided winter ravines with "
               "workers' settlements and factory edges, used for dugouts, gun positions and approaches "
               "(docs/research/STALINGRAD_URBAN_FABRIC_20260928.md §2). Depth, width and length are a playtest "
               "course's, deeper and wider than those ravines; no single ravine is reproduced."),
    "buildings": "src/building_grammar.rs typologies and conditions; sizes and condition shares from the urban "
                 "intro district tool (R1 §3, §6.2); a workers' settlement of timber houses, a factory edge.",
    "defence": ("45 mm guns sited for flanking fire at 150-400 m from concealment, one per rifle battalion's "
                "approach (docs/research/SOVIET_AT_WEAPONS_BRIEF_20260928.md); T-34 platoons of three with the "
                "leader's radio, an attrited platoon of two (docs/WINTER_PLATOONS_20260928.md); tank riders and "
                "riflemen in ruins. A playtest course's garrison, not a recorded order of battle."),
    "tiger_limits": ("src/tank_support.rs: gradient 35 deg (Spielberger pp. 190-191); static tipping about the "
                     "downhill track's measured outer edge (1.93 m) with the centre of gravity 1.30 m up [I]."),
}
LIMITATIONS = [
    "The walls are a heightfield: no overhangs, caves or dugouts cut into them.",
    "The course's depth (25-45 m) and cliff angles (64-76 deg) exceed the Stalingrad balkas' for play.",
    "The goal is the ravine's head on the plateau; beyond it the plateau is open snow to the map edge.",
    "Wrecks and the charred plank are workshop authoring assets outside the reviewed runtime closure, as in "
    "the urban intro district.",
]


# ════════════════════════════════════════════════════════════════════════════════════════════════
# utilities
# ════════════════════════════════════════════════════════════════════════════════════════════════
def rnd(key, salt):
    k = uid.fnv1a32(key) if isinstance(key, str) else int(key)
    return uid.hash01((SEED ^ (k * 0x9E3779B9) ^ (salt * 0x85EBCA77)) & 0xFFFFFFFF)


def r3(v):
    return round(float(v), 3)


def phase(name):
    return rnd("phase." + name, 1) * math.tau


def interp_profile(profile, f):
    """Cosine interpolation of (fraction, value) control points; f may be an array."""
    f = np.clip(np.asarray(f, dtype=np.float64), 0.0, 1.0)
    xs = np.array([p[0] for p in profile])
    ys = np.array([p[1] for p in profile])
    i = np.clip(np.searchsorted(xs, f, side="right") - 1, 0, len(xs) - 2)
    t = (f - xs[i]) / (xs[i + 1] - xs[i])
    w = 0.5 - 0.5 * np.cos(np.pi * np.clip(t, 0.0, 1.0))
    return ys[i] * (1.0 - w) + ys[i + 1] * w


def gaussian_blur(H, sigma_cells):
    """Separable Gaussian with edge padding (deterministic numpy)."""
    return uid.gaussian(H, sigma_cells)


def smoothstep(t):
    t = np.clip(t, 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def sha256_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def dumps(obj, indent=1):
    return json.dumps(obj, indent=indent, ensure_ascii=False, sort_keys=True) + "\n"


# ════════════════════════════════════════════════════════════════════════════════════════════════
# the ravine's plan: centreline, width, pockets, floor
# ════════════════════════════════════════════════════════════════════════════════════════════════
class Ravine:
    def __init__(self):
        # centreline x(z), sampled every metre of z; arc length s along it
        z = np.arange(Z_SOUTH, Z_NORTH - 0.5, -1.0)
        u = (Z_SOUTH - z) / (Z_SOUTH - Z_NORTH)
        x = (70.0 * np.sin(math.tau * u * 1.6 + 0.5 + 0.2 * phase("c1"))
             + 28.0 * np.sin(math.tau * u * 3.7 + 2.1 + 0.2 * phase("c2")))
        x -= x[0]   # the south end on x = 0
        seg = np.hypot(np.diff(x), np.diff(z))
        self.x, self.z = x, z
        self.s = np.concatenate([[0.0], np.cumsum(seg)])
        self.L = float(self.s[-1])
        dx = np.gradient(x)
        dz = np.gradient(z)
        n = np.hypot(dx, dz)
        self.tx, self.tz = dx / n, dz / n          # unit tangent along the travel (toward -z)

    def frac_of(self, s):
        return np.clip(np.asarray(s) / self.L, 0.0, 1.0)

    def at(self, f):
        """Centreline point and unit tangent at fraction f of the length."""
        s = float(f) * self.L
        i = int(np.clip(np.searchsorted(self.s, s), 1, len(self.s) - 1))
        t = (s - self.s[i - 1]) / max(self.s[i] - self.s[i - 1], 1e-9)
        x = self.x[i - 1] + (self.x[i] - self.x[i - 1]) * t
        z = self.z[i - 1] + (self.z[i] - self.z[i - 1]) * t
        tx, tz = self.tx[i], self.tz[i]
        return x, z, tx, tz

    def world(self, f, d):
        """World (x, z) at fraction f and signed lateral offset d (+ east of the travel)."""
        x, z, tx, tz = self.at(f)
        # right-hand normal of the travel (travel -z -> +x is the east side, d > 0)
        nx, nz = -tz, tx
        return x + nx * d, z + nz * d

    @staticmethod
    def width(f):
        return interp_profile(WIDTH_PROFILE, f)

    def half_width(self, f, side):
        """Floor half-width on one side (side -1 west, +1 east) at fraction f, pockets and the
        wall line's irregularity (gullies and buttresses) included."""
        f = np.asarray(f, dtype=np.float64)
        s = f * self.L
        hw = 0.5 * self.width(f)
        for (pf, pside, depth, length) in POCKETS:
            if pside != side:
                continue
            q = (s - pf * self.L) / (0.5 * length)
            hw = hw + depth * np.clip(1.0 - q * q, 0.0, None) ** 1.5
        tag = "w" if side < 0 else "e"
        jit = (7.0 * np.sin(math.tau * s / 143.0 + phase(tag + "j1"))
               + 4.0 * np.sin(math.tau * s / 67.0 + phase(tag + "j2"))
               + 1.5 * np.sin(math.tau * s / 31.0 + phase(tag + "j3")))
        return hw + jit * smoothstep((0.985 - f) / 0.05)

    def floor(self, f):
        """Floor elevation at fraction f (the ramp excluded)."""
        f = np.asarray(f, dtype=np.float64)
        return (2.0 + 5.0 * f + 4.5 * np.sin(math.tau * f * 2.2 + 0.7)
                + 2.5 * np.sin(math.tau * f * 5.3 + 1.9))

    @staticmethod
    def plateau(x, z):
        return (PLATEAU_M + 3.0 * np.sin(x / 173.0 + 0.3) + 2.5 * np.sin(z / 211.0 + 1.2)
                + 1.5 * np.sin((x + z) / 97.0))

    def wall_params(self, f, side):
        s = np.asarray(f) * self.L
        tag = "w" if side < 0 else "e"
        a = 0.5 + 0.5 * np.sin(math.tau * s / 310.0 + phase(tag + "c")) * np.cos(math.tau * s / 133.0 + phase(tag + "c2"))
        b = 0.5 + 0.5 * np.sin(math.tau * s / 187.0 + phase(tag + "t"))
        c = 0.5 + 0.5 * np.sin(math.tau * s / 71.0 + phase(tag + "h"))
        cliff = np.radians(CLIFF_DEG[0] + (CLIFF_DEG[1] - CLIFF_DEG[0]) * a)
        return cliff, TALUS_W[0] + (TALUS_W[1] - TALUS_W[0]) * b, TALUS_H[0] + (TALUS_H[1] - TALUS_H[0]) * c


# ════════════════════════════════════════════════════════════════════════════════════════════════
# terrain
# ════════════════════════════════════════════════════════════════════════════════════════════════
def nearest_centreline(rv, X, Z):
    """For every cell: arc length s of the nearest centreline point, signed lateral distance d (east
    positive) and along-distance a beyond the ends (negative past the south end)."""
    n = X.shape[0]
    S = np.zeros_like(X)
    D = np.full_like(X, 1e4)
    A = np.zeros_like(X)
    step = 2
    xs, zs, ss = rv.x[::step], rv.z[::step], rv.s[::step]
    txs, tzs = rv.tx[::step], rv.tz[::step]
    zmax = Z_SOUTH
    for r in range(n):
        z = Z[r, 0]
        # centreline samples within 260 m of this row
        k0 = max(int((zmax - (z + 260.0)) / step), 0)
        k1 = min(int((zmax - (z - 260.0)) / step) + 1, len(xs))
        if k1 <= k0:
            continue
        cx = xs[k0:k1][None, :]
        cz = zs[k0:k1][None, :]
        row_x = X[r, :]
        near = np.abs(row_x - np.interp(-z, -rv.z, rv.x)) < 480.0
        if not near.any():
            continue
        px = row_x[near][:, None]
        d2 = (px - cx) ** 2 + (z - cz) ** 2
        j = np.argmin(d2, axis=1)
        k = k0 + j
        vx = row_x[near] - xs[k]
        vz = z - zs[k]
        along = vx * txs[k] + vz * tzs[k]
        # the right-hand normal of the travel (-tz, tx): + on the east side for a travel toward -z
        lat = vx * (-tzs[k]) + vz * txs[k]
        s = ss[k] + along
        a = np.where(s < 0.0, s, np.where(s > rv.L, s - rv.L, 0.0))
        S[r, near] = np.clip(s, 0.0, rv.L)
        D[r, near] = lat
        A[r, near] = a
    return S, D, A


def compose_terrain(rv, grid):
    X, Z = np.meshgrid(grid.axis, grid.axis)
    S, D, A = nearest_centreline(rv, X, Z)
    F = rv.frac_of(S)
    east = D >= 0.0
    hw = np.where(east, rv.half_width(F, 1), rv.half_width(F, -1))
    cliff_e, tw_e, th_e = rv.wall_params(F, 1)
    cliff_w, tw_w, th_w = rv.wall_params(F, -1)
    cliff = np.where(east, cliff_e, cliff_w)
    tw = np.where(east, tw_e, tw_w)
    th = np.where(east, th_e, th_w)
    e_lat = np.abs(D) - hw
    beyond = np.maximum(-A, 0.0)            # only the south end is a wall; the north end is the ramp top
    E = np.where(beyond > 0.0, np.hypot(np.maximum(e_lat, 0.0), beyond), e_lat)
    P = rv.plateau(X, Z)
    floor = rv.floor(F)
    # the ramp: the floor rises to the plateau over the last part of the length
    end_x, end_z, _, _ = rv.at(1.0)
    top = float(rv.plateau(np.array(end_x), np.array(end_z)))
    # linear: its walls lose height evenly, so only the head (the goal) has walls a 3 m cell cannot hold
    ramp = np.clip((F - RAMP_START) / (1.0 - RAMP_START), 0.0, 1.0)
    floor = floor + (top - floor) * ramp
    # the frozen stream channel and micro relief on the floor
    chan = -1.0 * np.clip(1.0 - (D / np.maximum(0.3 * hw, 1.0)) ** 2, 0.0, None) * (1.0 - ramp)
    micro = 0.35 * (np.sin(X / 23.0 + 0.4) * np.sin(Z / 29.0 + 1.1) + 0.6 * np.sin((X - Z) / 13.0 + 2.2))
    # low hummocks of drifted snow over frozen spoil, 1-2 m over 60-110 m: hull-down ground
    hummock = (1.0 * np.sin(X / 17.0 + 0.9) * np.sin(Z / 13.0 + 0.2)
               + 0.8 * np.sin((X + 0.6 * Z) / 11.0 + 1.7)) * (1.0 - ramp)
    H = floor + chan + micro + hummock
    # the walls
    Ep = np.maximum(E, 0.0)
    delta0 = np.maximum(P - (floor + micro), 0.0)
    # the plateau falls away to the rim in a rounded shoulder, and side gullies notch it
    shoulder = SHOULDER_M[0] * np.exp(-np.maximum(Ep - tw - 8.0, 0.0) / SHOULDER_M[1]) * (1.0 - ramp)
    gully = np.zeros_like(X)
    for (gf, gside, glen, gdepth, gwidth) in GULLIES:
        side_ok = (D >= 0.0) if gside > 0 else (D < 0.0)
        q = (S - gf * rv.L) / (0.5 * gwidth)
        along = np.clip(1.0 - Ep / glen, 0.0, 1.0) ** 0.8
        g = gdepth * along * np.exp(-q * q) * side_ok
        gully = np.maximum(gully, g)
    cut = np.minimum(shoulder + gully, np.maximum(delta0 - GULLY_KEEP_M, 0.0))
    P = P - cut
    delta = np.maximum(P - (floor + micro), 0.0)
    # where the wall is low (the ramp) the talus shrinks with it and the face keeps the height
    th = np.minimum(th, 0.1 * delta)
    talus = th * (np.minimum(Ep, tw) / tw) ** 1.5
    face = np.maximum(Ep - tw, 0.0) * np.tan(cliff)
    rise = talus + face
    k = 3.0
    h = np.clip(k - np.abs(rise - delta), 0.0, None) / k
    rise_c = np.minimum(rise, delta) - h * h * k * 0.25
    wall = floor + micro + np.maximum(rise_c, 0.0)
    H = np.where(E > 0.0, wall, H)
    # rockfall cones at the wall foot bury its lowest metres
    for (cf, cside, cr, ch) in CONES:
        cx, cz = rv.world(cf, cside * (float(rv.half_width(np.array(cf), cside)) + 4.0))
        dist = np.hypot(X - cx, Z - cz)
        cone = ch * np.clip(1.0 - dist / cr, 0.0, None) ** 1.3
        H = np.maximum(H, floor + micro + cone)
    # far from the ravine (outside the searched band) is plateau
    far = D >= 1e4 - 1.0
    H = np.where(far, P, H)
    # The walls are eased over about a cell (a Gaussian of WALL_EASE_CELLS): a face crossing the 3 m
    # grid obliquely otherwise alternates cliff-steep and near-level triangles along its rim and foot
    # (drawn as teeth). The floor keeps its exact heights; the unclimbable test runs on the result.
    soft = gaussian_blur(H, WALL_EASE_CELLS)
    blend = smoothstep((E + 4.0) / 6.0) * (1.0 - smoothstep((F - RAMP_START + 0.02) / 0.02))  # not on the ramp
    H = H * (1.0 - blend) + soft * blend
    return X, Z, H, dict(S=S, D=D, A=A, E=E, F=F, hw=hw, floor=floor, P=P, tw=tw, cliff=cliff)


# ════════════════════════════════════════════════════════════════════════════════════════════════
# buildings
# ════════════════════════════════════════════════════════════════════════════════════════════════
def footprint(x, z, w, d, yaw):
    q = int(round(yaw / 90.0)) % 2
    fw, fd = (w, d) if q == 0 else (d, w)
    return (x - fw / 2, x + fw / 2, z - fd / 2, z + fd / 2)


def rect_gap(a, b):
    dx = max(a[0] - b[1], b[0] - a[1], 0.0)
    dz = max(a[2] - b[3], b[2] - a[3], 0.0)
    return math.hypot(dx, dz) if (dx > 0 or dz > 0) else -min(a[1] - b[0], b[1] - a[0], a[3] - b[2], b[3] - a[2])


def choose_size(typ, key):
    t = uid.SIZES[typ]
    w = uid.pick(t["width"], rnd(key, 11))
    d = uid.pick(t["depth"], rnd(key, 12))
    st = uid.pick(t["storeys"], rnd(key, 13))
    cellar = rnd(key, 14) < t["cellar"]
    return float(w), float(d), int(st), bool(cellar)


class Site:
    """Everything placed so far, for the placement tests."""

    def __init__(self, rv, grid, H, fields):
        self.rv, self.grid, self.H, self.f = rv, grid, H, fields
        self.rects = []
        self.buildings = []
        self.spawn = rv.world(SPAWN_FRAC, 0.0)
        self.goal = rv.world(1.0, 0.0)

    def sample(self, name, x, z):
        return self.grid.sample(self.f[name], x, z)

    def on_floor(self, x, z, margin):
        """Point is on the floor at least `margin` inside the wall foot (and short of the ramp)."""
        return self.sample("E", x, z) <= -margin and self.sample("F", x, z) < RAMP_START - 0.01

    def rect_ok(self, rect, lane=ROUTE_LANE_HALF_M, wall=WALL_CLEAR_M, gap=BUILDING_GAP_M):
        x0, x1, z0, z1 = rect
        pts = [(x0, z0), (x0, z1), (x1, z0), (x1, z1), ((x0 + x1) / 2, (z0 + z1) / 2),
               ((x0 + x1) / 2, z0), ((x0 + x1) / 2, z1), (x0, (z0 + z1) / 2), (x1, (z0 + z1) / 2)]
        for (x, z) in pts:
            if not self.on_floor(x, z, wall):
                return False
            if abs(self.sample("D", x, z)) < lane:
                return False
            if math.hypot(x - self.spawn[0], z - self.spawn[1]) < SPAWN_CLEAR_M:
                return False
        # the route lane itself: no corner may reach across the centreline
        ds = [self.sample("D", x, z) for (x, z) in pts]
        if min(ds) < 0.0 < max(ds):
            return False
        return all(rect_gap(rect, r) >= gap for r in self.rects)


def place_buildings(site):
    rv = site.rv
    out = []
    for gid, (fa, fb), (dmin, dmax), weights, target in GROUPS:
        placed = 0
        tries = 0
        k = 0
        while placed < target and tries < 900:
            tries += 1
            key = f"{gid}.{tries}"
            f = fa + (fb - fa) * rnd(key, 1)
            side = -1 if rnd(key, 2) < 0.5 else 1
            typ = uid.pick(weights, rnd(key, 3))
            w, d, st, cellar = choose_size(typ, key)
            # facade toward the route: local +Z faces the centreline
            cx, cz, tx, tz = rv.at(f)
            face_dir = (-side * -tz, -side * tx)       # from the building toward the route
            yaw = math.degrees(math.atan2(face_dir[0], face_dir[1]))
            yaw = (round(yaw / 90.0) * 90.0) % 360.0
            q = int(round(yaw / 90.0)) % 2
            across = d if q == 0 else w                # extent across x ... used for setback only
            lateral = dmin + (dmax - dmin) * rnd(key, 4) ** 1.6 + 0.5 * max(w, d)
            x, z = rv.world(f, side * lateral)
            x, z = round(x * 2) / 2, round(z * 2) / 2
            rect = footprint(x, z, w, d, yaw)
            if not site.rect_ok(rect):
                continue
            k += 1
            bid = f"canyon.{gid}.b{k:02d}"
            cond = uid.pick(uid.CONDITION_SHARES[typ], rnd(bid, 5))
            if cond == "intact":
                cond = "burned"      # nothing stood whole in a held ravine
            out.append(dict(id=bid, group=gid, typology=typ, width_m=w, depth_m=d, storeys=st, cellar=cellar,
                            condition=cond, x=x, z=z, yaw_deg=yaw, rect=rect, frac=r3(f), side=side))
            site.rects.append(rect)
            placed += 1
    for gid, f, side, typ in LONE_RUINS:
        for attempt in range(40):
            key = f"lone.{gid}.{attempt}"
            w, d, st, cellar = choose_size(typ, key)
            cx, cz, tx, tz = rv.at(f)
            face_dir = (-side * -tz, -side * tx)
            yaw = (round(math.degrees(math.atan2(face_dir[0], face_dir[1])) / 90.0) * 90.0) % 360.0
            lateral = 30.0 + 18.0 * rnd(key, 4) + 0.5 * max(w, d)
            x, z = rv.world(f + 0.01 * (rnd(key, 5) - 0.5), side * lateral)
            x, z = round(x * 2) / 2, round(z * 2) / 2
            rect = footprint(x, z, w, d, yaw)
            if not site.rect_ok(rect, wall=10.0):
                continue
            bid = f"canyon.{gid}.lone"
            out.append(dict(id=bid, group=gid, typology=typ, width_m=w, depth_m=d, storeys=st, cellar=cellar,
                            condition="partly_collapsed" if typ == "merchant_brick_house" else "shelled",
                            x=x, z=z, yaw_deg=yaw, rect=rect, frac=r3(f), side=side))
            site.rects.append(rect)
            break
    for b in out:
        b["seed"] = uid.fnv1a32(b["id"]) ^ (SEED & 0xFFFFFFFF)
    # razed plots: the masonry buildings nearest the route in the settlement and factory are shelled flat
    masonry = sorted((b for b in out if b["typology"] in uid.MASONRY and b["group"] in ("settlement", "factory")),
                     key=lambda b: (rnd(b["id"], 21), b["id"]))
    for b in masonry[:RAZED_PLOTS]:
        b["razed"] = True
    for b in out:
        b.setdefault("razed", False)
    out.sort(key=lambda b: b["id"])
    site.buildings = out
    return out


def apply_pads(site, X, Z, H):
    """Flat pads under standing buildings (the footprint and one cell round it at the mean ground),
    8 m aprons back to the ground; rubble domes on razed plots; craters."""
    grid = site.grid
    base = H.copy()
    records = []
    for b in site.buildings:
        x0, x1, z0, z1 = b["rect"]
        k = grid.sp + 0.01
        rs, cs = grid.window(x0 - k - PAD_APRON_M, x1 + k + PAD_APRON_M, z0 - k - PAD_APRON_M, z1 + k + PAD_APRON_M)
        Xw, Zw = X[rs, cs], Z[rs, cs]
        inside = (Xw >= x0 - k) & (Xw <= x1 + k) & (Zw >= z0 - k) & (Zw <= z1 + k)
        level = float(base[rs, cs][inside].mean())
        dx = np.maximum(np.maximum(x0 - k - Xw, Xw - (x1 + k)), 0.0)
        dz = np.maximum(np.maximum(z0 - k - Zw, Zw - (z1 + k)), 0.0)
        dist = np.hypot(dx, dz)
        wgt = 1.0 - smoothstep(dist / PAD_APRON_M)
        H[rs, cs] = H[rs, cs] * (1.0 - wgt) + level * wgt
        b["pad_m"] = r3(level)
        b["_pad"] = (rs, cs, inside, level)
    # a neighbour's apron may reach into a pad: every pad is laid exactly again over the aprons
    for b in site.buildings:
        rs, cs, inside, level = b.pop("_pad")
        block = H[rs, cs]
        block[inside] = level
        H[rs, cs] = block
    for b in site.buildings:
        x0, x1, z0, z1 = b["rect"]
        k = grid.sp + 0.01
        rs, cs = grid.window(x0 - k - PAD_APRON_M, x1 + k + PAD_APRON_M, z0 - k - PAD_APRON_M, z1 + k + PAD_APRON_M)
        Xw, Zw = X[rs, cs], Z[rs, cs]
        if b["razed"]:
            if b["typology"] == "apartment_block":
                v, c, a = uid.APARTMENT_MOUND_PER_STOREY
                vpm2, crest, apron = v * b["storeys"], c * b["storeys"], a
            else:
                tab = uid.MOUND_MODEL[b["typology"]]
                vpm2, crest, apron = tab[min(b["storeys"], max(tab))]
            volume = vpm2 * b["width_m"] * b["depth_m"]
            field, rec = uid.dome(Xw, Zw, b["rect"], apron, crest, volume)
            H[rs, cs] = H[rs, cs] + field
            rec.update(id=b["id"], rubble_volume_m3=r3(volume))
            records.append(rec)
    craters = []
    tries = 0
    while len(craters) < CRATERS and tries < 400:
        tries += 1
        key = f"crater.{tries}"
        f = 0.08 + 0.84 * rnd(key, 1)
        hw = float(site.rv.width(f)) * 0.5
        d = (rnd(key, 2) * 2.0 - 1.0) * (hw - 20.0)
        x, z = site.rv.world(f, d)
        r = 2.5 + 2.0 * rnd(key, 3)
        if not site.on_floor(x, z, 10.0) or math.hypot(x - site.spawn[0], z - site.spawn[1]) < 40.0:
            continue
        if any(uid.rect_dist(rr, x, z) < r * 2.5 for rr in site.rects):
            continue
        if any(math.hypot(x - c[0], z - c[1]) < r * 3.0 for c in craters):
            continue
        dpt = 0.9 + 0.8 * rnd(key, 4)
        rs, cs = grid.window(x - 3.1 * r, x + 3.1 * r, z - 3.1 * r, z + 3.1 * r)
        H[rs, cs] += uid.crater_profile(X[rs, cs], Z[rs, cs], x, z, r, dpt)
        craters.append((r3(x), r3(z), r3(r), r3(dpt)))
    return records, craters


# ════════════════════════════════════════════════════════════════════════════════════════════════
# dressing: cover and battle traces from the reviewed kit
# ════════════════════════════════════════════════════════════════════════════════════════════════
class Dresser:
    def __init__(self, site, H):
        self.site = site
        self.H = H
        self.items = []     # (key, x, y, z, deg, scale)
        self.hard = []
        self.counts = {}

    def ground(self, x, z):
        return self.site.grid.sample(self.H, x, z)

    def free(self, x, z, margin=1.0):
        """Clear of every building by `margin` and of the hard cover already placed (walls, wrecks)."""
        return (all(uid.rect_dist(r, x, z) > margin for r in self.site.rects)
                and all(math.hypot(h["x"] - x, h["z"] - z) > margin + 1.5 for h in self.hard))

    def add(self, cls, key, x, z, deg=0.0, s=1.0, dy=0.0):
        y = self.ground(x, z) + dy
        sc = [r3(v) for v in s] if isinstance(s, (tuple, list)) else r3(s)
        self.items.append((key, r3(x), r3(y), r3(z), round(deg % 360.0, 2), sc))
        self.counts[cls] = self.counts.get(cls, 0) + 1
        if key in HARD:
            self.hard.append(dict(cls=cls, key=key, x=r3(x), z=r3(z), deg=round(deg % 360.0, 2)))


def dress(site, H):
    Dr = Dresser(site, H)
    rv = site.rv
    # yard walls: brick walls along the backs and sides of the settlement's brick houses and the
    # factory's workshops, with gaps (gates, breaches); breakable cover to fight from and through
    for b in site.buildings:
        if b["razed"] or b["typology"] == "wooden_house":
            continue
        if b["group"] not in ("settlement", "factory", "strongpoint"):
            continue
        x0, x1, z0, z1 = b["rect"]
        side = b["side"]
        # the yard lies behind the building (away from the route)
        cx, cz = (x0 + x1) / 2, (z0 + z1) / 2
        f = b["frac"]
        _, _, tx, tz = rv.at(f)
        nx, nz = -tz * side, tx * side          # away from the route
        depth = 12.0 + 6.0 * rnd(b["id"], 31)
        horizontal = abs(nx) > abs(nz)
        if horizontal:
            wx = (x1 if nx > 0 else x0) + math.copysign(depth, nx)
            segs = [(wx, z) for z in np.arange(z0 + 2.0, z1 - 1.0, 4.0)]
            deg = 90.0
        else:
            wz = (z1 if nz > 0 else z0) + math.copysign(depth, nz)
            segs = [(x, wz) for x in np.arange(x0 + 2.0, x1 - 1.0, 4.0)]
            deg = 0.0
        for i, (x, z) in enumerate(segs):
            key = f"{b['id']}.yardwall.{i}"
            if rnd(key, 1) < 0.22:
                continue   # a breach or a gate
            if not site.on_floor(x, z, 6.0) or not Dr.free(x, z, 2.0) or abs(site.sample("D", x, z)) < 10.0:
                continue
            Dr.add("yard_wall", "yard_wall", x, z, deg, 1.0, dy=-0.1)
    # lone ruined walls: two to four brick bays in a line, gaps where bays fell
    placed = 0
    tries = 0
    while placed < WALL_CLUSTERS and tries < 600:
        tries += 1
        key = f"wallcluster.{tries}"
        f = 0.05 + 0.87 * rnd(key, 1)
        side = -1 if rnd(key, 2) < 0.5 else 1
        hw = float(rv.half_width(np.array(f), side))
        d = side * (14.0 + (hw - 30.0) * rnd(key, 3))
        x, z = rv.world(f, d)
        if not site.on_floor(x, z, 10.0) or not Dr.free(x, z, 10.0):
            continue
        if math.hypot(x - site.spawn[0], z - site.spawn[1]) < 45.0:
            continue
        along_x = rnd(key, 4) < 0.5
        n = 2 + int(rnd(key, 5) * 3)
        for k in range(n):
            if rnd(f"{key}.{k}", 6) < 0.2:
                continue
            wx, wz = (x + 4.0 * k, z) if along_x else (x, z + 4.0 * k)
            if site.on_floor(wx, wz, 8.0) and Dr.free(wx, wz, 2.0):
                Dr.add("field_wall", "yard_wall", wx, wz, 0.0 if along_x else 90.0, 1.0, dy=-0.1)
        placed += 1
    # wrecks: knocked-out tanks of the autumn's fighting, each in a halo of planks and crates; the
    # ravine floor was the approach, so they lie on and beside the route (cover for both sides)
    wrecks = [(0.065, 30.0, "wreck_pz3"), (0.155, -34.0, "wreck_kv1"), (0.245, 22.0, "wreck_pz3_whole"),
              (0.335, 12.0, "wreck_kv1_whole"), (0.39, -20.0, "wreck_pz3"), (0.47, -30.0, "wreck_kv1_whole"),
              (0.545, 26.0, "wreck_pz3"), (0.625, -14.0, "wreck_kv1"), (0.665, 18.0, "wreck_pz3_whole"),
              (0.745, -24.0, "wreck_kv1"), (0.81, 28.0, "wreck_pz3_whole"), (0.88, 8.0, "wreck_kv1_whole"),
              (0.915, -26.0, "wreck_pz3")]
    for (f, d, kind) in wrecks:
        for attempt in range(8):
            key = f"wreck.{f}.{attempt}"
            x, z = rv.world(f + 0.004 * attempt * (1 if attempt % 2 else -1), d)
            if site.on_floor(x, z, 8.0) and Dr.free(x, z, 8.0):
                break
        else:
            continue
        _, _, tx, tz = rv.at(f)
        heading = math.degrees(math.atan2(tx, tz)) + (rnd(key, 3) - 0.5) * 70.0
        Dr.add("wreck", kind, x, z, heading, 1.0, dy=-0.25)
        for m in range(8):
            k = f"{key}.halo.{m}"
            a = rnd(k, 1) * math.tau
            r = 5.0 + 9.0 * rnd(k, 2)
            hx, hz = x + math.cos(a) * r, z + math.sin(a) * r
            if not Dr.free(hx, hz, 0.5) or not site.on_floor(hx, hz, 3.0):
                continue
            item = ["board", "plank", "crate", "charred_plank", "brick_stack", "plank", "scorch", "crate"][m]
            Dr.add("wreck_halo", item, hx, hz, rnd(k, 3) * 360.0, 1.0, dy=-0.03)
    # trees: stripped deciduous trees and a few pines along the stream and the wall feet, thicker in
    # the pockets (a ravine's bottom kept its trees and scrub longest)
    tries = 0
    trees = 0
    while trees < 150 and tries < 3000:
        tries += 1
        key = f"tree.{tries}"
        f = 0.04 + 0.9 * rnd(key, 1)
        side = -1 if rnd(key, 2) < 0.5 else 1
        hw = float(rv.half_width(np.array(f), side))
        u = rnd(key, 3)
        d = side * (hw - 6.0 - 26.0 * u * u) if rnd(key, 4) < 0.8 else (rnd(key, 5) - 0.5) * 20.0
        x, z = rv.world(f, d)
        if not site.on_floor(x, z, 3.0) or not Dr.free(x, z, 3.0):
            continue
        if math.hypot(x - site.spawn[0], z - site.spawn[1]) < 25.0:
            continue
        v = rnd(key, 6)
        if v < 0.42:
            Dr.add("tree", "tree_tall", x, z, rnd(key, 7) * 360.0, 1.2 + 0.4 * rnd(key, 8))
        elif v < 0.66:
            Dr.add("tree", "tree_small", x, z, rnd(key, 7) * 360.0, 1.7 + 0.7 * rnd(key, 8))
        elif v < 0.78:
            Dr.add("tree", "pine" if rnd(key, 9) < 0.5 else "pine2", x, z, rnd(key, 7) * 360.0, 0.9 + 0.3 * rnd(key, 8))
        elif v < 0.90:
            Dr.add("stump", "stump", x, z, rnd(key, 7) * 360.0, 1.1 + 0.4 * rnd(key, 8))
        else:
            Dr.add("log", "log", x, z, rnd(key, 7) * 360.0, (1.6, 0.8, 0.9), dy=-0.05)
        trees += 1
    # debris and battle traces at the ruins and by the gun zones
    for b in site.buildings:
        if b["typology"] == "wooden_house" and not b["razed"]:
            continue
        x0, x1, z0, z1 = b["rect"]
        for m in range(6):
            k = f"{b['id']}.debris.{m}"
            a = rnd(k, 1) * math.tau
            cx, cz = (x0 + x1) / 2, (z0 + z1) / 2
            r = 0.5 * math.hypot(x1 - x0, z1 - z0) + 1.5 + 3.5 * rnd(k, 2)
            x, z = cx + math.cos(a) * r, cz + math.sin(a) * r
            if not Dr.free(x, z, 0.8) or not site.on_floor(x, z, 3.0):
                continue
            item = ["debris", "debris2", "gravel", "brick_stack", "plank", "charred_plank"][m]
            Dr.add("debris", item, x, z, rnd(k, 3) * 360.0, 1.0, dy=-0.03)
    # snow drifts in the lee of the yard walls and wrecks (Act 1's westerly: lee to the east)
    for h in list(Dr.hard):
        key = f"drift.{h['x']}.{h['z']}"
        if rnd(key, 1) < 0.35:
            continue
        x, z = h["x"] + 1.6, h["z"] + 0.45
        if not Dr.free(x, z, 0.3):
            continue
        hh = 0.25 + 0.2 * rnd(key, 2)
        Dr.add("snow_drift", "snow_drift", x, z, h["deg"], (1.6, hh / 0.13, 0.9), dy=-0.04)
    return Dr


# ════════════════════════════════════════════════════════════════════════════════════════════════
# the defence: the street-encounter block the game measures, and the course block
# ════════════════════════════════════════════════════════════════════════════════════════════════
def seg_dist(px, pz, a, b):
    return uid.seg_dist(px, pz, a, b)


def route_line(rv, fa, fb, step_m=40.0):
    n = max(int((fb - fa) * rv.L / step_m), 2)
    return [[r3(v) for v in rv.world(fa + (fb - fa) * i / n, 0.0)] for i in range(n + 1)]


def defence(site, Dr, beat_specs=BEATS):
    rv = site.rv
    beats = []
    for platoon, pts in beat_specs:
        beats.append(dict(platoon=platoon, points=[[r3(v) for v in rv.world(f, d)] for f, d in pts]))
    legs = [(tuple(a), tuple(b)) for bt in beats for a, b in zip(bt["points"], bt["points"][1:] + bt["points"][:1])]
    kill = []
    for gid, (fa, fb), _ in GUN_ZONES:
        kill.append(dict(id=f"route.{gid}", role="route", weight=1.0, points=route_line(rv, fa, fb)))
    # the whole route at a low weight: the nav grid the platoons drive covers it all
    kill.append(dict(id="route.whole", role="way_out", weight=0.1, points=route_line(rv, 0.01, 0.985, 80.0)))
    hard = [(h["x"], h["z"]) for h in Dr.hard]

    def usable(x, z, zone):
        return (site.on_floor(x, z, 6.0)
                and abs(site.sample("D", x, z)) >= GUN_ROUTE_CLEAR_M
                and Dr.free(x, z, 3.0)
                and all(math.hypot(x - hx, z - hz) > 5.0 for hx, hz in hard)
                and all(seg_dist(x, z, a, b) >= GUN_OFF_BEAT_M for a, b in legs)
                and math.hypot(x - site.spawn[0], z - site.spawn[1]) > 250.0)

    gun_sites = []
    zones = []
    for gid, _, (ca, cb) in GUN_ZONES:
        idx = []
        seen = set()
        # open ground in cover of the terrain: pockets, wall feet, craters (a lattice in s and d)
        f = ca
        while f <= cb:
            hw_w = float(rv.half_width(np.array(f), -1))
            hw_e = float(rv.half_width(np.array(f), 1))
            for side, hw in ((-1, hw_w), (1, hw_e)):
                d = GUN_ROUTE_CLEAR_M + 4.0
                while d <= hw - 8.0:
                    x, z = rv.world(f, side * d)
                    x, z = r3(round(x)), r3(round(z))
                    if (x, z) not in seen and usable(x, z, gid):
                        seen.add((x, z))
                        idx.append(len(gun_sites))
                        gun_sites.append(dict(kind="ground", on=f"{gid}.ground", x=x, z=z))
                    d += 14.0
            f += 12.0 / rv.L
        # beside building corners in the zone
        for b in site.buildings:
            if b["razed"] or not (ca - 0.02 <= b["frac"] <= cb + 0.02):
                continue
            x0, x1, z0, z1 = b["rect"]
            for cx, sx in ((x0, -1.0), (x1, 1.0)):
                for cz, sz in ((z0, -1.0), (z1, 1.0)):
                    for ox, oz in ((2.5 * sx, 2.5 * sz), (2.5 * sx, -2.0 * sz), (-2.0 * sx, 2.5 * sz)):
                        x, z = r3(cx + ox), r3(cz + oz)
                        if (x, z) not in seen and usable(x, z, gid):
                            seen.add((x, z))
                            idx.append(len(gun_sites))
                            gun_sites.append(dict(kind="corner", on=b["id"], x=x, z=z))
        # on razed plots' rubble in the zone
        for b in site.buildings:
            if not b["razed"] or not (ca - 0.02 <= b["frac"] <= cb + 0.02):
                continue
            x0, x1, z0, z1 = b["rect"]
            for x in np.arange(x0 + 2.0, x1 - 1.9, 4.0):
                for z in np.arange(z0 + 2.0, z1 - 1.9, 4.0):
                    x_, z_ = r3(x), r3(z)
                    if (x_, z_) not in seen and usable(x_, z_, gid):
                        seen.add((x_, z_))
                        idx.append(len(gun_sites))
                        gun_sites.append(dict(kind="rubble", on=b["id"], x=x_, z=z_))
        zones.append(dict(id=gid, kill_zone=f"route.{gid}", candidates=idx))
    # the ways a tank may drive: the floor, as corridors along the centreline
    corridors = []
    n = 60
    for i in range(n):
        fa, fb = 0.005 + 0.975 * i / n, 0.005 + 0.975 * (i + 1) / n
        w = 2.0 * min(float(rv.half_width(np.array(fa), -1)), float(rv.half_width(np.array(fa), 1)),
                      float(rv.half_width(np.array(fb), -1)), float(rv.half_width(np.array(fb), 1))) - 16.0
        corridors.append(dict(a=[r3(v) for v in rv.world(fa, 0.0)], b=[r3(v) for v in rv.world(fb, 0.0)],
                              width_m=r3(max(w, 40.0))))
    xs = [c for cor in corridors for c in (cor["a"][0], cor["b"][0])]
    zs = [c for cor in corridors for c in (cor["a"][1], cor["b"][1])]
    district = [r3(min(xs) - 260.0), r3(max(xs) + 260.0), r3(min(zs) - 60.0), r3(max(zs) + 60.0)]
    sx, sz = site.spawn
    start_rect = [r3(sx - 40.0), r3(sx + 40.0), r3(sz - 40.0), r3(sz + 40.0)]
    street = dict(
        schema="kessel.street-encounter", version=1, tool=TOOL, tool_version=TOOL_VERSION,
        kill_zone=kill, gun_sites=gun_sites, beats=beats,
        ways=dict(district=district, rects=[start_rect], corridors=corridors),
        rules=dict(gun_route_clear_m=GUN_ROUTE_CLEAR_M, gun_off_beat_m=GUN_OFF_BEAT_M,
                   candidates="per choke: open ground in the pockets and at the wall feet on a 12 x 14 m lattice, three "
                              "points outside each corner of each standing building, razed plots' rubble on a 4 m grid; "
                              "off the route and the beats, clear of buildings and hard cover, 250 m from the start"),
        provenance=dict(doctrine=["docs/research/SOVIET_AT_WEAPONS_BRIEF_20260928.md (flanking fire from concealment)"],
                        platoons=["docs/WINTER_PLATOONS_20260928.md"]),
        note=("The CANYON course's defence (tools/worldgen/canyon_course.py). The game sites one gun per choke among "
              "canyon_course.gun_zones' candidates by its field-of-fire probe on the built course (src/canyon.rs) "
              "and drives the platoons' beats over the floor it measures clear for a T-34."),
    )
    garrison = []
    used = set()
    for f, gid in GARRISON:
        best = None
        for b in site.buildings:
            if b["group"] != gid or b["razed"] or b["id"] in used:
                continue
            if best is None or abs(b["frac"] - f) < abs(best["frac"] - f):
                best = b
        if best is None:
            continue
        used.add(best["id"])
        x0, x1, z0, z1 = best["rect"]
        # at the building's corner toward the start, facing south along the ravine
        for (x, z) in ((x0 - 1.5, z1 + 1.5), (x1 + 1.5, z1 + 1.5), (x0 - 1.5, z0 - 1.5), (x1 + 1.5, z0 - 1.5)):
            if site.on_floor(x, z, 2.0) and Dr.free(x, z, 1.0):
                _, _, tx, tz = rv.at(best["frac"])
                garrison.append(dict(x=r3(x), z=r3(z), heading_deg=r3(math.degrees(math.atan2(-tx, -tz))), host=best["id"]))
                break
    sx, sz, stx, stz = rv.at(SPAWN_FRAC)
    gx, gz, _, _ = rv.at(1.0)
    course = dict(
        schema=COURSE_SCHEMA, version=1, tool=TOOL, tool_version=TOOL_VERSION,
        start=dict(x=r3(sx), z=r3(sz), heading_deg=r3(math.degrees(math.atan2(stx, stz)))),
        goal=dict(x=r3(gx), z=r3(gz), radius_m=GOAL_RADIUS_M),
        length_m=r3(rv.L), gun_zones=zones, garrison=garrison,
        note=("Reaching the goal disc in the Tiger completes the run (src/canyon.rs); the elapsed time is kept with "
              "the game's state. The street_encounter block beside this one carries the kill zones, candidates, "
              "beats and ways."),
    )
    return street, course


# ════════════════════════════════════════════════════════════════════════════════════════════════
# checks the tests and the receipt share
# ════════════════════════════════════════════════════════════════════════════════════════════════
def face_gradient(H, grid):
    """Gradient magnitude (tan of the steepest slope) at every sample, by central differences over
    two cells: gentler than the drawn triangles at a cliff's foot and rim, so generous to the Tiger."""
    gz, gx = np.gradient(H, grid.sp)
    return np.hypot(gx, gz)


def reachable(H, grid, start, max_deg, blocked=None, face_deg=None):
    """8-neighbour flood fill over edges whose slope is at most `max_deg`; with `face_deg`, samples
    whose gradient magnitude exceeds it are impassable in every direction (the Tiger's pitch and
    tipping limits combined: whatever its heading, such a face exceeds one of them)."""
    tanv = math.tan(math.radians(max_deg))
    n = H.shape[0]
    if face_deg is not None:
        steep = face_gradient(H, grid) > math.tan(math.radians(face_deg))
        blocked = steep if blocked is None else (blocked | steep)
    seen = np.zeros(H.shape, dtype=bool)
    r0 = int(round(grid.col(start[1])))
    c0 = int(round(grid.col(start[0])))
    seen[r0, c0] = True
    frontier = np.zeros(H.shape, dtype=bool)
    frontier[r0, c0] = True
    sp = grid.sp
    steps = [(-1, 0, sp), (1, 0, sp), (0, -1, sp), (0, 1, sp),
             (-1, -1, sp * 1.4142), (-1, 1, sp * 1.4142), (1, -1, sp * 1.4142), (1, 1, sp * 1.4142)]
    ok_edge = {}
    for dr, dc, dist in steps:
        a = H[max(dr, 0):n + min(dr, 0), max(dc, 0):n + min(dc, 0)]
        b = H[max(-dr, 0):n + min(-dr, 0), max(-dc, 0):n + min(-dc, 0)]
        ok_edge[(dr, dc)] = np.abs(a - b) <= tanv * dist
    while frontier.any():
        nxt = np.zeros(H.shape, dtype=bool)
        for dr, dc, _ in steps:
            ok = ok_edge[(dr, dc)]
            src = frontier[max(-dr, 0):n + min(-dr, 0), max(-dc, 0):n + min(-dc, 0)] & ok
            nxt[max(dr, 0):n + min(dr, 0), max(dc, 0):n + min(dc, 0)] |= src
        if blocked is not None:
            nxt &= ~blocked
        nxt &= ~seen
        seen |= nxt
        frontier = nxt
    return seen


def measurements(site, H, fields, grid, Dr, street, course):
    rv = site.rv
    f = np.linspace(0.0, 1.0, 1001)
    widths = [float(rv.half_width(np.array(x), -1) + rv.half_width(np.array(x), 1)) for x in f[:-60]]
    X, Z = np.meshgrid(grid.axis, grid.axis)
    goal = course["goal"]
    near_goal = np.hypot(X - goal["x"], Z - goal["z"]) <= goal["radius_m"]
    plateau = fields["E"] > (fields["tw"] + 70.0)
    free = reachable(H, grid, site.spawn, UNCLIMBABLE_DEG, blocked=near_goal, face_deg=UNCLIMBABLE_DEG)
    route = reachable(H, grid, site.spawn, TIGER_PITCH_DEG)
    gz_r, gz_c = int(round(grid.col(goal["z"]))), int(round(grid.col(goal["x"])))
    counts = {}
    for b in site.buildings:
        key = ("razed_" if b["razed"] else "") + b["typology"]
        counts[key] = counts.get(key, 0) + 1
    by_third = [0, 0, 0]
    for bt, tanks in zip(BEATS, (3, 2)):
        fr = sum(p[0] for p in bt[1]) / len(bt[1])
        by_third[min(int(fr * 3), 2)] += tanks
    for gid, (fa, fb), _ in GUN_ZONES:
        by_third[min(int((fa + fb) / 2 * 3), 2)] += 1
    return dict(
        length_m=r3(rv.L),
        floor_width_m=dict(min=r3(min(widths)), max=r3(max(widths))),
        chokes=[dict(frac=c, width_m=r3(float(rv.width(c)))) for c in CHOKES],
        depth_m=dict(min=r3(float(np.min((fields["P"] - fields["floor"])[fields["E"] < 0][fields["F"][fields["E"] < 0] < RAMP_START]))),
                     max=r3(float(np.max((fields["P"] - fields["floor"])[fields["E"] < 0])))),
        unclimbable_deg=r3(UNCLIMBABLE_DEG),
        plateau_reached_without_the_goal=int((free & plateau).sum()),
        goal_reached_at_35_deg=bool(route[gz_r, gz_c]),
        buildings=dict(standing=sum(1 for b in site.buildings if not b["razed"]),
                       razed=sum(1 for b in site.buildings if b["razed"]), by_typology=counts),
        dressing=dict(sorted(Dr.counts.items())),
        gun_site_candidates={z["id"]: len(z["candidates"]) for z in course["gun_zones"]},
        garrison=len(course["garrison"]),
        defenders_by_third_tanks_plus_guns=by_third,
    )


# ════════════════════════════════════════════════════════════════════════════════════════════════
# output
# ════════════════════════════════════════════════════════════════════════════════════════════════
def build():
    rv = Ravine()
    grid = uid.Grid(RES, SPACING)
    X, Z, H, fields = compose_terrain(rv, grid)
    site = Site(rv, grid, H, fields)
    place_buildings(site)
    mounds, craters = apply_pads(site, X, Z, H)
    site.H = H
    Dr = dress(site, H)
    street, course = defence(site, Dr)
    rust_limits = uid.read_rust_limits()
    instances = []
    for b in site.buildings:
        if b["razed"]:
            continue
        inst = {"id": b["id"], "typology": b["typology"], "seed": int(b["seed"]), "width_m": float(b["width_m"]),
                "depth_m": float(b["depth_m"]), "storeys": int(b["storeys"]), "cellar": bool(b["cellar"]),
                "condition": b["condition"], "x": float(b["x"]), "z": float(b["z"]), "yaw_deg": float(b["yaw_deg"])}
        errs = uid.within_limits(inst, rust_limits[b["typology"]]) + uid.within_limits(inst, uid.DESIGN_LIMITS_AS_RUST[b["typology"]])
        if errs:
            raise SystemExit(f"building {b['id']} outside the grammar's limits: {errs}")
        instances.append(inst)
    meas = measurements(site, H, fields, grid, Dr, street, course)
    return rv, grid, site, H, fields, Dr, street, course, instances, mounds, craters, meas


def manifest_document(site, H, Dr, course):
    keys = sorted({it[0] for it in Dr.items})
    assets = [ASSETS[k] for k in keys]
    index = {k: i for i, k in enumerate(keys)}
    placements = [[index[k], x, y, z, deg, s] for (k, x, y, z, deg, s) in Dr.items]
    g = site.grid
    st, gl = course["start"], course["goal"]
    markers = [
        dict(name="canyon_start", kind="Spawn", x=st["x"], z=st["z"], y=r3(g.sample(H, st["x"], st["z"]))),
        dict(name="canyon_goal", kind="Goal", x=gl["x"], z=gl["z"], y=r3(g.sample(H, gl["x"], gl["z"]))),
    ]
    for h in Dr.hard:
        if h["cls"] == "wreck":
            markers.append(dict(name="canyon_wrecks", kind="WreckField", x=h["x"], z=h["z"], y=r3(g.sample(H, h["x"], h["z"]))))
            break
    mid = site.rv.world(0.5, 0.0)
    return {
        "schema": "kessel.act", "version": 1, "act_id": 1, "act": "Die Stadt",
        "seed": SEED & 0xFFFFFFFF,
        "authoring": {"mode": "course", "tool": TOOL, "tool_version": TOOL_VERSION},
        "terrain": {"res": RES, "spacing": SPACING, "heights": "heights.npy", "color": [0.82, 0.86, 0.92, 1.0],
                    "texture": TERRAIN_TEXTURE},
        "assets": assets, "placements": placements, "markers": markers, "wires": [],
        "overview": {"pos": [r3(mid[0] + 900.0), 700.0, r3(mid[1] + 300.0)], "target": [r3(mid[0]), 10.0, r3(mid[1])]},
    }


def write(dest=None, plan=None):
    rv, grid, site, H, fields, Dr, street, course, instances, mounds, craters, meas = build()
    dest = Path(dest) if dest else ROOT / DEST_REL
    if not dest.is_absolute():
        dest = ROOT / dest
    dest.mkdir(parents=True, exist_ok=True)
    manifest = manifest_document(site, H, Dr, course)
    (dest / "manifest.v1.json").write_text(dumps(manifest, indent=None))
    np.save(dest / "heights.npy", H.astype(np.float32))
    edits = {
        "base_placements": len(manifest["placements"]), "base_markers": len(manifest["markers"]), "ops": [],
        "building_instances": instances, "street_encounter": street, COURSE_KEY: course,
        "environment_lighting": LIGHTING, "display_tuning": DISPLAY,
        "sound_emitters": [], "authored_lights": [],
    }
    (dest / "manifest_edits.json").write_text(dumps(edits))
    receipt = dict(
        schema="kessel.canyon-course.receipt", version=1, tool=TOOL, tool_version=TOOL_VERSION, seed=SEED,
        command="python3 " + TOOL,
        outputs={f: sha256_file(dest / f) for f in OUTPUT_FILES},
        measurements=meas, mounds=mounds, craters=craters,
        buildings=[{k: b[k] for k in ("id", "group", "typology", "width_m", "depth_m", "storeys", "condition",
                                      "x", "z", "yaw_deg", "frac", "razed", "pad_m")} for b in site.buildings],
        parameters=dict(res=RES, spacing=SPACING, width_profile=WIDTH_PROFILE, chokes=CHOKES, pockets=POCKETS,
                        plateau_m=PLATEAU_M, cliff_deg=CLIFF_DEG, talus_w=TALUS_W, talus_h=TALUS_H,
                        tiger_pitch_deg=TIGER_PITCH_DEG, tiger_tip_deg=r3(TIGER_TIP_DEG),
                        unclimbable_deg=r3(UNCLIMBABLE_DEG), groups=[g[:2] + g[2:3] + (g[4],) for g in GROUPS]),
        provenance=PROVENANCE, limitations=LIMITATIONS,
    )
    (dest / RECEIPT_NAME).write_text(dumps(receipt))
    if plan:
        render_plan(site, H, fields, street, course, plan)
    return dest, meas


def render_plan(site, H, fields, street, course, path):
    from PIL import Image, ImageDraw
    grid = site.grid
    lo, hi = float(H.min()), float(H.max())
    img = ((H - lo) / max(hi - lo, 1e-6) * 180 + 40).astype(np.uint8)
    rgb = np.stack([img, img, img], axis=-1)
    im = Image.fromarray(rgb, "RGB")
    dr = ImageDraw.Draw(im)
    px = lambda x, z: (grid.col(x), grid.col(z))
    for b in site.buildings:
        x0, x1, z0, z1 = b["rect"]
        c = (140, 80, 60) if b["razed"] else (220, 110, 70)
        dr.rectangle([px(x0, z0), px(x1, z1)], outline=c)
    for bt in street["beats"]:
        pts = [px(*p) for p in bt["points"]]
        dr.line(pts + pts[:1], fill=(240, 210, 70))
    for zone in course["gun_zones"]:
        for i in zone["candidates"]:
            s = street["gun_sites"][i]
            x, y = px(s["x"], s["z"])
            dr.point((x, y), fill=(255, 60, 60))
    st, gl = course["start"], course["goal"]
    for (p, c, r) in ((st, (255, 255, 255), 4), (gl, (80, 255, 120), GOAL_RADIUS_M / grid.sp)):
        x, y = px(p["x"], p["z"])
        dr.ellipse([x - r, y - r, x + r, y + r], outline=c)
    im.save(path)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dest", default=None, help="workshop directory (default: %s)" % DEST_REL)
    ap.add_argument("--plan", default=None, help="also write a plan image here")
    args = ap.parse_args(argv)
    dest, meas = write(args.dest, args.plan)
    print(f"workshop: {dest}")
    print(json.dumps(meas, indent=1, sort_keys=True))


if __name__ == "__main__":
    main()
