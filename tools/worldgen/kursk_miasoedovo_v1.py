#!/usr/bin/env python3
"""Kursk, south face, July 1943: a reverse slope held by a Tiger against attacks out of a wooded
valley. A 2.5 km square of summer chalk steppe, workshop `kursk-miasoedovo-v1`.

THE SOURCE EPISODE (docs/NARRATIVE_PLAYTEST_VON_ROSEN_20261109.md, episode 2; a single source, an
excerpt of Richard von Rosen's memoir, summarised there, his text not reproduced): 7 to 10 July
1943 near Miasoedovo, Leutnant von Rosen's platoon of 3./s.Pz.Abt. 503. Four Tigers hold a
reverse slope in the heat. He takes one Tiger forward to scout a wooded valley, halts about
400 m out, takes three hits from two hidden assault guns, none through, fires back and reverses
out while two Tigers cover. The Soviet tanks withdraw the next morning.

WHAT IS FROM THE SOURCE: a reverse slope held in July heat; a wooded valley ahead of it; one
Tiger going forward over the crest alone; fire from two hidden guns at about 400 m that does not
penetrate; reversing back behind the crest.

WHAT IS ADAPTED:
- The game has no 1943 Soviet assault gun. The two hidden guns are 45 mm anti-tank guns sited by
  the game among candidates in front of the wood's edge (they open fire inside 400 m, the game's
  own limit), with two T-34s in the wood behind them.
- The player's tank is the game's Panzer 231, not a numbered Tiger of 3./503. Solo by default
  (one Tiger and its crew); the launcher's --platoon adds two AI Tigers, still not the four of
  the episode.
- The sky is computed for the game's Act One site (48.7 N, 44.5 E), not for Belgorod.

WHAT IS EXAGGERATED FOR PLAY (not in the source): three armoured attacks of rising size on the
ridge, out of the valley and across the open ground beyond it, where the source has one scouting
clash and a withdrawal; a garrison of riflemen in the wood; the numbers of tanks.

WHAT IS GENERATED, NOT SURVEYED: all of the ground. The ridge, the valley (balka), the wood, the
collective-farm sheds, the hamlet and the track are a generated landscape in the character of the
chalk steppe east of Belgorod. It is not a survey of Miasoedovo, and no building stands where a
real one stood. The buildings are the building grammar's typologies standing in for a kolkhoz's
sheds and a hamlet's timber houses.

Layout (x east, z south, metres; u is distance north of the ridge's main crest):
- the approach track from the south edge, the camp and half-track in the hollow behind the ridge;
- the start on the reverse slope, 380 m behind the main crest; the hold position 100 m behind it;
- the main crest; a forward crest 120 m beyond it where the slope breaks toward the valley;
- the wooded valley 470 to 690 m beyond the main crest (350 to 570 m beyond the forward crest),
  a ride through the wood with a glade on the valley floor;
- open ground east of the wood: there the ridge has no forward crest, and its east shoulder, 340 m
  east of the track, looks into the open valley and across to the far slope (the long fields of
  fire); the far slope's track and the collective-farm sheds are in view from the main crest;
- a far rise 960 m out and dead ground behind it, where the hamlet stands.

Built on the spring steppe grammar (tools/worldgen/steppe_spring.py) and the course grammar it
uses; the vegetation set and helpers of the spring bridge map are reused by import.

Run: KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem python3 tools/worldgen/kursk_miasoedovo_v1.py [--plan PNG]
"""
import argparse
import json
import math
import sys
from pathlib import Path

import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import spring_bridge_v2 as v2  # noqa: E402

v1, sp, cc, gaea = v2.v1, v2.sp, v2.cc, v2.gaea
ASSET_ROOT = v2.ASSET_ROOT
NAME = 'kursk-miasoedovo-v1'
DEST = ASSET_ROOT/'assets/worldgen/workshops'/NAME
DEST_REL = f'assets/worldgen/workshops/{NAME}'
TOOL = 'tools/worldgen/kursk_miasoedovo_v1.py'
VERSION = 1
SEED = 0x4B55_1943
EXTENT, RES, TEX = sp.EXTENT, sp.RES, sp.TEX
CROP_HALF = 1250.0
KNOT_M = 10.0                       # course route knot spacing: a patrol corner every 100 m

# ── the designed ground ──────────────────────────────────────────────────────────────────────────
# Height (m, relative to the main crest) against u, the distance north of the main crest line.
PROFILE = [(-1600, -9.0), (-900, -13.0), (-620, -14.5), (-380, -11.0), (-100, -3.2), (0, 0.0), (50, -0.4),
           (120, -1.2), (200, -7.0), (330, -15.0), (470, -22.5), (580, -28.5), (690, -22.0), (830, -13.0),
           (900, -7.5), (960, -2.5), (1020, -8.5), (1080, -12.5), (1200, -14.0), (2700, -15.0)]
# East of the wood the ridge has no forward shoulder: the slope falls from the main crest straight
# into the open valley, so the east flank lies in view from the crest (the fields of fire). The
# profile changes over x = EAST_BLEND; these points replace the stretch from the crest to the wood.
PROFILE_EAST = {50: -1.2, 120: -6.0, 200: -11.5, 330: -18.5}
EAST_BLEND = (150.0, 400.0)
U_HOLD, U_START, U_CAMP = -100.0, -380.0, -610.0
U_FORWARD = 120.0
U_WOOD_S, U_FLOOR, U_WOOD_N = 470.0, 580.0, 690.0
U_FAR_CREST = 960.0
WOOD_X = (-620.0, 330.0)
RIDE_HALF_M = 28.0                  # the ride through the wood: trees stand this far off the track
GLADE = (150.0, 110.0)              # the glade on the valley floor: full length along the track, width
TREE_M = 12.0                       # canopy height used by the sight-line checks (meadow.rs uses the same)
EYE_M, ROOF_M, T34_M = 2.5, 2.9, 2.7   # Tiger commander's eye, Tiger roof (canyon_game.rs), T-34 height

# Buildings: the kolkhoz sheds on the far slope, the hamlet in the dead ground behind the far rise.
# Each stands along a stretch of the track given by its two ends as (x, u); the course grammar
# takes the stretch as fractions of the laid route (see `build`).
GROUP_SPECS = [('kolkhoz', ((175.0, 795.0), (345.0, 890.0)), (20.0, 120.0), {'workshop': .65, 'wooden_house': .35}, 11),
               ('hamlet', ((640.0, 1045.0), (880.0, 1105.0)), (18.0, 115.0), {'wooden_house': .88, 'workshop': .12}, 17)]

# Attacks: (id, tanks, where they enter, tanks by the consequence and where). Cumulative kills that
# close each attack keep the living tanks at or under 10 (see `mission_sheet`).
OPENING_T34S = 5                    # winter_encounter_ai::PLATOON_TANKS [3, 2], fielded by every course
ATTACKS = [('attack_1', 2, 'glade', 0, None), ('attack_2', 3, 'far', 0, None), ('attack_3', 3, 'far', 2, 'glade')]
ATTACK_TIMEOUT_S = 720.0            # an attack still open after this long fails forward (the next beat opens)


def crest_z(x):
    """The main crest line: z of the crest at x (it wanders a little, as a real ridge does)."""
    return 55.0+26.0*np.sin((np.asarray(x, dtype=np.float64)-140.0)/540.0)


def at_u(x, u):
    """World (x, z) at `u` metres north of the main crest."""
    return np.array([float(x), float(crest_z(x))-u])


def heights(rng):
    """The designed profile across the ridge and the valley, with the steppe's roll over it."""
    axis = np.linspace(-EXTENT/2, EXTENT/2, RES)
    X, Z = np.meshgrid(axis, axis)
    u = crest_z(X)-Z
    us = np.array([p[0] for p in PROFILE], dtype=np.float64)
    hs = np.array([p[1] for p in PROFILE], dtype=np.float64)
    he = np.array([PROFILE_EAST.get(p[0], p[1]) for p in PROFILE], dtype=np.float64)
    east = cc.smoothstep((X-EAST_BLEND[0])/(EAST_BLEND[1]-EAST_BLEND[0]))
    base = (np.interp(u, us, hs)*(1-east)+np.interp(u, us, he)*east)*(1.0+0.07*np.sin(X/760.0+0.9))-0.0025*X
    base = cc.gaussian_blur(base, 2.5)
    H = base+0.75*sp.band_noise(rng, RES, 500, 1500)+0.25*sp.band_noise(rng, RES, 90, 300) \
        + 0.06*sp.band_noise(rng, RES, 18, 60)
    # The balka's floor: greener ground in patches along it (dry in July; no stream is modelled).
    floor = np.exp(-((u-U_FLOOR)/36.0)**2)*np.clip(0.55+0.25*sp.band_noise(rng, RES, 60, 240), 0.2, 0.85)
    return H-H.min()+30.0, floor, u


# ── the course route ─────────────────────────────────────────────────────────────────────────────
def lay_route():
    """South edge, camp hollow, start, hold, both crests, the ride and the glade, the far slope,
    the far rise, the hamlet. A Catmull-Rom curve through the control points, a knot every 10 m."""
    c = [np.array([-45.0, 1186.0]), np.array([-40.0, 1000.0]), at_u(-30, U_CAMP-90), at_u(-10, U_START),
         at_u(0, U_HOLD), at_u(0, 0), at_u(6, U_FORWARD), at_u(-10, 300), at_u(-35, U_WOOD_S),
         at_u(-85, U_FLOOR-55), at_u(-120, U_FLOOR), at_u(-95, U_FLOOR+50), at_u(20, U_WOOD_N+10),
         at_u(170, 790), at_u(330, 880), at_u(500, U_FAR_CREST), at_u(660, 1050), at_u(880, 1105),
         at_u(1020, 1130), at_u(1150, 1140)]
    pts = []
    for i in range(len(c)-1):
        p0, p1, p2, p3 = c[max(i-1, 0)], c[i], c[i+1], c[min(i+2, len(c)-1)]
        for t in np.linspace(0, 1, 24, endpoint=False):
            pts.append(0.5*((2*p1)+(-p0+p2)*t+(2*p0-5*p1+4*p2-p3)*t*t+(-p0+3*p1-3*p2+p3)*t**3))
    pts.append(c[-1])
    pts = np.array(pts)
    s = np.r_[0.0, np.cumsum(np.hypot(*np.diff(pts, axis=0).T))]
    sample = np.r_[np.arange(0, s[-1], KNOT_M), s[-1]]
    return np.column_stack([np.interp(sample, s, pts[:, i]) for i in range(2)])


class DesignedRoute(gaea.GaeaRoute):
    """The course grammar's route object over the designed polyline (no path search)."""
    polyline = None

    def __init__(self, H):
        route = self.polyline
        self.x, self.z = route.T.copy()
        self.s = np.r_[0.0, np.cumsum(np.hypot(np.diff(self.x), np.diff(self.z)))]
        self.L = float(self.s[-1])
        dx, dz = np.gradient(self.x), np.gradient(self.z)
        n = np.hypot(dx, dz)
        self.tx, self.tz = dx/n, dz/n


def knot_near(route, p):
    return int(np.argmin(np.hypot(route[:, 0]-p[0], route[:, 1]-p[1])))


def knot_at_u(route, u, after=0):
    """First knot at or beyond `u` (north of the crest), searching from knot `after`."""
    us = crest_z(route[:, 0])-route[:, 1]
    return after+int(np.argmax(us[after:] >= u))


# ── sight lines over the generated ground ────────────────────────────────────────────────────────
class Ground:
    """The terrain's own triangles (src/terrain.rs) for vectors of points, and the wood's canopy."""

    def __init__(self, H, woods=None):
        self.H, self.sp, self.half = H, EXTENT/(RES-1), (RES-1)*0.5
        self.woods = woods

    def y(self, x, z):
        gx = np.clip(np.asarray(x, dtype=np.float64)/self.sp+self.half, 0, RES-1-1e-9)
        gz = np.clip(np.asarray(z, dtype=np.float64)/self.sp+self.half, 0, RES-1-1e-9)
        ix, iz = gx.astype(int), gz.astype(int)
        tx, tz = gx-ix, gz-iz
        a, b, c, d = self.H[iz, ix], self.H[iz, ix+1], self.H[iz+1, ix], self.H[iz+1, ix+1]
        return np.where(tx+tz <= 1, a+(b-a)*tx+(c-a)*tz, d+(c-d)*(1-tx)+(b-d)*(1-tz))

    def wooded(self, x, z):
        if self.woods is None:
            return np.zeros(np.shape(x), dtype=bool)
        ix = np.clip(np.rint(np.asarray(x)/self.sp+self.half).astype(int), 0, RES-1)
        iz = np.clip(np.rint(np.asarray(z)/self.sp+self.half).astype(int), 0, RES-1)
        return self.woods[iz, ix]

    def clearance(self, a, ah, b, bh, canopy=False):
        """Least height of the sight line from `a` (+ah above ground) to `b` (+bh) over the ground
        (and the canopy, with `canopy`) between them; negative means the line is blocked."""
        a, b = np.asarray(a, dtype=np.float64), np.asarray(b, dtype=np.float64)
        n = max(int(np.hypot(*(b-a))/3.0), 4)
        t = np.linspace(0, 1, n+1)[1:-1]
        x, z = a[0]+(b[0]-a[0])*t, a[1]+(b[1]-a[1])*t
        ya, yb = float(self.y(a[0], a[1]))+ah, float(self.y(b[0], b[1]))+bh
        ground = self.y(x, z)
        if canopy:
            near = (t*np.hypot(*(b-a)) > 8.0) & ((1-t)*np.hypot(*(b-a)) > 8.0)
            ground = ground+TREE_M*(self.wooded(x, z) & near)
        return float(np.min(ya+(yb-ya)*t-ground))

    def sees(self, a, ah, b, bh, canopy=False):
        return self.clearance(a, ah, b, bh, canopy) > 0.0

    def game_sees(self, a, ah, b, bh, trees):
        """The game's own sight line (src/game.rs `los_clear`): the terrain and the standing trees'
        trunks (0.30 m at scale 1, src/trees.rs), not their crowns."""
        if not self.sees(a, ah, b, bh):
            return False
        a, b = np.asarray(a, dtype=np.float64), np.asarray(b, dtype=np.float64)
        ab = b-a
        t = np.clip(((trees-a)@ab)/float(ab@ab), 0.0, 1.0)
        d = np.hypot(*(trees-(a+t[:, None]*ab)).T)
        return not bool(np.any((d < 0.33) & (t > 0.01) & (t < 0.99)))


# ── the wood ─────────────────────────────────────────────────────────────────────────────────────
def wood_mask(rng, u, route, buildings, glade_at, glade_dir):
    """The wood in the valley on the 1025 grid: between its two edges (ragged), its west and east
    ends, clear of the ride along the track and of the glade on the floor."""
    axis = np.linspace(-EXTENT/2, EXTENT/2, RES)
    X, Z = np.meshgrid(axis, axis)
    ragged = 16.0*sp.band_noise(rng, RES, 120, 400)
    mask = (u > U_WOOD_S+ragged) & (u < U_WOOD_N+0.6*ragged) & (X > WOOD_X[0]+2*ragged) & (X < WOOD_X[1]+2*ragged)
    near = np.full(X.shape, np.inf)
    for p in route[::1]:
        near = np.minimum(near, np.hypot(X-p[0], Z-p[1]))
    mask &= near > RIDE_HALF_M
    along = (X-glade_at[0])*glade_dir[0]+(Z-glade_at[1])*glade_dir[1]
    across = -(X-glade_at[0])*glade_dir[1]+(Z-glade_at[1])*glade_dir[0]
    mask &= (along/(GLADE[0]/2))**2+(across/(GLADE[1]/2))**2 > 1.0
    for b in buildings:
        mask &= np.hypot(X-b['x'], Z-b['z']) > max(b['width_m'], b['depth_m'])*0.5+14.0
    return mask


def plant(manifest, H, woods, P, extra):
    """The wood's trees (the steppe grammar's jittered grid) in the summer foliage of the spring
    set, an understorey of hazel and hawthorn; then the lone trees."""
    sp.WOOD_SPACING_M = 7.0
    sp.WOOD_TREES = ['tree_5', 'tree_6', 'tree_7', 'tree_1']
    trees = sp.plant_woods(manifest, H, woods)
    weights = np.cumsum([w for _, w in v2.WOOD_MIX])/sum(w for _, w in v2.WOOD_MIX)
    for p in manifest['placements'][-trees:] if trees else []:
        key = f'kwood{p[1]:.1f},{p[3]:.1f}'
        p[0] = P.asset(v2.veg_path(v2.WOOD_MIX[int(np.searchsorted(weights, v2.rnd(key, 1)))][0]))
        p[5] = cc.r3(0.8+0.35*v2.rnd(key, 3))
    for k, (x, z, pool) in enumerate(extra):
        key = f'ktree{k}'
        P.put(None, x, z, v2.rnd(key, 1)*360, 0.85+0.3*v2.rnd(key, 2), sink=0.05, tag='lone_trees',
              path=v2.veg_path(pool[int(v2.rnd(key, 3)*len(pool))]))
    return trees


def tree_points(manifest):
    ids = {i for i, a in enumerate(manifest['assets']) if any(t in a for t in ('/tree_', 'pine_tree', '/shrub_'))}
    pts = np.array([[q[1], q[3]] for q in manifest['placements'] if q[0] in ids])
    return pts if len(pts) else np.zeros((0, 2))


# ── the summer ground ────────────────────────────────────────────────────────────────────────────
def july(rng, colour, grass, H, floor, woods):
    """July over the spring grammar's paint: the sward stands everywhere it can (the meadow stands
    in for the standing rye and wheat too; it does not model grain), the open ground has dried
    toward straw, and chalk shows where the slopes steepen."""
    slope = sp.upsample(np.hypot(*np.gradient(H, EXTENT/(RES-1))))
    wood = np.clip(sp.upsample(woods.astype(np.float32)), 0, 1)
    wet = np.clip(sp.upsample(floor.astype(np.float32))*1.2, 0, 1)
    patch = sp.band_noise(rng, TEX, 25, 140).astype(np.float32)
    stand = grass > 0.3
    grass[stand] = np.maximum(grass[stand], 0.88)
    grass *= 1.0-0.75*wood
    open_ = (1-wood)*(1-wet)
    luma = (colour*np.array([0.3, 0.55, 0.15], dtype=np.float32)).sum(axis=-1)
    # The grammar's ploughed strips: by July a field left unsown that spring is weeds and old stubble.
    fallow = (luma < 0.19) & (open_ > 0.5) & stand
    colour[fallow] = (np.array([0.40, 0.41, 0.20], dtype=np.float32)*(0.92+0.10*patch[fallow, None]))
    luma = (colour*np.array([0.3, 0.55, 0.15], dtype=np.float32)).sum(axis=-1)
    k = (0.42*open_*stand)[..., None]
    colour[:] = colour*(1-k)+np.array([0.56, 0.50, 0.26], dtype=np.float32)*(luma/0.31)[..., None]*k
    chalk = np.clip((slope-0.055)/0.03, 0, 1)*np.clip(0.35+0.9*patch, 0, 1)*open_*0.6
    colour[:] = colour*(1-chalk[..., None])+np.array([0.74, 0.72, 0.63], dtype=np.float32)*chalk[..., None]
    grass *= 1.0-0.8*chalk
    return np.clip(colour, 0, 1), np.clip(grass, 0, 1)


# ── the mission ──────────────────────────────────────────────────────────────────────────────────
def line(text, sender='Adler'):
    return [{'from': sender, 'text': text}]


def mission_sheet(places, fractions):
    """The order: go forward and look into the valley, come back, hold the ridge through three
    attacks, destroy every tank. Only beat kinds, cues and pacing src/mission.rs implements."""
    p = lambda q: {'x': cc.r3(q[0]), 'z': cc.r3(q[1])}
    hold, mouth = places['hold'], places['wood_mouth']
    look_r = cc.r3(float(np.hypot(*(places['forward_crest']-mouth)))+35.0)
    kills = 0
    fielded = OPENING_T34S
    beats = [
        {'id': 'position', 'title': 'The reverse slope', 'goal': 'reach', 'at': p(hold), 'radius_m': 70.0,
         'complete': {'kind': 'enter', 'dwell_s': 2.0},
         'lines': {'open': line('Kiebitz, Adler. Reverse slope 280 metres north of you. Halt below the crest. Out.'),
                   'done': line('Kiebitz, Adler. Wooded valley 500 metres beyond the crest. Go forward and look into it. Out.')},
         'map': {'mark': 'box', 'bearing': True}},
        {'id': 'look', 'title': 'Look into the valley', 'goal': 'observe', 'at': p(mouth), 'radius_m': look_r,
         'complete': {'kind': 'observe', 'cone_deg': 70.0, 'dwell_s': 6.0},
         'pacing': {'nudge_after_s': 180.0},
         'lines': {'open': line('Adler, Kiebitz. Going forward over the crest. Out.', 'Kiebitz'),
                   'nudge': line('Kiebitz, Adler. Onto the forward crest, where the slope breaks. Out.'),
                   'done': line('Adler, Kiebitz. On the forward crest. The wood\'s edge 350 metres ahead. Out.', 'Kiebitz')},
         'map': {'mark': 'circle', 'bearing': True}},
        {'id': 'first_fire', 'title': 'Fire from the wood', 'goal': 'survive_until', 'at': p(mouth), 'radius_m': look_r,
         'complete': {'kind': 'survive', 'seconds': 40.0},
         'cues': [{'kind': 'alert', 'at': p(mouth), 'radius_m': 320.0}],
         'lines': {'open': line('Kiebitz, Adler. They have heard you. Out.'),
                   'done': line('Kiebitz, Adler. Come back behind the crest. Reverse, keep your front to them. Out.')},
         'map': {'mark': 'circle', 'bearing': False}},
        {'id': 'reverse_out', 'title': 'Back behind the crest', 'goal': 'reach', 'at': p(hold), 'radius_m': 70.0,
         'complete': {'kind': 'enter', 'dwell_s': 2.0},
         'pacing': {'nudge_after_s': 150.0},
         'lines': {'nudge': line('Kiebitz, Adler. Behind the crest. Out.'),
                   'done': line('Kiebitz, Adler. They will come for the ridge. Hold it. The east shoulder sees the open ground. Out.')},
         'map': {'mark': 'box', 'bearing': True}},
    ]
    words = {2: 'Two', 3: 'Three', 5: 'Five', 10: 'Ten'}
    opens = {'attack_1': 'Kiebitz, Adler. Two tanks in the valley, coming up the track. Hold the ridge. Out.',
             'attack_2': 'Kiebitz, Adler. Three tanks behind the far rise, 1,000 metres, coming across the open ground. Out.',
             'attack_3': 'Kiebitz, Adler. Third attack. Armour over the far rise again, more of it. Out.'}
    for k, (aid, count, entry, later, later_entry) in enumerate(ATTACKS):
        fielded += count+later
        kills += count+later
        alive_cap = fielded-(kills-count-later)      # the most that can be alive while this beat is open
        assert alive_cap <= 10, (aid, alive_cap)
        beat = {'id': aid, 'title': f'Hold: the {("first", "second", "third")[k]} attack', 'goal': 'destroy',
                'at': p(hold), 'radius_m': 150.0,
                'complete': {'kind': 'destroy_near', 'count': kills, 'radius_m': 1700.0},
                # The stall guard: a platoon the game could not site is never sent again
                # (src/mission_game.rs), and without it the kills could not be had.
                'fail': {'kind': 'timeout', 'seconds': ATTACK_TIMEOUT_S},
                'cues': [{'kind': 'alert', 'at': p(places[entry]), 'radius_m': 420.0 if k == 2 else 70.0}],
                'pacing': {'release': {'count': count, 'fraction': fractions[entry]}, 'nudge_after_s': 240.0},
                'lines': {'open': line(opens[aid]),
                          'nudge': line('Kiebitz, Adler. The attack is not broken yet. Find them from the crest. Out.'),
                          'failed': line('Kiebitz, Adler. No count from your front. Stay on the ridge. Out.'),
                          'done': line(f'Kiebitz, Adler. {words.get(kills, str(kills))} of theirs destroyed. Out.')},
                'map': {'mark': 'cross', 'bearing': False}}
        if later:
            beat['pacing'].update(consequence_after_s=60.0, consequence={'count': later, 'fraction': fractions[later_entry]})
            beat['lines']['consequence'] = line('Kiebitz, Adler. Two more in the valley, on the track. Out.')
        beats.append(beat)
        if k < len(ATTACKS)-1:
            beats.append({'id': f'lull_{k+1}', 'title': 'Below the crest', 'goal': 'hold', 'at': p(hold), 'radius_m': 140.0,
                          'complete': {'kind': 'capture', 'seconds': 45.0, 'contest_radius_m': 420.0},
                          'lines': {'open': line('Kiebitz, Adler. Back below the crest. Keep the 400 metres in front of you clear. Out.')},
                          'map': {'mark': 'box', 'bearing': True}})
    beats.append({'id': 'all_tanks', 'title': 'Destroy every tank', 'goal': 'destroy', 'at': p(mouth), 'radius_m': 400.0,
                  'complete': {'kind': 'destroy_all'},
                  'pacing': {'nudge_after_s': 300.0},
                  'lines': {'open': line('Kiebitz, Adler. Finish whatever armour is left on this ground. Out.'),
                            'nudge': line('Kiebitz, Adler. Armour still alive on this ground. Out.'),
                            'done': line('Kiebitz, Adler. No armour left in front of the ridge. Out.')},
                  'map': {'mark': 'cross', 'bearing': False}})
    return {
        'schema': 'kessel.mission', 'version': 1, 'id': NAME, 'act_id': 1,
        'title': 'BEFEHL: HOEHE HALTEN', 'callsign': 'Kiebitz', 'net': 'Adler',
        'note': ('Kursk defensive test map (tools/worldgen/kursk_miasoedovo_v1.py). FROM THE SOURCE (one source, an excerpt '
                 'of Richard von Rosen\'s memoir as summarised in docs/NARRATIVE_PLAYTEST_VON_ROSEN_20261109.md, episode 2; '
                 'Miasoedovo, 7 to 10 July 1943, 3./s.Pz.Abt. 503): a reverse slope held in the heat, a wooded valley ahead, '
                 'one Tiger forward over the crest, non-penetrating fire from two hidden guns at about 400 m, reversing out. '
                 'ADAPTED: the game has no 1943 assault gun, so the two guns are 45 mm anti-tank guns in front of the wood\'s '
                 'edge with T-34s in the wood; the tank is the game\'s Panzer 231 with one crew (the episode had four Tigers; '
                 '--platoon adds two AI Tigers). EXAGGERATED FOR PLAY: three attacks of rising size (2, 3, then 3 and 2 more; '
                 '15 T-34s with the wood\'s and the hamlet\'s five) where the source has a scouting clash and a Soviet '
                 'withdrawal the next morning; the game has no withdrawal, so the sheet ends when every tank is destroyed. '
                 'An attack closes on cumulative kills (2, 5, 10), which keeps the living tanks at or under 10; an attack '
                 'still open after 12 minutes fails forward (the guard against a platoon the game could not site), and only '
                 'then can more be alive (15 at most). '
                 'GENERATED: the whole landscape; it is not a survey of Miasoedovo. Places are the map\'s own geometry: the '
                 'hold position below the main crest, the wood\'s mouth where the track enters it, the glade on the valley '
                 'floor and the dead ground behind the far rise where the attacks enter.'),
        'order': ['Panzer 231. Full crew aboard, you command. July 1943, east of Belgorod. The camp and the half-track are behind you on the track.',
                  'The ridge ahead is ours. Beyond its crest a wooded valley, 500 metres. Nobody has looked into it.',
                  'Halt on the reverse slope. Go forward over the crest and look. Then hold the ridge: expect armour out of the valley and across the open ground beyond it. Destroy every tank.'],
        'beats': beats}


# ── the build ────────────────────────────────────────────────────────────────────────────────────
def build(plan=None):
    rng = np.random.default_rng(SEED)
    H0, floor, u = heights(rng)
    route0 = lay_route()
    us0 = crest_z(route0[:, 0])-route0[:, 1]
    frac_at = lambda uu: float(np.argmax(us0 >= uu))/(len(route0)-1)
    DEST.mkdir(parents=True, exist_ok=True)
    sp.SOURCE = DEST/'source'
    source = sp.write_source(H0)
    DesignedRoute.polyline = route0
    gaea.GaeaRoute = DesignedRoute
    frac_near = lambda xu: knot_near(route0, at_u(*xu))/(len(route0)-1)
    cc.GROUPS = [(gid, (frac_near(a), frac_near(b)), band, weights, n) for gid, (a, b), band, weights, n in GROUP_SPECS]
    cc.GUN_ZONES = [('wood', (frac_at(40.0), frac_at(360.0)), (frac_at(380.0), frac_at(455.0)))]
    cc.LONE_RUINS = []
    cc.WALL_CLUSTERS = 0
    cc.CRATERS = 14
    cc.RAZED_PLOTS = 0
    cc.ASSETS['tree_tall'] = v2.veg_path('tree_linden_a')
    cc.ASSETS['tree_small'] = v2.veg_path('tree_apple_a')
    manifest, edits, H, receipt = gaea.build(source)
    route = np.array(edits['canyon_course']['route'], dtype=np.float64)
    assert np.abs(route).max() < CROP_HALF-60, 'the course route leaves the map'
    knots = len(route)
    us = crest_z(route[:, 0])-route[:, 1]
    grid = gaea.SurfaceGrid(RES, EXTENT/(RES-1))
    axis = grid.axis
    X, Z = np.meshgrid(axis, axis)
    buildings = [b for b in edits['building_instances'] if abs(b['x']) < CROP_HALF-60 and abs(b['z']) < CROP_HALF-60]
    edits['building_instances'] = buildings
    rects = [cc.footprint(b['x'], b['z'], b['width_m'], b['depth_m'], b['yaw_deg']) for b in buildings]

    # Places, as knots of the route.
    k_start, k_hold, k_crest = knot_at_u(route, U_START), knot_at_u(route, U_HOLD), knot_at_u(route, 0.0)
    k_forward, k_mouth = knot_at_u(route, U_FORWARD), knot_at_u(route, U_WOOD_S)
    k_glade = knot_near(route, at_u(-120, U_FLOOR))
    k_north = knot_at_u(route, U_WOOD_N+10, k_glade)
    k_far = knot_at_u(route, U_FAR_CREST, k_north)
    glade_dir = route[k_glade+3]-route[k_glade-3]
    glade_dir /= np.hypot(*glade_dir)

    # The grammar's dressing: nothing of it south of the wood (the ridge is open grass), no winter
    # drifts, no tank wrecks (the episode names none), nothing in the lane of the track.
    drop = {i for i, a in enumerate(manifest['assets']) if 'snow_drift' in a or 'wreck' in a or 'pine_tree' in a}
    lane = {}
    for x, z in route:
        lane.setdefault((int(x//32), int(z//32)), []).append((x, z))
    def off_lane(x, z, m):
        i, j = int(x//32), int(z//32)
        return all(math.hypot(x-a, z-b) >= m for di in (-1, 0, 1) for dj in (-1, 0, 1) for a, b in lane.get((i+di, j+dj), ()))
    placed = []
    for q in manifest['placements']:
        x, z = q[1], q[3]
        if q[0] in drop or abs(x) > CROP_HALF-5 or abs(z) > CROP_HALF-5:
            continue
        if crest_z(x)-z < U_WOOD_S-30 or not off_lane(x, z, 14.0):
            continue
        placed.append(list(q))
    manifest['placements'] = placed

    # The wood, the glade, the lone trees.
    woods = wood_mask(rng, u, route, buildings, route[k_glade], glade_dir)
    woods &= (np.abs(X) < CROP_HALF-20) & (np.abs(Z) < CROP_HALF-20)
    P = v2.Placer(manifest, H, grid)
    extra = []
    camp_at = at_u(-75, U_CAMP)
    for k in range(9):                                   # shade by the camp, west of the track
        a = math.tau*v2.rnd(f'shade{k}', 1)
        extra.append((camp_at[0]-75+34*math.cos(a)*v2.rnd(f'shade{k}', 2), camp_at[1]+40*math.sin(a), v2.STREET_TREES))
    for b, (x0, x1, z0, z1) in zip(buildings, rects):    # orchards behind the sheds and the houses
        for k in range(3):
            key = f"{b['id']}.orchard{k}"
            x, z = (x0+x1)/2+(v2.rnd(key, 1)-0.5)*44, (z0+z1)/2+(v2.rnd(key, 2)-0.5)*44
            if all(not (r[0]-4 < x < r[1]+4 and r[2]-4 < z < r[3]+4) for r in rects) and off_lane(x, z, 18.0):
                extra.append((x, z, v2.ORCHARD))
    trees = plant(manifest, H, woods, P, extra)
    tree_xz = tree_points(manifest)
    def tree_clear(x, z, m):
        return len(tree_xz) == 0 or float(np.hypot(tree_xz[:, 0]-x, tree_xz[:, 1]-z).min()) >= m
    def building_clear(x, z, m):
        return all(not (r[0]-m < x < r[1]+m and r[2]-m < z < r[3]+m) for r in rects)
    G = Ground(H, woods)
    def relief(x, z, half):
        y = float(G.y(x, z))
        return max(abs(float(G.y(x+sx*half, z+sz*half))-y) for sx in (-1, 1) for sz in (-1, 1))
    def tank_ground(x, z):
        """Room and footing for a T-34 as the game's open-ground probe asks (4.46 m half box,
        under 1.2 m of relief), judged here from the trees, the buildings and the heights only."""
        return tree_clear(x, z, 6.5) and building_clear(x, z, 8.0) and relief(x, z, 4.46) < 1.2

    # The course block.
    course, street = edits['canyon_course'], edits['street_encounter']
    start = route[k_start]
    ahead = route[k_start+3]-start
    course['start'] = dict(x=cc.r3(start[0]), z=cc.r3(start[1]), heading_deg=cc.r3(math.degrees(math.atan2(ahead[0], ahead[1]))))
    course['goal'] = dict(x=cc.r3(route[-1, 0]), z=cc.r3(route[-1, 1]), radius_m=85.0)
    course['length_m'] = cc.r3(float(np.hypot(*np.diff(route, axis=0).T).sum()))
    course['camp_at'] = [cc.r3(camp_at[0]), cc.r3(camp_at[1])]
    f_of = lambda k: round(k/(knots-1), 4)
    course['design'] = [dict(name=n, from_fraction=f_of(a), to_fraction=f_of(b)) for n, a, b in [
        ('approach_and_camp', 0, k_start), ('reverse_slope', k_start, k_crest), ('crest_and_forward_crest', k_crest, k_forward),
        ('forward_slope', k_forward, k_mouth), ('wooded_valley', k_mouth, k_north), ('far_slope_and_kolkhoz', k_north, k_far),
        ('dead_ground_and_hamlet', k_far, knots-1)]]

    # The two opening platoons (five T-34s, every course fields them): B, two tanks on the ride
    # north of the glade; A, three in the dead ground behind the far rise, east of the hamlet.
    def loop(ka, kb, off=5.0):
        pts = []
        for k in list(range(ka, kb+1, 3))+list(range(kb, ka-1, -3)):
            t = route[min(k+1, knots-1)]-route[max(k-1, 0)]
            t /= np.hypot(*t)
            side = 1.0 if len(pts) < len(range(ka, kb+1, 3)) else -1.0
            pts.append([cc.r3(route[k, 0]-t[1]*off*side), cc.r3(route[k, 1]+t[0]*off*side)])
        return pts
    kb0 = knot_at_u(route, U_FLOOR+75, k_glade)
    loop_b = loop(kb0, k_north+3)
    ka0 = knot_near(route, at_u(930, 1112))
    loop_a = loop(ka0, min(ka0+15, knots-4))
    street['beats'] = [dict(platoon='A', points=loop_a), dict(platoon='B', points=loop_b)]

    # Where the attacks enter: the glade on the valley floor and the dead ground behind the far
    # rise. The game sites the platoon at a knot near the fraction, its tanks 36 m apart on the
    # line away from the Tiger (src/canyon_game.rs `canyon_platoon_site`); the same is laid here
    # to choose a knot whose ground holds three tanks.
    def entry_positions(j, count=3):
        d = route[j-10]-route[j]
        d /= np.hypot(*d)
        return [route[j]-d*36.0*k for k in range(count)]
    def entry_ok(j):
        if not (60 <= j and j+12 < knots):
            return False
        loops = np.array(loop_a+loop_b)
        return all(tank_ground(q[0], q[1]) and float(np.hypot(loops[:, 0]-q[0], loops[:, 1]-q[1]).min()) > 24.0
                   for q in entry_positions(j))
    def entry_knot(target):
        for step in range(0, 14):
            for j in (target+step, target-step):
                if entry_ok(j):
                    return j
        raise ValueError(f'no ground for an attack to enter near knot {target}')
    entries = dict(glade=entry_knot(k_glade-2), far=entry_knot(knot_at_u(route, 1045.0, k_far)))
    fractions = {name: round((j+0.5)/knots, 4) for name, j in entries.items()}

    crest, forward = route[k_crest], route[k_forward]
    # Gun candidates: the grammar's lattice beside the track in front of the wood, and a line of
    # sites along the wood's south edge west of the open flank, 34 to 62 m in front of the trees (the
    # game asks for 25 m of walkable ground behind a gun). Kept: those with a line of sight over the
    # ground to three or more points of the kill zone at 100 to 400 m (at_guns.rs), clear of trees
    # and out of view from the main crest.
    kill_pts = route[k_forward-6:knot_at_u(route, 345.0)+1]
    kill_line = [[cc.r3(x), cc.r3(z)] for x, z in kill_pts[::4]]
    def field_of_fire(x, z):
        seen = []
        for q in kill_pts[::1]:
            r = math.hypot(q[0]-x, q[1]-z)
            if 100.0 <= r <= 400.0 and G.sees((x, z), 1.1, q, 1.5, canopy=True):
                seen.append(r)
        return seen
    cand = [(g['x'], g['z']) for i, g in enumerate(street['gun_sites'])]
    for x in np.arange(WOOD_X[0]+60, EAST_BLEND[0]-30, 22.0):
        for du in (34.0, 48.0, 62.0):
            col = np.where(woods[:, int(round(grid.col(x)))])[0]
            if len(col) == 0:
                continue
            edge_z = axis[col.max()]                     # the wood's southernmost tree row at this x
            cand.append((cc.r3(x), cc.r3(edge_z+du)))
    sites, fields = [], []
    for x, z in cand:
        if not (tree_clear(x, z, 6.0) and off_lane(x, z, 24.0) and relief(x, z, 2.0) < 0.8):
            continue
        if not all(tree_clear(x, z-s, 3.0) for s in (5.0, 10.0, 15.0, 20.0, 25.0)):
            continue
        if any(math.hypot(x-px, z-pz) < 14.0 for px, pz in loop_a+loop_b):
            continue
        if x > EAST_BLEND[0]-30 or G.sees(crest, EYE_M, (x, z), 1.2):
            continue                                     # hidden guns: none in view from the main crest
        seen = field_of_fire(x, z)
        if len(seen) >= 3:
            sites.append(dict(kind='ground', on='wood.edge', x=cc.r3(x), z=cc.r3(z)))
            fields.append(seen)
    assert len(sites) >= 8, f'only {len(sites)} gun candidates have a field of fire'
    street['gun_sites'] = sites
    course['gun_zones'] = [dict(id='wood', kill_zone='route.wood', candidates=list(range(len(sites))))]
    street['kill_zone'] = [dict(id='route.wood', role='route', weight=1.0, points=kill_line),
                           dict(id='route.whole', role='way_out', weight=0.1, points=[[cc.r3(x), cc.r3(z)] for x, z in route[::12]])]
    # Ways: the track's corridor, wide enough to deploy off it, and the whole forward slope.
    cors = [dict(a=[cc.r3(v) for v in a], b=[cc.r3(v) for v in b], width_m=300.0) for a, b in zip(route[::20], route[20::20])]
    cors.append(dict(a=[cc.r3(v) for v in route[-21]], b=[cc.r3(v) for v in route[-1]], width_m=300.0))
    street['ways']['corridors'] = cors
    street['ways']['district'] = [-CROP_HALF, CROP_HALF, -CROP_HALF, CROP_HALF]
    fz = float(crest_z(0.0))
    street['ways']['rects'] = [[cc.r3(start[0]-40), cc.r3(start[0]+40), cc.r3(start[1]-40), cc.r3(start[1]+40)],
                               [-520.0, 520.0, cc.r3(fz-U_WOOD_S-40), cc.r3(fz+160)]]
    street['tool'], street['note'] = TOOL, (
        'The Kursk ridge map\'s defence (tools/worldgen/kursk_miasoedovo_v1.py): two 45 mm guns sited by the game among '
        'the candidates in front of the wood\'s south edge over the kill zone on the forward crest and slope; platoon B '
        '(two T-34s) on the ride north of the glade, platoon A (three) in the dead ground behind the far rise.')
    street['rules'] = dict(gun_route_clear_m=24.0, gun_off_beat_m=14.0,
        candidates='in front of the wood\'s south edge, west of the open flank: the grammar\'s lattice beside the track (15 to '
                   '90 m in front of the trees) and a line 34, 48 and 62 m in front of them every 22 m; out of view from the main crest; kept where the generated ground gives a line of sight to three or more '
                   'kill-zone points at 100 to 400 m, 25 m of tree-free ground behind and under 0.8 m of relief')

    # Riflemen: posts in front of the wood's edge among the guns, at the wood's mouth, round the
    # glade and at the wood's north edge (the game adds posts at the hamlet's houses itself).
    posts = []
    def post(x, z, host):
        if tree_clear(x, z, 3.0) and building_clear(x, z, 2.0) and off_lane(x, z, 9.0) and relief(x, z, 0.5) < 0.3 \
                and all(math.hypot(x-q['x'], z-q['z']) >= 40.0 for q in posts):
            posts.append(dict(x=cc.r3(x), z=cc.r3(z), heading_deg=0.0, host=host))   # facing the ridge (south)
    for g in sites[::2]:
        post(g['x']+9.0, g['z']-6.0, 'wood.edge')
    for a in np.linspace(0, math.tau, 8, endpoint=False):
        post(route[k_glade, 0]+math.cos(a)*50, route[k_glade, 1]+math.sin(a)*38, 'glade')
    for k in range(k_north, k_north+16, 5):
        for side in (-1, 1):
            post(route[k, 0]+side*34.0, route[k, 1], 'wood.north')
    course['garrison'] = posts
    assert len(posts) >= 12, f'only {len(posts)} garrison posts'

    course.update(variant=NAME, tool=TOOL, tool_version=VERSION, finish='mission', director=False, opening_platoons=False,
                  headline='The ridge is held', camp=True, camp_start='tank',
                  brief=['Hold the ridge north of you against attacks out of the wooded valley beyond it.',
                         'Guns and T-34s hold the wood; more armour comes up the track and over the far rise.',
                         'Destroying every tank, alive, ends the run and keeps the time.'])
    course['note'] = ('A defensive course: the run ends with the mission sheet (every tank destroyed), not in the goal disc, '
                      'which lies at the track\'s far end behind the enemy. The street_encounter block carries the kill zone, '
                      'the gun candidates, the two opening platoons\' beats and the ways.')
    course['provenance'] = dict(
        terrain='Generated chalk steppe: a ridge, a wooded valley (balka), a far rise. In the character of the ground east of '
                'Belgorod; not a survey of Miasoedovo or of any real ground.',
        episode='Adapted from one episode: Leutnant Richard von Rosen\'s platoon of 3./s.Pz.Abt. 503 near Miasoedovo, 7 to 10 '
                'July 1943 (docs/NARRATIVE_PLAYTEST_VON_ROSEN_20261109.md, episode 2; a single source, a memoir excerpt). '
                'From it: the reverse slope, the wooded valley, one Tiger forward, non-penetrating fire from two hidden guns '
                'at about 400 m, reversing out.',
        adapted='The two assault guns are 45 mm anti-tank guns with T-34s behind them (the game has no 1943 assault gun). '
                'One Tiger with its crew, or three with --platoon, where the episode had four.',
        exaggerated='Three attacks of rising size on the ridge and a garrison of riflemen in the wood; the source has a '
                    'scouting clash and a Soviet withdrawal the next morning.',
        buildings=cc.PROVENANCE['buildings']+' Here: the grammar\'s workshops and timber houses standing in for a '
                  'collective farm\'s sheds and a hamlet, all burned or shelled; no real building is reproduced.',
        defence=cc.PROVENANCE['defence'],
        scope='A generated defensive test map, not a reconstruction of the engagement.')

    for m in manifest['markers']:
        if m['kind'] == 'Spawn':
            m.update(x=course['start']['x'], z=course['start']['z'], y=cc.r3(grid.sample(H, start[0], start[1])))
        if m['kind'] == 'Goal':
            m.update(x=course['goal']['x'], z=course['goal']['z'], y=cc.r3(grid.sample(H, route[-1, 0], route[-1, 1])))
    manifest['markers'] = [m for m in manifest['markers'] if m['kind'] in ('Spawn', 'Goal')]

    # Ground paint and grass, cropped with the heights.
    sp.SEED = SEED
    colour, grass = sp.ground(rng, H, floor, route, buildings, woods, clump_floor=0.9, bare=(42, 42))
    colour, grass = july(rng, colour, grass, H, floor, woods)
    axis_tex = np.linspace(-EXTENT/2, EXTENT/2, TEX, dtype=np.float32)
    t0, t1 = int(np.searchsorted(axis_tex, -CROP_HALF)), int(np.searchsorted(axis_tex, CROP_HALF))
    c = colour[t0:t1, t0:t1]
    rgba = np.dstack([np.uint8(np.rint(np.clip(c, 0, 1)*255)), np.full(c.shape[:2], 32, dtype=np.uint8)])
    Image.fromarray(rgba, 'RGBA').save(DEST/'ground_material.png')
    g = np.uint8(np.rint(np.clip(grass[t0:t1, t0:t1], 0, 1)*255))
    Image.fromarray(np.dstack([g, np.zeros_like(g), np.zeros_like(g)]), 'RGB').save(DEST/'ground_class.png')
    lo, hi = int(np.searchsorted(axis, -CROP_HALF-1e-3)), int(np.searchsorted(axis, CROP_HALF+1e-3))
    Hc = H[lo:hi, lo:hi]
    # The meadow is the game's spring meadow (src/meadow.rs); `grass_season` has no summer value,
    # and "spring" is also what makes the act's air warm and snowless (src/game.rs).
    manifest['terrain'].update(res=int(Hc.shape[0]), spacing=float(EXTENT/(RES-1)),
        texture=f'{DEST_REL}/ground_material.png', material_encoding='gaea-rock-srgb-snow-linear-alpha32-223-v1',
        ground_class=f'{DEST_REL}/ground_class.png', grass_season='spring', color=[0.9, 0.9, 0.9, 1.0])
    manifest['authoring'].update(tool=TOOL, tool_version=VERSION, variant=NAME)
    manifest['overview'] = dict(pos=[cc.r3(crest[0]+520), 300.0, cc.r3(crest[1]+520)],
                                target=[cc.r3(route[k_mouth, 0]), cc.r3(grid.sample(H, *route[k_mouth])), cc.r3(route[k_mouth, 1])])
    edits['editor_home'] = dict(eye=[cc.r3(crest[0]), cc.r3(grid.sample(H, *crest)+EYE_M), cc.r3(crest[1])],
                                target=[cc.r3(route[k_mouth, 0]), cc.r3(grid.sample(H, *route[k_mouth])+2.0), cc.r3(route[k_mouth, 1])])
    # 7 July 1943, early afternoon by the sun (11:00 UT is 13:58 local mean solar time at the
    # game's Act One site; 12:00 on the crew's clock, which the game keeps at UT+1). Hot and hazy:
    # a summer continental aerosol, AOD 0.3 at 550 nm (Moscow's monthly mean reaches about 0.3 in
    # July and August, Chubarova et al. 2016), fair-weather cloud, no mist banks.
    edits['environment_lighting'] = dict(enabled=True, fog_density=0.0012, overcast=0.10, storm=0.0,
        aerosol=dict(aod_550=0.3, angstrom=1.4, scale_height_m=1500),
        schema='kessel.environment-authoring', utc='1943-07-07T11:00:00Z', version=2)
    edits['base_placements'] = len(manifest['placements'])
    edits['base_markers'] = len(manifest['markers'])
    edits['ops'] = []

    places = dict(start=start, hold=route[k_hold], main_crest=crest, forward_crest=forward, wood_mouth=route[k_mouth],
                  glade=route[entries['glade']], far=route[entries['far']], camp=camp_at,
                  east_shoulder=at_u(EAST_BLEND[1]-60, -6.0))
    sheet = mission_sheet(places, fractions)
    checks = validate(G, route, places, entries, entry_positions, loop_a, loop_b, sites, fields, posts, sheet,
                      tank_ground, buildings, k_north, k_far, tree_clear, tree_xz)
    carried = v2.carry(DEST/'manifest_edits.json', None, edits)
    (DEST/'manifest.v1.json').write_text(cc.dumps(manifest, indent=None))
    (DEST/'manifest_edits.json').write_text(cc.dumps(edits))
    np.save(DEST/'heights.npy', Hc.astype(np.float32))
    (DEST/'mission.v1.json').write_text(cc.dumps(sheet))
    xz = lambda q: [cc.r3(q[0]), cc.r3(q[1])]
    report = dict(schema='panzer.kursk-miasoedovo', version=VERSION, seed=SEED, extent_m=2*CROP_HALF,
        places={k_: xz(q) for k_, q in places.items()},
        ground_m={k_: cc.r3(float(G.y(q[0], q[1]))) for k_, q in places.items()},
        wood=dict(south_edge_u_m=U_WOOD_S, north_edge_u_m=U_WOOD_N, x_m=list(WOOD_X), trees=trees,
                  cover_fraction_of_map=cc.r3(float(woods[lo:hi, lo:hi].mean()))),
        route=dict(knots=knots, knot_m=KNOT_M, length_m=course['length_m'], travel_surface=receipt['travel_surface']),
        entries={n_: dict(knot=j, fraction=fractions[n_], at=xz(route[j]), tanks_at=[xz(q) for q in entry_positions(j)])
                 for n_, j in entries.items()},
        opening=dict(platoon_a=dict(tanks=3, loop=[loop_a[0], loop_a[len(loop_a)//2]]),
                     platoon_b=dict(tanks=2, loop=[loop_b[0], loop_b[len(loop_b)//2]])),
        attacks=[dict(id=a, tanks=n_+m_, enters=e if not m_ else f'{e}, then {m_} at the {e2} after 60 s') for a, n_, e, m_, e2 in ATTACKS],
        t34s_fielded_in_all=OPENING_T34S+sum(a[1]+a[3] for a in ATTACKS),
        gun_candidates=len(sites), garrison_posts=len(posts), buildings=len(buildings),
        building_groups={gid: sum(gid in b['id'] for b in buildings) for gid, *_ in GROUP_SPECS},
        placements=len(manifest['placements']), dressing=P.count, checks=checks, carried=carried,
        outputs={n_: cc.sha256_file(DEST/n_) for n_ in cc.OUTPUT_FILES+['mission.v1.json']})
    (DEST/'kursk-miasoedovo.receipt.json').write_text(cc.dumps(report))
    if plan:
        draw_plan(plan, c, route, buildings, places, sites, posts, loop_a, loop_b, woods[lo:hi, lo:hi])
    print(json.dumps({k_: report[k_] for k_ in ('places', 'ground_m', 'wood', 'entries', 'gun_candidates', 'garrison_posts',
                                                 'building_groups', 'placements', 't34s_fielded_in_all', 'checks')}, indent=1))
    failed = [name for name, ok in checks['passed'].items() if not ok]
    if failed:
        raise SystemExit(f'[kursk] checks failed: {failed}')
    return report


def validate(G, route, places, entries, entry_positions, loop_a, loop_b, sites, fields, posts, sheet, tank_ground,
             buildings, k_north, k_far, tree_clear, tree_xz):
    """What can be measured without the game: sight lines over the generated ground (terrain
    alone; with the wood's canopy at 12 m, which is the rendered picture; and by the game's own
    trunk test), footing at every place the mission and the
    deployment name. The game's own probes (collision boxes, the nav grid, the gun siting) are
    not run here."""
    start, hold, crest, forward = places['start'], places['hold'], places['main_crest'], places['forward_crest']
    enemy_starts = [('platoon_a', q) for q in loop_a]+[('platoon_b', q) for q in loop_b] \
        + [(f'entry_{n}', q) for n, j in entries.items() for q in entry_positions(j)] \
        + [('gun', (g['x'], g['z'])) for g in sites]+[('post', (q['x'], q['z'])) for q in posts] \
        + [('building', (b['x'], b['z'])) for b in buildings]
    def worst(viewer, height, canopy=False, kinds=None):
        """The most exposed enemy start seen from `viewer`: the largest sight-line clearance from a
        T-34's sight (2.3 m, canyon_game.rs) to the viewer's point; negative means none sees it."""
        vals = [(G.clearance(q, 2.3, viewer, height, canopy), kind) for kind, q in enemy_starts if kinds is None or kind in kinds]
        return max(vals)
    s_clear, s_kind = worst(start, ROOF_M)
    h_clear, h_kind = worst(hold, ROOF_M)
    tanks = ('platoon_a', 'platoon_b', 'entry_glade', 'entry_far')
    crest_terrain = {kind: cc.r3(max(G.clearance(crest, EYE_M, q, T34_M) for k_, q in enemy_starts if k_ == kind)) for kind in tanks}
    crest_canopy = {kind: cc.r3(max(G.clearance(crest, EYE_M, q, T34_M, True) for k_, q in enemy_starts if k_ == kind)) for kind in tanks}
    fwd_canopy = {kind: cc.r3(max(G.clearance(forward, EYE_M, q, T34_M, True) for k_, q in enemy_starts if k_ == kind)) for kind in tanks}
    # Fields of fire: from the main crest on the track, the track's knots on the far slope that a
    # commander there sees a T-34 on; from the ridge's east shoulder (no forward crest there), the
    # open valley east of the wood and the far slope.
    far_slope = [(float(np.hypot(*(q-crest))), G.sees(crest, EYE_M, q, T34_M, True)) for q in route[k_north:k_far+1]]
    seen_r = [r for r, ok in far_slope if ok]
    shoulder = places['east_shoulder']
    east = [(float(np.hypot(x-shoulder[0], z-shoulder[1])), G.sees(shoulder, EYE_M, (x, z), T34_M, True))
            for x in np.arange(380.0, 1150.0, 40.0) for z in np.arange(float(crest_z(x))-950.0, float(crest_z(x))-440.0, 40.0)]
    east_r = [r for r, ok in east if ok]
    shoulder_canopy = {kind: cc.r3(max(G.clearance(shoulder, EYE_M, q, T34_M, True) for k_, q in enemy_starts if k_ == kind)) for kind in tanks}
    # The wood as the game's sight lines have it: trunks only. Of the sight lines from each viewing
    # place to the T-34s in and behind the wood, how many the game would call clear.
    in_wood = [q for k_, q in enemy_starts if k_ in ('platoon_b', 'entry_glade')]
    trunk_model = {name: f'{sum(G.game_sees(v, EYE_M, q, T34_M, tree_xz) for q in in_wood)} of {len(in_wood)}'
                   for name, v in (('main_crest', crest), ('forward_crest', forward), ('east_shoulder', shoulder))}
    floor_hidden = max(G.clearance(crest, EYE_M, q, T34_M) for q in entry_positions(entries['glade']))
    mouth = places['wood_mouth']
    beats = {}
    for b in sheet['beats']:
        q = (b['at']['x'], b['at']['z'])
        beats[b['id']] = dict(at=[q[0], q[1]], ground_ok=bool(tank_ground(*q)),
                              slope_pct=cc.r3(100*abs(float(G.y(q[0], q[1]+8))-float(G.y(q[0], q[1]-8)))/16))
    look = next(b for b in sheet['beats'] if b['id'] == 'look')
    ranges = [r for f in fields for r in f]
    passed = dict(
        start_unseen_by_every_enemy_start=s_clear < 0,
        hold_unseen_by_every_enemy_start=h_clear < 0,
        valley_floor_is_dead_ground_from_the_main_crest=floor_hidden < 0,
        far_entry_is_dead_ground_from_both_crests_and_the_east_shoulder=crest_terrain['entry_far'] < 0
            and max(G.clearance(v, EYE_M, q, T34_M) for v in (forward, shoulder) for q in entry_positions(entries['far'])) < 0,
        nothing_fielded_at_the_start_is_in_view_from_the_east_shoulder=all(v < 0 for v in shoulder_canopy.values()),
        opening_platoons_unseen_from_the_main_crest=crest_canopy['platoon_a'] < 0 and crest_canopy['platoon_b'] < 0,
        glade_entry_unseen_from_the_forward_crest=fwd_canopy['entry_glade'] < 0,
        far_slope_in_view_from_the_main_crest=len(seen_r) >= 8,
        open_ground_600_to_1200_m_in_view_from_the_east_shoulder=len(east_r) >= 40 and min(east_r) < 650 and max(east_r) > 1200,
        forward_crest_inside_the_look_radius=float(np.hypot(*(forward-mouth))) < look['radius_m']
            and float(np.hypot(*(crest-mouth))) > look['radius_m'],
        every_beat_place_on_tank_ground=all(v['ground_ok'] for v in beats.values()),
        entries_have_room_for_three=all(all(tank_ground(*q) for q in entry_positions(j)) for j in entries.values()),
        entry_knots_legal=all(60 <= j and j+12 < len(route) for j in entries.values()),
        loops_on_tank_ground=all(tank_ground(*q) for q in loop_a+loop_b),
        gun_candidates_with_a_field_of_fire=len(sites) >= 8,
        guns_cannot_reach_the_hold_position=min(math.hypot(g['x']-hold[0], g['z']-hold[1]) for g in sites) > 400.0,
        route_clear_of_trees=all(tree_clear(x, z, 6.5) for x, z in route),
    )
    return dict(
        method='sight lines over the generated heightfield with the terrain\'s own triangles, sampled every 3 m; "canopy" '
               'adds 12 m where the wood stands; eye 2.5 m, Tiger roof 2.9 m, T-34 2.7 m, gun eye 1.1 m',
        start=dict(most_exposed_enemy_start=s_kind, sight_line_clearance_m=cc.r3(s_clear)),
        hold=dict(most_exposed_enemy_start=h_kind, sight_line_clearance_m=cc.r3(h_clear)),
        from_main_crest_clearance_m=dict(terrain_only=crest_terrain, with_canopy=crest_canopy),
        from_forward_crest_clearance_m=dict(with_canopy=fwd_canopy),
        from_east_shoulder_clearance_m=dict(with_canopy=shoulder_canopy),
        game_sight_lines_clear_to_the_tanks_in_the_wood=dict(trunk_model, note='src/game.rs los_clear tests the terrain and '
            'tree trunks, not crowns: what the rendered wood hides from the eye the game\'s perception may still see'),
        far_slope_track_in_view_m=[cc.r3(min(seen_r)), cc.r3(max(seen_r))] if seen_r else [],
        far_slope_track_knots_in_view=f'{len(seen_r)} of {len(far_slope)}',
        east_shoulder_gun_range_m=cc.r3(min(math.hypot(g['x']-shoulder[0], g['z']-shoulder[1]) for g in sites)),
        open_ground_east_in_view_from_the_east_shoulder_m=[cc.r3(min(east_r)), cc.r3(max(east_r))] if east_r else [],
        open_ground_east_points_in_view_from_the_east_shoulder=f'{len(east_r)} of {len(east)}',
        gun_field_ranges_m=[cc.r3(min(ranges)), cc.r3(max(ranges))],
        gun_to_forward_crest_m=cc.r3(min(math.hypot(g['x']-forward[0], g['z']-forward[1]) for g in sites)),
        gun_to_hold_m=cc.r3(min(math.hypot(g['x']-hold[0], g['z']-hold[1]) for g in sites)),
        wood_mouth_from_forward_crest_m=cc.r3(float(np.hypot(*(forward-mouth)))),
        wood_mouth_from_hold_m=cc.r3(float(np.hypot(*(hold-mouth)))),
        beats=beats, passed=passed)


def draw_plan(path, colour, route, buildings, places, sites, posts, loop_a, loop_b, woods):
    prev = np.uint8(np.rint(np.clip(colour[::2, ::2], 0, 1)*255)).copy()
    n = prev.shape[0]
    to_px = (n-1)/(2*CROP_HALF)
    wood = np.asarray(Image.fromarray(np.uint8(woods)*255).resize((n, n), Image.NEAREST)) > 0
    prev[wood] = (prev[wood]*0.55+np.array([20, 60, 25])*0.45).astype(np.uint8)
    def dot(x, z, col, r=1):
        px, pz = int((x+CROP_HALF)*to_px), int((z+CROP_HALF)*to_px)
        prev[max(pz-r, 0):pz+r+1, max(px-r, 0):px+r+1] = col
    for b in buildings:
        x0, x1, z0, z1 = cc.footprint(b['x'], b['z'], b['width_m'], b['depth_m'], b['yaw_deg'])
        for xx in np.arange(x0, x1, 2.0):
            for zz in np.arange(z0, z1, 2.0):
                dot(xx, zz, [200, 90, 40], 0)
    for x, z in route:
        dot(x, z, [255, 255, 255], 0)
    for g in sites:
        dot(g['x'], g['z'], [255, 220, 0], 1)
    for q in posts:
        dot(q['x'], q['z'], [255, 120, 200], 1)
    for q in loop_a+loop_b:
        dot(q[0], q[1], [255, 60, 0], 1)
    for name, q in places.items():
        dot(q[0], q[1], [40, 40, 255] if name in ('start', 'hold', 'camp') else [255, 30, 30], 4)
    Image.fromarray(prev).save(path)


if __name__ == '__main__':
    ap = argparse.ArgumentParser()
    ap.add_argument('--plan', type=Path)
    build(ap.parse_args().plan)
