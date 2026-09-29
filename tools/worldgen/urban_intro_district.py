#!/usr/bin/env python3
"""urban_intro_district.py — the urban-intro district around the Act 1 spawn (role L, 2026-09-28).

Forks the winter-courtyard workshop into ``assets/worldgen/workshops/act1-urban-intro-20260928/`` and
lays out a piece of the Stalingrad city centre as it stood in January 1943: a tram street, side
streets and lanes, blocks and plots, a square, building instances for the grammar, flattened pads,
rubble, craters, poles and wires, fences, yard walls, wrecks and snow. Nothing here tells a story in
words; every parameter carries its provenance (the urban-fabric research R1 and the photo analysis P,
see ``docs/URBAN_INTRO_TEAM_20260928.md``), and the receipt records it per element.

Deterministic: the same inputs give byte-identical outputs. Randomness is the project's counter
hash (``hash01``), keyed by FNV-1a hashes of stable ids; nothing depends on dict order, time or the
machine. The courtyard workshop is read, never written.

Usage:
    python3 tools/worldgen/urban_intro_district.py                  # write the workshop
    python3 tools/worldgen/urban_intro_district.py --plan captures/urban_district_plan.png
    python3 tools/worldgen/urban_intro_district.py --review         # + daylight review variant
    python3 tools/worldgen/urban_intro_district.py --write-sample   # refresh the tracked sample
    python3 tools/worldgen/urban_intro_district.py --texture        # + the opt-in ground-texture fork

Outputs (workshop directory):
    manifest.v1.json      the courtyard manifest, byte for byte (with --texture: terrain.texture points
                          at the forked ground texture)
    manifest_edits.json   the courtyard edits + district dressing ops + ``building_instances``
    heights.npy           the courtyard terrain + pads, aprons, razed-plot mounds, one fallen-facade
                          ridge and craters (every other sample untouched)
    features.json         the courtyard features + ``urban_intro_district_20260928`` (streets,
                          blocks, plots, pads, terrain features, protected zones, encounter review)
    ground_hires.png      only with --texture: the act drape with the district's snow, streets and
                          paths. Act 1's winter terrain shading (shaders/terrain_material.wgsl, mood 0)
                          uses a constant snow albedo and never samples the drape, so the fork shows
                          only in spring shading and offline previews (see LIMITATIONS)
    level_design.v1.json  copied unchanged
    urban-intro.receipt.json  provenance, parameters, measurements, hashes, conflicts and the street
                          encounter (manifest_edits.json "street_encounter": kill zone, candidate
                          45 mm gun sites and platoon beats, which the game measures and applies)
"""
import argparse
import hashlib
import json
import math
import re
import shutil
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
SOURCE_REL = "assets/worldgen/workshops/act1-winter-courtyard-20260925"
DEST_REL = "assets/worldgen/workshops/act1-urban-intro-20260928"
REVIEW_SUFFIX = "-review-daylight"
SAMPLE_REL = "catalog/examples/urban_intro_district.sample.json"
TOOL = "tools/worldgen/urban_intro_district.py"
TOOL_VERSION = 4   # 2: the street encounter block (manifest_edits.json "street_encounter");
                   # 3: the version-2 lighting record (an ephemeris instant) and the cellars' scenes
                   # 4: the density pass (short-side street fronts, yard outbuildings, slabs) and the
                   #    battle traces (crater ejecta, soot, wrecks, positions)
SEED = 0x20260928
EDITS_KEY = "urban_intro_district_20260928"
R1_DOC = "STALINGRAD_URBAN_FABRIC_20260928.md"
P_DOC = "PHOTO_REFERENCE_ANALYSIS_20260928.md"
RESEARCH_DIR = "/home/alex/.claude/jobs/ca78baf9/tmp/research (integrator copies to docs/research/)"

# ── assets: reviewed kit only (runtime catalog, building_kit, the editor's reviewed utility roles,
#    and the authored battlefield/historical sets the courtyard workshop already ships) ────────────
MP2 = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/"
NAT = "assets/pizza/_extracted/PSX Nature v1.7.3/PSX Nature (Tree branches separated)/Models/GLB (recommended)/"
HIST = "assets/authoring/historical_20260925/"
BATTLE = "assets/authoring/battlefield/"
ASSET = {
    # hard (collidable in game: manifest structure whitelist / source-surface wreck proxies)
    "yard_wall": MP2 + "Modular Structures/wall_hr_1.glb",          # brick wall 4 x 3 x 0.4 m
    "pole": MP2 + "Modular Structures/pillar_hr_2.glb",             # editor-reviewed "telegraph pole"
    "wreck_kv1": BATTLE + "kv1_wreck_burnt.glb",
    "wreck_pz3": BATTLE + "pz3_wreck_burnt.glb",
    # hinge / live (topples or falls under a tank)
    "fence_board": MP2 + "Modular Props/fence_a_1.glb",            # vertical boards, 4 x 2.03 m
    "tree_tall": NAT + "tree_8.glb",                                # bare, stripped, 7.9 m
    "tree_small": NAT + "tree_9.glb",                               # bare, stripped, 4.5 m
    # soft visual dressing (crushable / drive-over, no collision)
    "debris": MP2 + "Debris & Misc/debris_bricks_mx_1.glb",
    "debris2": MP2 + "Debris & Misc/debris_bricks_mx_2.glb",
    "gravel": MP2 + "Debris & Misc/gravel_pile_hr_1.glb",
    "gravel2": MP2 + "Debris & Misc/gravel_pile_hr_2.glb",
    "brick_stack": MP2 + "Debris & Misc/bricks_stacked_mx_1.glb",
    "brick": MP2 + "Debris & Misc/brick_mx_1.glb",
    "plank": MP2 + "Props/wooden_plank_1.glb",
    "board": MP2 + "Props/wooden_board_2.glb",
    "crate": MP2 + "Props/wooden_crate_2_a.glb",
    "joist": HIST + "gumrak_charred_joist_2b9e5b42bbbe.glb",       # charred 8 m joist
    "house_joist": HIST + "household_joist_d9bbd850748a.glb",
    "ash_bricks": HIST + "household_ash_bricks_4a1d9acf1bd3.glb",
    "splinter": HIST + "household_splinter_1ea7029d7216.glb",
    "stump": NAT + "tree_stump_1.glb",
    "log": NAT + "tree_log_1.glb",                                  # fallen pole / trunk
    "snow_drift": BATTLE + "snow_drift.glb",
    # battle traces (2026-09-29): a knocked-out or abandoned wreck without burning, the dark earth a
    # shell or bomb threw onto the snow (and soot fallen from burned buildings), a charred board, the
    # reviewed German steel helmet (its mesh sits 1.42 m above its origin, on a head)
    "wreck_kv1_whole": BATTLE + "kv1_wreck_detail.glb",
    "wreck_pz3_whole": BATTLE + "pz3_wreck_detail.glb",
    "scorch": BATTLE + "scorched_earth.glb",                       # 2.2 m across, 0.07 m
    "charred_plank": HIST + "gumrak_charred_plank_a48a6440323a.glb",
    "helmet": "assets/flavor/Characters/german-ww2-soldier-cc0/baked/german_steel_helmet_panzertruppe_relaxed_static.glb",
    # wires (no collision): a plain 4 m span along local X, stretched by scale (wires_hr_1 carries
    # insulator blocks that would stretch with the span, so it is not used)
    "wire": MP2 + "Large Props & Machinery/wires_hr_2.glb",
}
HARD_ASSETS = {"yard_wall", "pole", "wreck_kv1", "wreck_pz3", "wreck_kv1_whole", "wreck_pz3_whole"}
HELMET_MESH_Y = 1.42     # the helmet mesh's lowest point above its origin
WIRE_TOP_OFFSET = 0.017  # wires_hr_2 ends sit 0.017 m below its origin
POLE_BASE_H = 3.0        # pillar_hr_2 is 3.0 m tall at scale 1
WIRE_MAX_DROP_M = 1.2    # a level wire mesh spans poles whose attachments differ by at most this

# ── provenance-tagged parameters. prov codes follow R1: D documented, M measured, P photo
#    cross-check, I inferred (a proposal for the game, not a historical fact). ─────────────────────
PARAMS = {
    "street.main_tram_width_m": dict(v=24.0, prov="M+P+I", src=[
        "R1 §5.2 summary for L: main (tram) 22-30 m", "P L3: main streets 18-22 m +/-25 % [07-01]"]),
    "street.ordinary_width_m": dict(v=18.0, prov="M+P", src=[
        "R1 §5.2: ordinary 17-22 m; facade-to-facade 17.6-29 m (S8, S9)", "P L3 [07-01]"]),
    "street.lane_width_m": dict(v=12.0, prov="I", src=["R1 §5.2: lane or passage 8-14 m"]),
    "street.old_road_width_m": dict(v=14.0, prov="I", src=[
        "level_design road.act1.breakout_spine nominal 11 m; gen_world ROAD_W 7 m half-width",
        "R1 §2: irregular old lanes at an angle to the 1820 grid (S10n, S10o)"]),
    "street.pitch_across_m": dict(v=120.0, prov="M", src=[
        "R1 §5.1: street pitch 93-126 m across (S8); half blocks 94-120 m (S6)"]),
    "square.min_side_m": dict(v=60.0, prov="M+P", src=["R1 §5.2 square 60-250 m (S14)", "P L3 >= 100 m [07-16]"]),
    "block.plot_depth_m": dict(v=[30.0, 50.0], prov="I", src=["R1 §5.3 / JSON block.plot_depth_m"]),
    "tram.gauge_m": dict(v=1.524, prov="D", src=["R1 §5.4 (S15 via Radtsig 1962)"]),
    "tram.track_centres_m": dict(v=3.5, prov="I", src=["R1 §5.4: 3.3-3.8 m, no Stalingrad source"]),
    "tram.pole_pitch_m": dict(v=34.0, prov="I", src=["R1 §5.4: 30-40 m, no Stalingrad source"]),
    "tram.contact_wire_h_m": dict(v=5.8, prov="I", src=["R1 §5.4: 5.5-6.0 m, no Stalingrad source"]),
    "tram.span_wire_h_m": dict(v=6.3, prov="I", src=["R1 §5.4: span wires between side poles in the centre (S15)"]),
    "pole.height_m": dict(v=7.65, prov="P", src=["R1 JSON poles_and_wires 7-9 m", "P L4 [04-14] [08-12]"]),
    "pole.telegraph_pitch_m": dict(v=36.0, prov="I", src=["gen_world POLE_PITCH 36 m", "R1 §5.4 30-40 m"]),
    "fence.board_h_m": dict(v=1.83, prov="P", src=["R1 §5.5 / P L5: board fences 1.6-2.0 m [03-04]"]),
    "fence.low_h_m": dict(v=1.0, prov="P", src=["P L5: picket fences 0.8-1.1 m [03-04] [04-05]"]),
    "yard_wall.h_m": dict(v=2.4, prov="D+I", src=["R1 §5.5: yard walls 2.2-3.0 m (S10o, S10g)"]),
    "rubble.bulking": dict(v=1.5, prov="I", src=["R1 §6.3 / P G17 engineering estimate"]),
    "rubble.razed_share_of_masonry": dict(v=0.15, prov="I", src=[
        "R1 §6.2: a further 5-15 % of pre-war masonry razed (Zabolotny's house, milk house, S13, S11)",
        "P L1: open ground with isolated masses [04-01] [06-12] [04-02]: upper bound taken"]),
    "rubble.ridge_crest_m": dict(v=3.0, prov="P+I", src=["R1 §6.3 / §8.5: ridges 2-4 m close streets (S5 pp. 205-206, 211)"]),
    "crater.diameter_m": dict(v=[5.0, 9.0], prov="I", src=[
        "R1 §6.4: 1-10 m (inferred); only >= 5 m resolvable on the 4.29 m heightfield"]),
    "crater.density_per_km2_all": dict(v=1300.0, prov="D+I", src=[
        "R1 §6.4: more than 150,000 craters city-wide (S2); over the 80-200 km2 battle zone a mean of 750-1,900 "
        "per km2 (inferred), the centre denser: the upper half of the range is taken"]),
    "crater.share_resolved": dict(v=0.10, prov="I", src=[
        "R1 §6.4: most craters are 1-10 m, from 50-250 kg bombs and 76-152 mm shells, a few larger; the size "
        "split is not sourced: 0.10 taken as 5-9 m (bombs), carved in the heightfield; the rest are 1-5 m"]),
    "crater.density_per_km2_resolved": dict(v=130.0, prov="I", src=[
        "crater.density_per_km2_all x crater.share_resolved: the >= 5 m subset the 4.29 m heightfield resolves"]),
    "crater.small_diameter_m": dict(v=[1.2, 4.5], prov="I", src=[
        "R1 §6.4: 1-10 m; below 5 m the bowl is not carved (heightfield 4.29 m): only the dark earth thrown "
        "onto the snow is laid (the scorched-earth patch, about 1.6 crater diameters across)"]),
    "battle.wrecks_added": dict(v=4, prov="D+P", src=[
        "R1 §6.4 (S3 p. 393: on 4 February 1943 the streets 'jammed with the enemy's wrecked vehicles')",
        "P L11: knocked-out tanks in streets and squares, each in a 5-15 m halo [08-12] [08-13] [06-08] [01-25]; "
        "vehicles abandoned along one side of a road, leaving the other passable [05-01]"]),
    "battle.positions": dict(v=28, prov="D+P", src=[
        "R1 §7.1: strongpoints in stone buildings, fire from semi-basements and ruins (S3 pp. 296-298)",
        "P S3: spent cases, straw and boxes at gun and rifle positions [03-03] [03-16] [06-09] [02-13]; "
        "P S6: kit dropped on top of the last snow (boxes, planks, a helmet) [05-01] [05-02] [08-14]"]),
    "snow.drift_h_m": dict(v=[0.25, 0.45], prov="P", src=["P L8 / V11: drifts in the lee [05-13] [05-06]"]),
    "snow.albedo_linear": dict(v=0.86, prov="I", src=[
        "aged snow reflects about 0.8-0.9 of visible light (clean fresh snow up to ~0.97, the act drape's "
        "value); soot lowers it; the centre had burned for five months (R1 §6.1, S3 p. 341)",
        "P L8: trodden to a crust with debris on top [04-01] [04-11]; P §1.4: albedo from physical data, "
        "never from the prints; matches the authored snow_drift material (0.76-0.84)"]),
    "wind.act1_direction": dict(v=[1.0, 0.28], prov="code", src=["src/world.rs weather_wind_profile(1)"]),
    "tiger.min_clear_lane_m": dict(v=6.0, prov="D+I", src=["R1 §8.2: collision box 4.12 m needs >= 4.6-5 m"]),
}


def pv(name):
    return PARAMS[name]["v"]


# typology frontage recipes (R1 §4 inferred block recipes) — shares of frontage length. Plots run
# back to back from the two long sides of a block (R1 §4 arrangement rules); a short side carries the
# corner buildings' flanks, yard walls and gates, and at most one building of its own ("flank").
RECIPES = {
    "core": dict(occupancy=(0.75, 0.90), shares={
        "apartment_block": 0.60, "merchant_brick_house": 0.37, "wooden_house": 0.0, "workshop": 0.03},
        gaps={"party": 0.62, "gate": 0.24, "wall": 0.10, "open": 0.04}),
    "ordinary": dict(occupancy=(0.60, 0.80), shares={
        "apartment_block": 0.25, "merchant_brick_house": 0.55, "wooden_house": 0.12, "workshop": 0.08},
        gaps={"party": 0.40, "gate": 0.28, "wall": 0.16, "open": 0.16}),
    "fringe": dict(occupancy=(0.40, 0.60), shares={
        "apartment_block": 0.0, "merchant_brick_house": 0.22, "wooden_house": 0.74, "workshop": 0.04},
        gaps={"party": 0.0, "gate": 0.10, "wall": 0.0, "open": 0.20, "fence": 0.70}),
}
FLANK_BUILDING_P = {"ordinary": 0.45, "fringe": 0.35}  # chance of one mid building on a short side

# typology size tables. Widths/depths are 4 m kit bays inside DESIGN limits (the intersection of the
# contract's provisional limits() and R1's recommended_grammar_limits_4m_bay). Weights from R1 §3.
SIZES = {
    "merchant_brick_house": dict(width={12: .18, 16: .28, 20: .28, 24: .26}, depth={8: .22, 12: .68, 16: .10},
                                 storeys={1: .45, 2: .55}, cellar=0.60),
    # R1 §3.2: 27-80 m (Proletkultskaya 3 27.2 m, Univermag 48.2 m, Pavlov's House 41.7-56.4 m)
    "apartment_block": dict(width={32: .15, 36: .10, 40: .20, 44: .10, 48: .25, 52: .10, 56: .10},
                            depth={12: .60, 16: .40}, storeys={3: .15, 4: .50, 5: .35}, cellar=0.85),
    "wooden_house": dict(width={8: .60, 12: .40}, depth={8: 1.0}, storeys={1: 1.0}, cellar=0.50),
    "workshop": dict(width={8: .20, 12: .30, 16: .30, 20: .10, 24: .10}, depth={8: .50, 12: .40, 16: .10},
                     storeys={1: 1.0}, cellar=0.0),
}
# R1 §6.2 inferred condition shares (each building one dominant condition)
CONDITION_SHARES = {
    "merchant_brick_house": {"intact": 0.02, "burned": 0.55, "shelled": 0.18, "partly_collapsed": 0.25},
    "apartment_block": {"intact": 0.01, "burned": 0.45, "shelled": 0.24, "partly_collapsed": 0.30},
    "wooden_house": {"intact": 0.02, "burned": 0.90, "shelled": 0.03, "partly_collapsed": 0.05},
    "workshop": {"intact": 0.03, "burned": 0.50, "shelled": 0.17, "partly_collapsed": 0.30},
}
CONDITIONS = ["intact", "burned", "shelled", "partly_collapsed"]
TYPOLOGIES = ["merchant_brick_house", "apartment_block", "wooden_house", "workshop"]
MASONRY = {"merchant_brick_house", "apartment_block", "workshop"}
# R1 §4 main-building shares for the intro district (verification ranges)
SHARE_BY_COUNT = {"merchant_brick_house": (0.35, 0.45), "wooden_house": (0.25, 0.40),
                  "apartment_block": (0.08, 0.12), "workshop": (0.10, 0.20)}
SHARE_BY_AREA = {"merchant_brick_house": (0.35, 0.45), "wooden_house": (0.10, 0.20),
                 "apartment_block": (0.25, 0.35), "workshop": (0.10, 0.15)}

# DESIGN limits: the grammar's limits() ∩ R1 recommended_grammar_limits_4m_bay. Since the grammar
# took its limits from R1 (building_grammar.rs, 2026-09-28) the intersection is R1's recommended set:
# merchant houses up to 3 storeys (the grammar also accepts 4, R1 keeps it rare), wooden houses 8 m
# deep (the grammar accepts 12). Sizes are drawn from SIZE above; these limits only validate. Every
# instance must also pass the limits parsed from src/building_grammar.rs at run time.
DESIGN_LIMITS = {
    "merchant_brick_house": dict(width=(12.0, 40.0), depth=(8.0, 16.0), storeys=(1, 3), bay=4.0, cellar=True),
    "apartment_block": dict(width=(24.0, 72.0), depth=(12.0, 16.0), storeys=(3, 6), bay=4.0, cellar=True),
    "wooden_house": dict(width=(8.0, 12.0), depth=(8.0, 8.0), storeys=(1, 1), bay=4.0, cellar=True),
    "workshop": dict(width=(8.0, 36.0), depth=(8.0, 16.0), storeys=(1, 1), bay=4.0, cellar=False),
}

TYPOLOGY_COLOUR = {"merchant_brick_house": (196, 96, 64), "apartment_block": (112, 128, 196),
                   "wooden_house": (150, 120, 70), "workshop": (120, 160, 120)}


# ════════════════════════════════════════════════════════════════════════════════════════════════
# utilities
# ════════════════════════════════════════════════════════════════════════════════════════════════
def hash01(n: int) -> float:
    """Project counter hash (gen_world.py / worldgen.rs), [0, 1)."""
    x = (n * 2654435761) & 0xFFFFFFFF
    x ^= x >> 15
    x = (x * 2246822519) & 0xFFFFFFFF
    x ^= x >> 13
    return ((x >> 8) & 0xFFFF) / 65536.0


def fnv1a32(text: str) -> int:
    h = 0x811C9DC5
    for byte in text.encode("utf-8"):
        h ^= byte
        h = (h * 0x01000193) & 0xFFFFFFFF
    return h


def rnd(key, salt: int) -> float:
    """Deterministic [0,1) keyed by a stable string or int and an integer salt."""
    k = fnv1a32(key) if isinstance(key, str) else int(key)
    return hash01((SEED ^ (k * 0x9E3779B9) ^ (salt * 0x85EBCA77)) & 0xFFFFFFFF)


def pick(weights: dict, u: float):
    """Pick a key from {key: weight} with u in [0,1); keys iterate in sorted order."""
    keys = sorted(weights)
    total = sum(weights[k] for k in keys)
    acc = 0.0
    for k in keys:
        acc += weights[k] / total
        if u < acc:
            return k
    return keys[-1]


def smoothstep(t):
    t = np.clip(t, 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def r3(v):
    return round(float(v), 3)


def rect_overlap(a, b, tol=0.01):
    """Axis-aligned rects (x0, x1, z0, z1) overlap by more than tol (touching is allowed)."""
    return a[0] < b[1] - tol and b[0] < a[1] - tol and a[2] < b[3] - tol and b[2] < a[3] - tol


def rect_dist(r, x, z):
    """Euclidean distance from a point to a rect (0 inside)."""
    dx = max(r[0] - x, 0.0, x - r[1])
    dz = max(r[2] - z, 0.0, z - r[3])
    return math.hypot(dx, dz)


def rect_gap(a, b):
    """Euclidean gap between two rects (0 if touching or overlapping)."""
    dx = max(a[0] - b[1], b[0] - a[1], 0.0)
    dz = max(a[2] - b[3], b[2] - a[3], 0.0)
    return math.hypot(dx, dz)


def rect_gap_inf(a, b):
    """Chebyshev gap between two rects: two footprints whose one-cell skirts can share a terrain
    sample are closer than two cells in this metric, whatever their diagonal distance."""
    dx = max(a[0] - b[1], b[0] - a[1], 0.0)
    dz = max(a[2] - b[3], b[2] - a[3], 0.0)
    return max(dx, dz)


def seg_dist(px, pz, a, b):
    ax, az = a
    bx, bz = b
    dx, dz = bx - ax, bz - az
    L2 = dx * dx + dz * dz
    t = 0.0 if L2 < 1e-9 else max(0.0, min(1.0, ((px - ax) * dx + (pz - az) * dz) / L2))
    return math.hypot(px - (ax + t * dx), pz - (az + t * dz))


def rect_seg_dist(r, a, b, samples=48):
    """Approximate min distance between a rect and a segment (dense sampling + corners)."""
    best = min(seg_dist(x, z, a, b) for x in (r[0], r[1]) for z in (r[2], r[3]))
    for i in range(samples + 1):
        t = i / samples
        best = min(best, rect_dist(r, a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
    return best


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rel(path: Path) -> str:
    return str(path.resolve().relative_to(ROOT))


# ════════════════════════════════════════════════════════════════════════════════════════════════
# source reading
# ════════════════════════════════════════════════════════════════════════════════════════════════
VEC2 = r"Vec2::new\(\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*\)"


def read_rust_constants():
    """Mission points, encounter routes, anti-tank-gun sites and Act 1 wind, read from the live
    Rust source so the conflict review can never drift from the game."""
    out = {}
    mission = (ROOT / "src/winter_mission.rs").read_text()
    for name in ("HOUSE", "WRECK", "WITHDRAW"):
        m = re.search(r"const %s: Vec2 = %s;" % (name, VEC2), mission)
        if not m:
            raise SystemExit(f"cannot read {name} from src/winter_mission.rs")
        out[name.lower()] = (float(m.group(1)), float(m.group(2)))
    enc = (ROOT / "src/winter_encounter_ai.rs").read_text()
    m = re.search(r"const ROUTES: \[\[Vec2; 4\]; 2\] = \[(.*?)\];", enc, re.S)
    if not m:
        raise SystemExit("cannot read ROUTES from src/winter_encounter_ai.rs")
    pts = [(float(a), float(b)) for a, b in re.findall(VEC2, m.group(1))]
    if len(pts) != 8:
        raise SystemExit("ROUTES must hold 2 x 4 points")
    out["routes"] = [pts[:4], pts[4:]]
    m = re.search(r"pub\(super\) const SPACING_M: f32 = ([\d.]+);", enc)
    out["platoon_spacing_m"] = float(m.group(1)) if m else 40.0
    m = re.search(r"pub\(super\) const PLATOON_TANKS: \[usize; 2\] = \[(\d+), (\d+)\];", enc)
    out["platoon_tanks"] = [int(m.group(1)), int(m.group(2))] if m else [3, 2]
    live = (ROOT / "src/live_playtest.rs").read_text()
    m = re.search(r"let route_mid = %s;" % VEC2, live)
    out["route_mid"] = (float(m.group(1)), float(m.group(2)))
    m = re.search(r"let sites = \[%s, %s\]" % (VEC2, VEC2), live)
    out["at_gun_sites"] = [(float(m.group(1)), float(m.group(2))), (float(m.group(3)), float(m.group(4)))]
    world = (ROOT / "src/world.rs").read_text()
    m = re.search(r"1 => \(Vec2::new\((-?[\d.]+), (-?[\d.]+)\)", world)
    wx, wz = float(m.group(1)), float(m.group(2))
    n = math.hypot(wx, wz)
    out["wind"] = (wx / n, wz / n)
    return out


def read_rust_limits():
    """Parse limits() from src/building_grammar.rs: {typology: dict(width, depth, storeys, bay, cellar)}."""
    text = (ROOT / "src/building_grammar.rs").read_text()
    names = {"MerchantBrickHouse": "merchant_brick_house", "ApartmentBlock": "apartment_block",
             "WoodenHouse": "wooden_house", "Workshop": "workshop"}
    body = text[text.index("pub fn limits("):]
    out = {}
    num = r"(-?[\d.]+)"
    for rust, key in names.items():
        m = re.search(r"Typology::%s => TypologyLimits \{(.*?)\}," % rust, body, re.S)
        if not m:
            raise SystemExit(f"cannot parse limits() for {rust} in src/building_grammar.rs")
        blk = m.group(1)

        def pair(field):
            mm = re.search(r"%s: \(\s*%s\s*,\s*%s\s*\)" % (field, num, num), blk)
            return (float(mm.group(1)), float(mm.group(2)))

        bay = float(re.search(r"bay_m: %s" % num, blk).group(1))
        cellar = re.search(r"cellar_allowed: (true|false)", blk).group(1) == "true"
        s = pair("storeys")
        out[key] = dict(width=pair("width_m"), depth=pair("depth_m"), storeys=(int(s[0]), int(s[1])),
                        bay=bay, cellar=cellar)
    return out


def within_limits(spec, lim):
    """Mirror of building_grammar::check_spec for one spec dict."""
    errs = []
    for label, side, (lo, hi) in (("width", spec["width_m"], lim["width"]), ("depth", spec["depth_m"], lim["depth"])):
        if not (lo - 1e-3 <= side <= hi + 1e-3):
            errs.append(f"{label} {side} outside {lo}..{hi}")
        bays = side / lim["bay"]
        if abs(bays - round(bays)) >= 1e-3 or round(bays) < 1:
            errs.append(f"{label} {side} not whole {lim['bay']} m bays")
    if not (lim["storeys"][0] <= spec["storeys"] <= lim["storeys"][1]):
        errs.append(f"storeys {spec['storeys']} outside {lim['storeys']}")
    if spec["cellar"] and not lim["cellar"]:
        errs.append("cellar not allowed")
    return errs


class Source:
    def __init__(self, root: Path):
        self.dir = root / SOURCE_REL
        self.manifest = json.loads((self.dir / "manifest.v1.json").read_text())
        self.edits = json.loads((self.dir / "manifest_edits.json").read_text())
        self.features = json.loads((self.dir / "features.json").read_text())
        self.receipt = json.loads((self.dir / "winter-courtyard.receipt.json").read_text())
        self.level_design = json.loads((self.dir / "level_design.v1.json").read_text())
        self.heights = np.load(self.dir / "heights.npy")
        t = self.manifest["terrain"]
        self.res = int(t["res"])
        self.spacing = float(t["spacing"])
        spawn = [m for m in self.manifest["markers"] if m["kind"] == "Spawn"]
        self.spawn = (float(spawn[0]["x"]), float(spawn[0]["z"]))
        pad = self.receipt["pad_bounds_xz"]
        self.courtyard_pad = (float(pad[0]), float(pad[2]), float(pad[1]), float(pad[3]))  # x0,x1,z0,z1
        self.courtyard_anchor = self.receipt["anchor"]
        self.courtyard_apron = float(self.receipt["apron_metres"])

    def objects(self):
        """Every pre-existing placement and add op as (x, y, z, stem, source)."""
        assets = self.manifest["assets"]
        out = []
        for i, p in enumerate(self.manifest["placements"]):
            out.append((float(p[1]), float(p[2]), float(p[3]), Path(assets[p[0]]).stem, f"base:{i}"))
        for i, op in enumerate(self.edits.get("ops", [])):
            if op.get("op") == "add":
                out.append((float(op["x"]), float(op["y"]), float(op["z"]), Path(op["asset"]).stem, f"op:{i}"))
        return out


# ════════════════════════════════════════════════════════════════════════════════════════════════
# terrain grid
# ════════════════════════════════════════════════════════════════════════════════════════════════
class Grid:
    """World <-> sample mapping of the act heightfield (row = z, col = x, origin-centred)."""

    def __init__(self, res, spacing):
        self.res = res
        self.sp = spacing
        self.half = (res - 1) * 0.5
        self.axis = (np.arange(res, dtype=np.float64) - self.half) * spacing
        self.world_half = self.half * spacing

    def col(self, x):
        return x / self.sp + self.half

    def x_of(self, c):
        return (c - self.half) * self.sp

    def window(self, x0, x1, z0, z1):
        """Index slices covering a world rect (inclusive of boundary samples)."""
        c0 = max(0, int(math.floor(self.col(x0))))
        c1 = min(self.res - 1, int(math.ceil(self.col(x1))))
        r0 = max(0, int(math.floor(self.col(z0))))
        r1 = min(self.res - 1, int(math.ceil(self.col(z1))))
        return slice(r0, r1 + 1), slice(c0, c1 + 1)

    def sample(self, H, x, z):
        """Bilinear sample (same as gen_world.ground / terrain.rs height_at)."""
        gx = min(max(self.col(x), 0.0), self.res - 1.0)
        gz = min(max(self.col(z), 0.0), self.res - 1.0)
        x0, z0 = int(gx), int(gz)
        x1, z1 = min(x0 + 1, self.res - 1), min(z0 + 1, self.res - 1)
        tx, tz = gx - x0, gz - z0
        a = H[z0, x0] * (1 - tx) + H[z0, x1] * tx
        b = H[z1, x0] * (1 - tx) + H[z1, x1] * tx
        return float(a * (1 - tz) + b * tz)


def gaussian(H, sigma_cells):
    """Separable Gaussian blur with edge padding (deterministic numpy)."""
    r = int(3 * sigma_cells) + 1
    k = np.exp(-0.5 * (np.arange(-r, r + 1) / sigma_cells) ** 2)
    k /= k.sum()
    P = np.pad(H, r, mode="edge")
    tmp = np.zeros_like(P)
    for i, w in enumerate(k):
        tmp[:, r:-r] += w * P[:, i:i + P.shape[1] - 2 * r]
    out = np.zeros_like(H)
    for i, w in enumerate(k):
        out += w * tmp[i:i + H.shape[0], r:-r]
    return out


# ════════════════════════════════════════════════════════════════════════════════════════════════
# extent: chosen from the terrain around the spawn and the courtyard
# ════════════════════════════════════════════════════════════════════════════════════════════════
EXTENT_SEARCH = dict(x0=(1450, 1650), x1=(2000, 2150), z0=(-140, -60), z1=(440, 540), step=5)
EXTENT_MAX_SIDE_M = 600.0      # brief: "about 500 x 500 m"; STATUS: "a dense 500x500 m warzone block"
EXTENT_SOFT_MAX = 0.03         # at most 3 % of the samples steep (> 9 deg smoothed) or in the balka
ANCHOR_MARGIN_M = 60.0
ROUTE_CLEARANCE_M = 60.0       # keep the district edge 60 m short of the T-34 patrol beats
RAIL_CORRIDOR_HALF_M = 25.0    # the railway on the east_rail_embankment crest bounds the centre (R1 §2)
EDGE_MARGIN_M = 55.0           # act boundary: T-34 clear-point test keeps ~12 m, plus the apron


def terrain_masks(src, grid, H):
    """Masks over a window around the spawn: hard (rail corridor, act edge) and soft (steep, balka)."""
    rs, cs = grid.window(1300.0, grid.world_half, -400.0, 800.0)
    Hw = H[rs, cs].astype(np.float64)
    xs = grid.axis[cs][None, :]
    zs = grid.axis[rs][:, None]
    Hs = gaussian(Hw, 12.0 / grid.sp)
    gz, gx = np.gradient(Hs, grid.sp)
    slope = np.degrees(np.arctan(np.hypot(gx, gz)))
    ridge = [t for t in src.features.get("tactical", []) if t.get("name") == "east_rail_embankment"][0]
    rail_x = ridge["center"][0] + RAIL_CORRIDOR_HALF_M
    hard = (xs < rail_x) | (xs > grid.world_half - EDGE_MARGIN_M) | (np.abs(zs) > grid.world_half - EDGE_MARGIN_M)
    hard = np.broadcast_to(hard, Hw.shape)
    anchor_h = float(src.courtyard_anchor[1])
    soft = (slope > 9.0) | (Hw < anchor_h - 4.0)
    return dict(rs=rs, cs=cs, xs=xs, zs=zs, Hs=Hs, slope=slope, hard=hard, soft=soft, rail_x=rail_x,
                anchor_h=anchor_h, ridge=ridge)


def choose_extent(src, grid, H, rust):
    masks = terrain_masks(src, grid, H)
    xs = masks["xs"][0]
    zs = masks["zs"][:, 0]
    hard = masks["hard"].astype(np.int64)
    soft = masks["soft"].astype(np.int64)
    Sh = np.pad(hard.cumsum(0).cumsum(1), ((1, 0), (1, 0)))
    Ss = np.pad(soft.cumsum(0).cumsum(1), ((1, 0), (1, 0)))

    def count(S, r0, r1, c0, c1):
        return int(S[r1, c1] - S[r0, c1] - S[r1, c0] + S[r0, c0])

    def span(axis, lo, hi):
        i0 = int(np.searchsorted(axis, lo, side="left"))
        i1 = int(np.searchsorted(axis, hi, side="right"))
        return i0, i1

    anchors = [src.spawn, rust["house"], rust["wreck"], rust["withdraw"],
               (src.courtyard_pad[0], src.courtyard_pad[2]), (src.courtyard_pad[1], src.courtyard_pad[3])]
    route_z_max = max(p[1] for route in rust["routes"] for p in route)
    z0_min = route_z_max + ROUTE_CLEARANCE_M
    s = EXTENT_SEARCH
    best = None
    evaluated = 0
    for x0 in range(s["x0"][0], s["x0"][1] + 1, s["step"]):
        c0, _ = span(xs, x0, x0)
        for x1 in range(s["x1"][0], s["x1"][1] + 1, s["step"]):
            if x1 - x0 > EXTENT_MAX_SIDE_M:
                continue
            _, c1 = span(xs, x1, x1)
            for z0 in range(s["z0"][0], s["z0"][1] + 1, s["step"]):
                if z0 < z0_min:
                    continue
                r0, _ = span(zs, z0, z0)
                for z1 in range(s["z1"][0], s["z1"][1] + 1, s["step"]):
                    if z1 - z0 > EXTENT_MAX_SIDE_M:
                        continue
                    evaluated += 1
                    if any(not (x0 + ANCHOR_MARGIN_M <= ax <= x1 - ANCHOR_MARGIN_M
                                and z0 + ANCHOR_MARGIN_M <= az <= z1 - ANCHOR_MARGIN_M) for ax, az in anchors):
                        continue
                    _, r1 = span(zs, z1, z1)
                    if count(Sh, r0, r1, c0, c1) > 0:
                        continue
                    n = (r1 - r0) * (c1 - c0)
                    soft_frac = count(Ss, r0, r1, c0, c1) / max(n, 1)
                    if soft_frac > EXTENT_SOFT_MAX:
                        continue
                    area = (x1 - x0) * (z1 - z0)
                    key = (area, -round(soft_frac, 6), -abs(z0 - (-100)), -x0, -z0)
                    if best is None or key > best[0]:
                        best = (key, (x0, x1, z0, z1), soft_frac)
    if best is None:
        raise SystemExit("no district extent satisfies the terrain constraints")
    x0, x1, z0, z1 = best[1]
    return dict(x0=float(x0), x1=float(x1), z0=float(z0), z1=float(z1), soft_fraction=round(best[2], 4),
                candidates_evaluated=evaluated, rail_corridor_east_edge_x=round(masks["rail_x"], 2),
                route_clearance_north_z=z0_min,
                rule=("maximise area within the brief's ~500-600 m square: no hard samples (the rail "
                      "corridor on the east_rail_embankment crest, the act edge), at most 3 % steep "
                      "(> 9 deg on the 12 m-smoothed terrain) or balka samples (> 4 m below the "
                      "courtyard pad), anchors (spawn, courtyard pad, mission points) >= 60 m inside, "
                      "north edge >= 60 m short of the T-34 patrol beats")), masks


# ════════════════════════════════════════════════════════════════════════════════════════════════
# streets, square and blocks
# ════════════════════════════════════════════════════════════════════════════════════════════════
def street_rect(s):
    h = s["w"] / 2
    if s["axis"] == "ns":
        return (s["c"] - h, s["c"] + h, s["lo"], s["hi"])
    return (s["lo"], s["hi"], s["c"] - h, s["c"] + h)


def plan_streets(ext, src, rust):
    """Street network from R1's pattern: one N-S tram street through the spawn, side streets at the
    R1 across pitch, lanes at the fringe, E-W streets framing a square between the courtyard and the
    withdraw point, and the old diagonal road (the Act 1 breakout spine) kept as a lane."""
    streets = []
    xm = float(round(src.spawn[0]))
    wm, wo, wl = pv("street.main_tram_width_m"), pv("street.ordinary_width_m"), pv("street.lane_width_m")
    pitch = pv("street.pitch_across_m")
    streets.append(dict(id="st.main", cls="main", axis="ns", c=xm, w=wm, lo=ext["z0"], hi=ext["z1"]))
    for k in (1, 2, 3):
        for sign, tag in ((-1, "w"), (1, "e")):
            c = xm + sign * k * pitch
            if not (ext["x0"] + 30.0 <= c <= ext["x1"] - 30.0):
                continue
            outer = not (ext["x0"] + 30.0 <= xm + sign * (k + 1) * pitch <= ext["x1"] - 30.0)
            cls, w = ("lane", wl) if outer else ("ordinary", wo)
            streets.append(dict(id=f"st.{tag}{k}", cls=cls, axis="ns", c=c, w=w, lo=ext["z0"], hi=ext["z1"]))
    pad = src.courtyard_pad
    zc = float(math.ceil(pad[3] + 4.0 + wo / 2))                       # square's north street
    wz = rust["withdraw"][1]
    zd = float(math.ceil(wz + 8.0 + wo / 2))                           # square's south street
    za = ext["z0"] + wl / 2
    ze = ext["z1"] - wl / 2
    streets.append(dict(id="st.a", cls="lane", axis="ew", c=za, w=wl, lo=ext["x0"], hi=ext["x1"]))
    streets.append(dict(id="st.c", cls="ordinary", axis="ew", c=zc, w=wo, lo=ext["x0"], hi=ext["x1"]))
    streets.append(dict(id="st.d", cls="ordinary", axis="ew", c=zd, w=wo, lo=ext["x0"], hi=ext["x1"]))
    streets.append(dict(id="st.e", cls="lane", axis="ew", c=ze, w=wl, lo=ext["x0"], hi=ext["x1"]))
    north_gap = (zc - wo / 2) - (za + wl / 2)
    if north_gap > 180.0:
        zb = float(round(((za + wl / 2) + (zc - wo / 2)) / 2))
        streets.append(dict(id="st.b", cls="ordinary", axis="ew", c=zb, w=wo, lo=ext["x0"], hi=ext["x1"]))
    south_gap = (ze - wl / 2) - (zd + wo / 2)
    if south_gap > 180.0:
        zf = float(round(((zd + wo / 2) + (ze - wl / 2)) / 2))
        streets.append(dict(id="st.f", cls="ordinary", axis="ew", c=zf, w=wo, lo=ext["x0"], hi=ext["x1"]))
    for s in streets:
        s["rect"] = street_rect(s)
    streets.sort(key=lambda s: s["id"])
    # the old road: the graded Act 1 breakout spine leaves the spawn toward the grain elevator
    road = [r for r in src.features["roads"] if abs(r[0] - src.spawn[0]) < 1 and abs(r[1] - src.spawn[1]) < 1][0]
    a = (float(road[0]), float(road[1]))
    d = (road[2] - road[0], road[3] - road[1])
    L = math.hypot(*d)
    u = (d[0] / L, d[1] / L)
    exits = []
    if u[0] < 0:
        exits.append((ext["x0"] - a[0]) / u[0])
    if u[0] > 0:
        exits.append((ext["x1"] - a[0]) / u[0])
    if u[1] > 0:
        exits.append((ext["z1"] - a[1]) / u[1])
    if u[1] < 0:
        exits.append((ext["z0"] - a[1]) / u[1])
    t_exit = min(t for t in exits if t > 0)
    b = (a[0] + u[0] * t_exit, a[1] + u[1] * t_exit)
    diag = dict(id="st.old_road", cls="old_road", axis="diag", a=a, b=b, w=pv("street.old_road_width_m"),
                clear_half=pv("street.old_road_width_m") / 2 + 3.0, dir=u, full_end=(road[2], road[3]))
    return streets, diag


def plan_blocks(ext, streets):
    ns = sorted([s for s in streets if s["axis"] == "ns"], key=lambda s: s["c"])
    ew = sorted([s for s in streets if s["axis"] == "ew"], key=lambda s: s["c"])
    xs = []  # column spans: (x0, x1, west street, east street)
    for i in range(len(ns) + 1):
        lo = ext["x0"] if i == 0 else ns[i - 1]["c"] + ns[i - 1]["w"] / 2
        hi = ext["x1"] if i == len(ns) else ns[i]["c"] - ns[i]["w"] / 2
        xs.append((lo, hi, ns[i - 1] if i > 0 else None, ns[i] if i < len(ns) else None))
    zs = []
    for j in range(len(ew) - 1):
        lo = ew[j]["c"] + ew[j]["w"] / 2
        hi = ew[j + 1]["c"] - ew[j + 1]["w"] / 2
        zs.append((lo, hi, ew[j], ew[j + 1]))
    main = [s for s in ns if s["cls"] == "main"][0]
    blocks, square = [], None
    for i, (bx0, bx1, west, east) in enumerate(xs):
        for j, (bz0, bz1, north, south) in enumerate(zs):
            bid = f"c{i}r{j}"
            rect = (bx0, bx1, bz0, bz1)
            if east is main and north["id"] == "st.c" and south["id"] == "st.d":
                square = dict(id="square", rect=rect, block_id=bid)
                continue
            blocks.append(dict(id=bid, col=i, row=j, rect=rect,
                               sides={"n": north, "s": south, "w": west, "e": east},
                               edge_col=(i == 0 or i == len(xs) - 1)))
    for b in blocks:
        b["side_cls"] = {side: side_class(b, side, st, square) for side, st in b["sides"].items()}
    return blocks, square


def side_class(block, side, street, square):
    if street is None:
        return None
    if block["edge_col"]:
        return "fringe"
    if street["cls"] == "main":
        return "core"
    if square is not None:
        sq = square["rect"]
        bx0, bx1, bz0, bz1 = block["rect"]
        facing_square = (
            (side == "s" and abs(bz1 + street["w"] - sq[2]) < 0.5 and bx0 < sq[1] and sq[0] < bx1)
            or (side == "n" and abs(bz0 - street["w"] - sq[3]) < 0.5 and bx0 < sq[1] and sq[0] < bx1)
            or (side == "e" and abs(bx1 + street["w"] - sq[0]) < 0.5 and bz0 < sq[3] and sq[2] < bz1)
            or (side == "w" and abs(bx0 - street["w"] - sq[1]) < 0.5 and bz0 < sq[3] and sq[2] < bz1))
        if facing_square:
            return "core"
    if street["cls"] == "lane" and street["axis"] == "ew":
        return "fringe"   # the district's north and south boundary lanes
    return "ordinary"


def long_sides(block):
    x0, x1, z0, z1 = block["rect"]
    return ("w", "e") if (z1 - z0) >= (x1 - x0) else ("n", "s")


CLASS_PRIORITY = {"core": 3, "ordinary": 2, "fringe": 1}
SIDES = ("n", "s", "w", "e")


def side_frame(rect, side):
    """(along_lo, along_hi, footprint(p, w, d, setback), yaw) for a block side."""
    x0, x1, z0, z1 = rect
    if side == "n":
        return x0, x1, (lambda p, w, d, s: (p, p + w, z0 + s, z0 + s + d)), 180.0
    if side == "s":
        return x0, x1, (lambda p, w, d, s: (p, p + w, z1 - s - d, z1 - s)), 0.0
    if side == "w":
        return z0, z1, (lambda p, w, d, s: (x0 + s, x0 + s + d, p, p + w)), 270.0
    return z0, z1, (lambda p, w, d, s: (x1 - s - d, x1 - s, p, p + w)), 90.0


# ════════════════════════════════════════════════════════════════════════════════════════════════
# plots and buildings
# ════════════════════════════════════════════════════════════════════════════════════════════════
PAD_STEP_M = 0.15         # near neighbours' pads differ by at most this (shared samples averaged)
PAD_TOLERANCE_M = 0.10    # every point of a standing footprint lies within this of its pad
PAD_BLEND_SLOPE_DEG = 15.0  # pads further apart may differ by what the ground between their skirts
                            # can take at this grade, so aprons never build a cliff in a street
PAD_MAX_CUT_FILL_M = 1.2  # a pad may sit at most this far from the mean ground under it
PAD_OFF_STREET_M = 0.6    # ... and at most this far from the street level at its entrance
APRON_M = 8.0             # smooth blend from a pad's skirt back to the ground


STREET_GRADE_SIGMA_M = 24.0   # the street surface: the ground with its crater and micro-relief smoothed
STREET_SIDE_FEATHER_M = 3.0
STREET_EDGE_FEATHER_M = 20.0  # streets fade back to the open ground at the district edge


class Placer:
    """Holds every placed footprint and the checks a new footprint must pass."""

    def __init__(self, grid, H, masks, protected_discs, protected_rects, diag, streets, square, G):
        self.grid = grid
        self.H = H.astype(np.float64)
        self.G = G
        self.masks = masks
        self.discs = protected_discs
        self.prects = protected_rects
        self.diag = diag
        self.street_rects = [s["rect"] for s in streets]
        self.square = square
        self.buildings = []
        self.cell = grid.sp + 1e-6
        rs, cs = masks["rs"], masks["cs"]
        self.win_r0, self.win_c0 = rs.start, cs.start

    def _win(self, rect, pad=0.0):
        r0 = int(math.floor(self.grid.col(rect[2] - pad))) - self.win_r0
        r1 = int(math.ceil(self.grid.col(rect[3] + pad))) - self.win_r0
        c0 = int(math.floor(self.grid.col(rect[0] - pad))) - self.win_c0
        c1 = int(math.ceil(self.grid.col(rect[1] + pad))) - self.win_c0
        return slice(max(r0, 0), r1 + 1), slice(max(c0, 0), c1 + 1)

    def ideal_pad(self, rect, frontage=None):
        """A street building takes its level from the street at its entrance (the graded street
        surface at the facade midpoint); a yard building or slab from the grade at its centre."""
        if frontage is not None:
            return self.grid.sample(self.G, *frontage)
        return self.grid.sample(self.G, (rect[0] + rect[1]) / 2, (rect[2] + rect[3]) / 2)

    def ground_mean(self, rect):
        rs, cs = self._win(rect)
        return float(self.H[rs.start + self.win_r0:rs.stop + self.win_r0,
                            cs.start + self.win_c0:cs.stop + self.win_c0].mean())

    def terrain_ok(self, rect):
        rs, cs = self._win(rect, self.cell + APRON_M * 0.5)
        if self.masks["hard"][rs, cs].any():
            return False, "hard"
        rs2, cs2 = self._win(rect, self.cell)
        if self.masks["soft"][rs2, cs2].any():
            return False, "steep"
        sub = self.H[rs2.start + self.win_r0:rs2.stop + self.win_r0, cs2.start + self.win_c0:cs2.stop + self.win_c0]
        if float(sub.max() - sub.min()) > 4.0:
            return False, "relief"
        return True, ""

    def site_reason(self, rect):
        """Why a footprint could never stand here (protected zones, streets, terrain), ignoring the
        buildings already placed; '' if it could."""
        for s in self.street_rects:
            if rect_overlap(rect, s, 0.01):
                return "street"
        if self.square is not None and rect_overlap(rect, self.square["rect"], 0.01):
            return "square"
        if rect_seg_dist(rect, self.diag["a"], self.diag["b"]) < self.diag["clear_half"]:
            return "old_road"
        for (x, z, r, why) in self.discs:
            if rect_dist(rect, x, z) < r:
                return why
        for (pr, why) in self.prects:
            if rect_overlap(rect, pr, 0.0):
                return why
        ok, why = self.terrain_ok(rect)
        return "" if ok else why

    def check(self, rect):
        """Return (ok, reason, info): info is the overlapped building or the list of near neighbours."""
        for s in self.street_rects:
            if rect_overlap(rect, s, 0.01):
                return False, "street", None
        if self.square is not None and rect_overlap(rect, self.square["rect"], 0.01):
            return False, "square", None
        if rect_seg_dist(rect, self.diag["a"], self.diag["b"]) < self.diag["clear_half"]:
            return False, "old_road", None
        for (x, z, r, why) in self.discs:
            if rect_dist(rect, x, z) < r:
                return False, why, None
        for (pr, why) in self.prects:
            if rect_overlap(rect, pr, 0.0):
                return False, why, None
        neighbours = []
        for b in self.buildings:
            if rect_overlap(rect, b["rect"], 0.01):
                return False, "overlap", b
            if rect_gap_inf(rect, b["rect"]) < 2 * self.cell + APRON_M:
                neighbours.append(b)
        ok, why = self.terrain_ok(rect)
        if not ok:
            return False, why, None
        return True, "", neighbours

    def pad_for(self, rect, neighbours, frontage=None, step=PAD_STEP_M):
        ideal = self.ideal_pad(rect, frontage)
        if abs(ideal - self.ground_mean(rect)) > PAD_MAX_CUT_FILL_M:
            return None, ideal
        lo, hi = -1e9, 1e9
        grade = math.tan(math.radians(PAD_BLEND_SLOPE_DEG))
        for n in neighbours:
            free = max(rect_gap_inf(rect, n["rect"]) - 2 * self.cell, 0.0)
            allow = step + grade * free
            lo = max(lo, n["pad"] - allow)
            hi = min(hi, n["pad"] + allow)
        if lo > hi:
            return None, ideal
        pad = min(max(ideal, lo), hi)
        if abs(pad - ideal) > PAD_OFF_STREET_M or abs(pad - self.ground_mean(rect)) > PAD_MAX_CUT_FILL_M:
            return None, ideal
        return pad, ideal


def frontage_point(rect, side):
    """Midpoint of a footprint's street facade, on the street line."""
    x0, x1, z0, z1 = rect
    return {"n": ((x0 + x1) / 2, z0), "s": ((x0 + x1) / 2, z1), "w": (x0, (z0 + z1) / 2),
            "e": (x1, (z0 + z1) / 2)}[side]


def choose_typology(cls, u):
    rec = RECIPES[cls]["shares"]
    mean_w = {t: sum(w * p for w, p in SIZES[t]["width"].items()) / sum(SIZES[t]["width"].values())
              for t in TYPOLOGIES}
    probs = {t: rec[t] / mean_w[t] for t in TYPOLOGIES if rec[t] > 0}
    return pick(probs, u)


def choose_size(typ, key, remaining):
    tab = SIZES[typ]
    widths = {w: p for w, p in tab["width"].items() if w <= remaining + 1e-6}
    if not widths:
        return None
    w = pick(widths, rnd(key, 11))
    d = pick(tab["depth"], rnd(key, 12))
    storeys = pick(tab["storeys"], rnd(key, 13))
    cellar = rnd(key, 14) < tab["cellar"]
    return float(w), float(d), int(storeys), bool(cellar)


def gap_length(kind, key):
    if kind == "party":
        return 0.0
    if kind == "gate":
        return 4.0
    if kind == "wall":
        return 8.0
    if kind == "fence":
        return 4.0 + 4.0 * math.floor(rnd(key, 21) * 2)          # 4 or 8 m of fence
    return 12.0 + 4.0 * math.floor(rnd(key, 22) * 3)              # open: 12-20 m


def fill_side(placer, block, side, cls, gaps_out):
    lo, hi, fp, yaw = side_frame(block["rect"], side)
    p = lo
    idx = 0
    tries = 0
    while p + 8.0 <= hi and tries < 400:
        tries += 1
        key = f"{block['id']}.{side}.{idx}.{int(p)}"
        typ = choose_typology(cls, rnd(key, 1))
        placed = None
        fallback = {"fringe": ("wooden_house", "merchant_brick_house"),
                    "core": ("merchant_brick_house", "workshop"),
                    "ordinary": ("merchant_brick_house", "workshop", "wooden_house")}[cls]
        order = [typ] + [t for t in fallback if t != typ]
        blocked_by = None
        for attempt, t in enumerate(order):
            size = choose_size(t, f"{key}.{attempt}", hi - p)
            if size is None:
                continue
            w, d, storeys, cellar = size
            for shrink in range(5):
                ww = w - 4.0 * shrink
                if ww < DESIGN_LIMITS[t]["width"][0] - 1e-6:
                    break
                rect = fp(p, ww, d, 0.0)
                ok, why, info = placer.check(rect)
                if not ok:
                    if why == "overlap":
                        blocked_by = info
                    continue
                pad, ideal = placer.pad_for(rect, info, frontage_point(rect, side))
                if pad is None:
                    continue
                placed = dict(typology=t, width_m=ww, depth_m=d, storeys=storeys,
                              cellar=bool(cellar and DESIGN_LIMITS[t]["cellar"]), rect=rect, yaw_deg=yaw,
                              block=block["id"], side=side, cls=cls, pad=pad, ideal=ideal, kind="street",
                              along=(p, p + ww))
                break
            if placed:
                break
        if placed is None:
            if blocked_by is not None:
                far = blocked_by["rect"][1] if side in ("n", "s") else blocked_by["rect"][3]
                p = max(p + 4.0, far)
            else:
                p += 4.0
            continue
        placed["id"] = f"urban_intro.{block['id']}.{side}{idx:02d}"
        placer.buildings.append(placed)
        idx += 1
        p = placed["along"][1]
        gap_kind = pick(RECIPES[cls]["gaps"], rnd(key, 2))
        g = gap_length(gap_kind, key)
        if g > 0 and p + g < hi:
            gaps_out.append(dict(block=block["id"], side=side, kind=gap_kind, along=(p, p + g), cls=cls,
                                 rect=fp(p, g, 0.4, 0.0), yaw=yaw))
        p += g
    return idx


def free_runs(placer, block, side, step=4.0):
    """Free stretches of a side's street line (no footprint within a thin strip behind it)."""
    lo, hi, fp, yaw = side_frame(block["rect"], side)
    runs, cur = [], None
    p = lo
    while p + step <= hi + 1e-6:
        probe = fp(p + 0.5, step - 1.0, 0.5, 0.25)
        occupied = any(rect_overlap(probe, b["rect"], 0.0) for b in placer.buildings)
        if not occupied:
            cur = (cur[0], p + step) if cur else (p, p + step)
        elif cur:
            runs.append(cur)
            cur = None
        p += step
    if cur:
        runs.append(cur)
    return runs, fp, yaw


def fill_flank(placer, block, side, cls, gaps_out):
    """A short side: the corner buildings' flanks already stand; add at most one building and
    close the rest with yard walls and gates (brick street), fences or nothing (timber fringe)."""
    runs, fp, yaw = free_runs(placer, block, side)
    key = f"{block['id']}.{side}.flank"
    made = 0
    if runs and rnd(key, 1) < FLANK_BUILDING_P[cls]:
        run = max(runs, key=lambda r: (r[1] - r[0], -r[0]))
        span = run[1] - run[0]
        if span >= 16.0:
            if cls == "fringe":
                typ = "wooden_house" if rnd(key, 2) < 0.8 else "merchant_brick_house"
            else:
                typ = "merchant_brick_house" if rnd(key, 2) < 0.7 else "workshop"
            size = choose_size(typ, key, span - 8.0)
            if size:
                w, d, storeys, cellar = size
                p = run[0] + 4.0 * math.floor(((span - w) / 2) / 4.0)
                rect = fp(p, w, d, 0.0)
                ok, why, info = placer.check(rect)
                if ok:
                    pad, ideal = placer.pad_for(rect, info, frontage_point(rect, side))
                    if pad is not None:
                        placer.buildings.append(dict(
                            id=f"urban_intro.{block['id']}.{side}{made:02d}", typology=typ, width_m=w, depth_m=d,
                            storeys=storeys, cellar=bool(cellar and DESIGN_LIMITS[typ]["cellar"]), rect=rect,
                            yaw_deg=yaw, block=block["id"], side=side, cls=cls, pad=pad, ideal=ideal,
                            kind="street", along=(p, p + w)))
                        made += 1
                        runs, fp, yaw = free_runs(placer, block, side)
    for k, (a, b) in enumerate(runs):
        if b - a < 4.0:
            continue
        if cls == "fringe":
            kind = "fence" if rnd(f"{key}.{k}", 3) < 0.7 else "open"
        else:
            kind = "wall" if rnd(f"{key}.{k}", 3) < 0.85 else "open"
        gaps_out.append(dict(block=block["id"], side=side, kind=kind, along=(a, b), cls=cls,
                             rect=fp(a, b - a, 0.4, 0.0), yaw=yaw))
    return made


def fill_slabs(placer, blocks, main):
    """1930s blocks as free-standing slabs in parallel rows behind the street range of the blocks at
    the district's north and south ends, behind the tram street's ranges first (R1 §4 arrangement;
    P L2 [07-01] [04-08])."""
    rows = sorted(set(b["row"] for b in blocks))
    for b in blocks:
        if b["edge_col"] or b["row"] not in (rows[0], rows[-1]):
            continue
        sides = [s for s in ("w", "e") if b["sides"][s] is main]
        if not sides:
            sides = sorted((s for s in ("w", "e") if b["side_cls"].get(s) == "ordinary"),
                           key=lambda s: rnd(b["id"] + s, 6))
        if not sides:
            continue
        side = sides[0]
        rng = [x for x in placer.buildings if x["block"] == b["id"] and x["side"] == side]
        if not rng:
            continue
        x0, x1, z0, z1 = b["rect"]
        deep = max(x["rect"][1] - x["rect"][0] for x in rng)
        key = b["id"] + ".slab"
        w = float(pick({40: .3, 48: .4, 56: .3}, rnd(key, 1)))
        d = float(pick({12: .6, 16: .4}, rnd(key, 2)))
        storeys = int(pick({4: .6, 5: .4}, rnd(key, 3)))
        court = 20.0 + 4.0 * math.floor(rnd(key, 4) * 3)
        zc = float(round((z0 + z1) / 2))
        if side == "e":
            sx1 = x1 - deep - court
            rect = (sx1 - d, sx1, zc - w / 2, zc + w / 2)
            yaw = 90.0
        else:
            sx0 = x0 + deep + court
            rect = (sx0, sx0 + d, zc - w / 2, zc + w / 2)
            yaw = 270.0
        ok, why, info = placer.check(rect)
        if not ok:
            continue
        pad, ideal = placer.pad_for(rect, info)
        if pad is None:
            continue
        placer.buildings.append(dict(
            id=f"urban_intro.{b['id']}.r00", typology="apartment_block", width_m=w, depth_m=d, storeys=storeys,
            cellar=rnd(key, 5) < SIZES["apartment_block"]["cellar"], rect=rect, yaw_deg=yaw, block=b["id"],
            side=side, cls="core", pad=pad, ideal=ideal, kind="slab", along=None))


def balance_area_shares(L):
    """R1 §4 area shares are upper-bounded: while a typology's footprint-area share is above its
    range, its widest building gives up one 4 m bay at its high end (a smaller footprint inside the
    old one, so pads, overlaps and streets stay valid); a street building's freed bay becomes a gate."""
    shrunk = []
    for _ in range(200):
        st = [b for b in L.buildings if not b["razed"]]
        total = sum(b["width_m"] * b["depth_m"] for b in st)
        over = [t for t in TYPOLOGIES
                if sum(b["width_m"] * b["depth_m"] for b in st if b["typology"] == t) / total > SHARE_BY_AREA[t][1] - 0.004]
        if not over:
            break
        t = over[0]
        cands = sorted((b for b in st if b["typology"] == t
                        and b["width_m"] - 4.0 >= DESIGN_LIMITS[t]["width"][0] - 1e-6),
                       key=lambda b: (-b["width_m"], b["id"]))
        if not cands:
            break
        b = cands[0]
        x0, x1, z0, z1 = b["rect"]
        along_x = int(round(b["yaw_deg"] / 90.0)) % 2 == 0
        if along_x:
            b["rect"] = (x0, x1 - 4.0, z0, z1)
            freed = (x1 - 4.0, x1, z0, z1)
        else:
            b["rect"] = (x0, x1, z0, z1 - 4.0)
            freed = (x0, x1, z1 - 4.0, z1)
        b["width_m"] -= 4.0
        if b.get("along"):
            b["along"] = (b["along"][0], b["along"][1] - 4.0)
            lo, hi = b["along"][1], b["along"][1] + 4.0
            side = b["side"]
            line = {"n": (lo, hi, freed[2], freed[2] + 0.4), "s": (lo, hi, freed[3] - 0.4, freed[3]),
                    "w": (freed[0], freed[0] + 0.4, lo, hi), "e": (freed[1] - 0.4, freed[1], lo, hi)}[side]
            L.gaps.append(dict(block=b["block"], side=side, kind="gate", along=(lo, hi), cls=b["cls"],
                               rect=line, yaw=b["yaw_deg"], balanced=True))
        shrunk.append(b["id"])
    L.balanced = shrunk


def destruction_intensity(L, x, z):
    """A relative weight, 0..1, for where January 1943 left least standing: the north and west of
    the district (the direction of the Soviet January assault, S27) and the square (the German
    strongpoint at the Square of Fallen Fighters, S14). It only orders the razed plots and the
    heavier conditions; the district-wide shares stay R1's."""
    ext = L.extent
    north = min(max((ext["z1"] - z) / (ext["z1"] - ext["z0"]), 0.0), 1.0)
    west = min(max((ext["x1"] - x) / (ext["x1"] - ext["x0"]), 0.0), 1.0)
    sq = L.square["rect"] if L.square else None
    near_sq = math.exp(-rect_dist(sq, x, z) / 120.0) if sq else 0.0
    return min(max(0.10 + 0.40 * north + 0.15 * west + 0.35 * near_sq, 0.0), 1.0)


def fill_yards(placer, blocks):
    """One or two workshops in the yards behind the street ranges (R1 §4: core yards hold one
    workshop, ordinary plots sometimes one; R1 §3.4 footprints)."""
    for b in blocks:
        if b["edge_col"]:
            continue
        street_bldgs = [x for x in placer.buildings if x["block"] == b["id"] and x["kind"] == "street"]
        want = 1 if any(c == "core" for c in b["side_cls"].values() if c) else (1 if rnd(b["id"], 31) < 0.6 else 0)
        want += 1 if rnd(b["id"], 32) < 0.35 else 0
        made = 0
        for host in sorted(street_bldgs, key=lambda x: (rnd(x["id"], 33), x["id"])):
            if made >= want:
                break
            x0, x1, z0, z1 = host["rect"]
            side = host["side"]
            key = host["id"] + ".yard"
            w = float(pick({12: .3, 16: .4, 20: .3}, rnd(key, 1)))   # R1 §3.4: typical 10-24 m
            d = float(pick({8: .5, 12: .5}, rnd(key, 2)))
            back = 6.0 + 4.0 * math.floor(rnd(key, 3) * 3)
            if side == "n":
                rect = ((x0 + x1) / 2 - w / 2, (x0 + x1) / 2 + w / 2, z1 + back, z1 + back + d)
            elif side == "s":
                rect = ((x0 + x1) / 2 - w / 2, (x0 + x1) / 2 + w / 2, z0 - back - d, z0 - back)
            elif side == "w":
                rect = (x1 + back, x1 + back + d, (z0 + z1) / 2 - w / 2, (z0 + z1) / 2 + w / 2)
            else:
                rect = (x0 - back - d, x0 - back, (z0 + z1) / 2 - w / 2, (z0 + z1) / 2 + w / 2)
            rect = tuple(float(round(v)) for v in rect)
            bx0, bx1, bz0, bz1 = b["rect"]
            if rect[0] < bx0 + 4 or rect[1] > bx1 - 4 or rect[2] < bz0 + 4 or rect[3] > bz1 - 4:
                continue
            ok, why, info = placer.check(rect)
            if not ok:
                continue
            if any(rect_gap(rect, n["rect"]) < 4.0 for n in info):
                continue
            pad, ideal = placer.pad_for(rect, info)
            if pad is None:
                continue
            # a yard workshop's facade faces the yard entrance, i.e. the same way as its host range
            placer.buildings.append(dict(
                id=f"urban_intro.{b['id']}.y{made:02d}", typology="workshop", width_m=w, depth_m=d, storeys=1,
                cellar=False, rect=rect, yaw_deg=host["yaw_deg"], block=b["id"], side=side, cls="yard",
                pad=pad, ideal=ideal, kind="yard", along=None))
            made += 1


def choose_razed(buildings):
    """R1 §6.2: razed plots are a further share of the masonry (rubble, not buildings). The
    destruction intensity only orders which buildings are razed."""
    masonry = [b for b in buildings if b["typology"] in MASONRY]
    n_razed = int(math.floor(pv("rubble.razed_share_of_masonry") * len(masonry)))
    order = sorted(masonry, key=lambda b: (rnd(b["id"], 41) - 1.0 * b["intensity"], b["id"]))
    razed = set(b["id"] for b in order[:n_razed])
    for b in buildings:
        b["razed"] = b["id"] in razed


def assign_conditions(buildings):
    """Every standing building gets one dominant condition in R1's inferred shares (§6.2), realised
    per typology by largest remainder so small counts still match the table; the intensity orders
    which take the heavier conditions."""
    standing = [b for b in buildings if not b["razed"]]
    for typ in TYPOLOGIES:
        group = [b for b in standing if b["typology"] == typ]
        n = len(group)
        if n == 0:
            continue
        shares = CONDITION_SHARES[typ]
        raw = {c: shares[c] * n for c in CONDITIONS}
        counts = {c: int(math.floor(raw[c])) for c in CONDITIONS}
        left = n - sum(counts.values())
        for c in sorted(CONDITIONS, key=lambda c: (-(raw[c] - counts[c]), CONDITIONS.index(c)))[:left]:
            counts[c] += 1
        # the rare intact building stands at the edge of the fighting (R1 §6.2: S10e, S10g; P L1 [07-01])
        fringe_first = sorted(group, key=lambda b: (-b["fringe_score"], rnd(b["id"], 42), b["id"]))
        intact = set(x["id"] for x in fringe_first[:counts["intact"]])
        rest = [b for b in group if b["id"] not in intact]
        rest.sort(key=lambda b: (rnd(b["id"], 43) + 0.5 * (1.0 - b["intensity"]), b["id"]))
        seq = (["partly_collapsed"] * counts["partly_collapsed"] + ["shelled"] * counts["shelled"]
               + ["burned"] * counts["burned"])
        for b in group:
            if b["id"] in intact:
                b["condition"] = "intact"
        for b, c in zip(rest, seq):
            b["condition"] = c
    for b in buildings:
        if b["razed"]:
            b["condition"] = "razed"


# ════════════════════════════════════════════════════════════════════════════════════════════════
# density pass (2026-09-29): the plan above left the short sides of every block to yard walls (5 %
# built) and the yards nearly empty, so the district read as buildings standing apart in open snow.
# R1 §4 and §0 ask for blocks packed with small buildings on every street line, merchant houses in
# continuous party-wall rows and yards full of sheds, stores and summer kitchens (S28, S10r, S10o).
# The pass only adds: every building placed above keeps its id, footprint, pad, condition and razing
# (the tableaux are authored on them), and the yards of the tableau hosts stay as authored.
# ════════════════════════════════════════════════════════════════════════════════════════════════
TABLEAU_CATALOG_REL = "catalog/urban_intro_tableaux.v1.json"
TABLEAU_ALONG_MARGIN_M = 10.0       # a host's band runs this far past its frontage on both ends
TABLEAU_SLAB_MARGIN_M = 20.0        # ... and round a free-standing slab
# hosts whose story plays in the street before them, not in their yard (the catalog's "choice" text):
# only the building and its neighbours' frontage are kept
TABLEAU_FRONT_ONLY = {
    "urban_intro.c3r2.w01": "the embrasure fires from the semi-basement across the tram street into the square",
    "urban_intro.c2r3.e00": "the tramcar stands on the rails in the tram street before its frontage",
}
DENSE_SLABS_MAX = 5                 # free-standing 1930s slabs added in the yards of blocks without one:
                                    # the new street fronts are small buildings, so the blocks' count
                                    # share (R1 §4: 0.08-0.12) needs a few more of them
MAIN_KINDS = ("street", "slab", "yard")   # R1 §4's main buildings (yard workshops and stores included)
OUTBUILDING = "outbuilding"         # summer kitchens and timber yard cottages: yard contents, counted apart
DENSE_GAPS = {                      # between the new street-front buildings (party walls first, R1 §4)
    "core": {"party": 0.66, "gate": 0.24, "wall": 0.08, "open": 0.02},
    "ordinary": {"party": 0.46, "gate": 0.26, "wall": 0.16, "open": 0.12},
    "fringe": {"fence": 0.62, "gate": 0.08, "open": 0.30},
}
DENSE_FALLBACK = {"fringe": ("wooden_house", "merchant_brick_house"),
                  "core": ("merchant_brick_house", "workshop"),
                  "ordinary": ("merchant_brick_house", "workshop", "wooden_house")}
SHARE_STEER_GAIN = 2.0              # exp(gain * (range middle - share) / half range), by count and by area
DENSE_PAD_STEP_M = 0.12             # the pass's party-wall pad step: its neighbours' shared skirts stay inside the
                                    # 0.10 m flatness tolerance (PAD_STEP_M 0.15 left one old pad at 0.106 m)
SHARE_HEADROOM = 0.004              # a new building may not take a share closer than this to its upper bound
# yard contents per plot behind a brick street building (R1 §4 recipes: core yards one workshop and
# sheds; ordinary plots 1-4 sheds or stores, a summer kitchen, sometimes a workshop). Sheds and fences
# are dressing (their burned remains); stores and kitchens are grammar buildings.
YARD_BUILDING_P = {"core": {"store": 0.30, "kitchen": 0.35, "none": 0.35},
                   "ordinary": {"store": 0.25, "kitchen": 0.55, "none": 0.20}}
YARD_CLEAR_M = 3.0                  # a detached yard building keeps a passage to its neighbours
YARD_EDGE_INSET_M = 3.0             # and stays off the block's street lines


def tableau_keepouts(L):
    """Bands the density pass leaves exactly as the tableaux were authored on: from a host's street
    line across its whole block (its yard and the open ground its story reads across), along its
    frontage plus a margin; a slab host's surroundings; the catalog names the hosts."""
    data = json.loads((ROOT / TABLEAU_CATALOG_REL).read_text())
    hosts = sorted(set(t["host"]["instance"] for t in data["tableaux"]))
    bmap = {b["id"]: b for b in L.buildings}
    blocks = {b["id"]: b for b in L.blocks}
    out = []
    for h in hosts:
        b = bmap.get(h)
        if b is None or b["block"] not in blocks:
            continue
        bx0, bx1, bz0, bz1 = blocks[b["block"]]["rect"]
        x0, x1, z0, z1 = b["rect"]
        if b["kind"] == "slab":
            m = TABLEAU_SLAB_MARGIN_M
            out.append((h, (max(x0 - m, bx0), min(x1 + m, bx1), max(z0 - m, bz0), min(z1 + m, bz1))))
            continue
        m = TABLEAU_ALONG_MARGIN_M
        if h in TABLEAU_FRONT_ONLY:
            out.append((h, (max(x0 - m, bx0), min(x1 + m, bx1), max(z0 - m, bz0), min(z1 + m, bz1))))
            continue
        if b["side"] in ("w", "e"):
            out.append((h, (bx0, bx1, max(z0 - m, bz0), min(z1 + m, bz1))))
        else:
            out.append((h, (max(x0 - m, bx0), min(x1 + m, bx1), bz0, bz1)))
    return out


def in_keepout(rect, keep):
    return any(rect_overlap(rect, k, 0.0) for _, k in keep)


class ShareSteer:
    """Running main-building counts and footprint areas, steering each new building's typology
    toward the middle of R1 §4's district shares and refusing one that would pass an upper bound."""

    def __init__(self, buildings):
        self.n = {t: 0 for t in TYPOLOGIES}
        self.a = {t: 0.0 for t in TYPOLOGIES}
        for b in buildings:
            self.add(b)

    def add(self, b):
        if b["kind"] in MAIN_KINDS and not b.get("razed"):
            self.n[b["typology"]] += 1
            self.a[b["typology"]] += b["width_m"] * b["depth_m"]

    def factor(self, t):
        N = max(sum(self.n.values()), 1)
        A = max(sum(self.a.values()), 1.0)
        f = 1.0
        for s, (lo, hi) in ((self.n[t] / N, SHARE_BY_COUNT[t]), (self.a[t] / A, SHARE_BY_AREA[t])):
            f *= math.exp(SHARE_STEER_GAIN * ((lo + hi) / 2 - s) / ((hi - lo) / 2))
        return f

    def allowed(self, t, area):
        N = sum(self.n.values()) + 1
        A = sum(self.a.values()) + area
        return ((self.n[t] + 1) / N <= SHARE_BY_COUNT[t][1] - SHARE_HEADROOM
                and (self.a[t] + area) / A <= SHARE_BY_AREA[t][1] - SHARE_HEADROOM)

    def pick(self, cls, u):
        rec = RECIPES[cls]["shares"]
        mean_w = {t: sum(w * p for w, p in SIZES[t]["width"].items()) / sum(SIZES[t]["width"].values())
                  for t in TYPOLOGIES}
        probs = {t: rec[t] / mean_w[t] * self.factor(t) for t in TYPOLOGIES if rec[t] > 0}
        return pick(probs, u)


def fill_run(L, steer, block, side, cls, lo, hi, idx0, keep, gaps_out):
    """fill_side over one free stretch [lo, hi) of a side's street line, for the density pass: the
    typology is steered by the district's running shares, and nothing is placed in a tableau band."""
    placer = L.placer
    _, _, fp, yaw = side_frame(block["rect"], side)
    p, idx, tries, made = lo, idx0, 0, []
    while p + 8.0 <= hi + 1e-6 and tries < 200:
        tries += 1
        key = f"dense.{block['id']}.{side}.{idx}.{int(p)}"
        typ = steer.pick(cls, rnd(key, 1))
        order = [typ] + [t for t in DENSE_FALLBACK[cls] if t != typ]
        placed, blocked_by, last = None, None, "size"
        for attempt, t in enumerate(order):
            size = choose_size(t, f"{key}.{attempt}", hi - p)
            if size is None:
                continue
            w, d, storeys, cellar = size
            for shrink in range(6):
                ww = w - 4.0 * shrink
                if ww < DESIGN_LIMITS[t]["width"][0] - 1e-6:
                    break
                if not steer.allowed(t, ww * d):
                    last = "share"
                    continue
                rect = fp(p, ww, d, 0.0)
                if in_keepout(rect, keep):
                    last = "tableau"
                    continue
                ok, why, info = placer.check(rect)
                if not ok:
                    last = why
                    if why == "overlap":
                        blocked_by = info
                    continue
                pad, ideal = placer.pad_for(rect, info, frontage_point(rect, side), step=DENSE_PAD_STEP_M)
                if pad is None:
                    last = "pad"
                    continue
                placed = dict(typology=t, width_m=ww, depth_m=d, storeys=storeys,
                              cellar=bool(cellar and DESIGN_LIMITS[t]["cellar"]), rect=rect, yaw_deg=yaw,
                              block=block["id"], side=side, cls=cls, pad=pad, ideal=ideal, kind="street",
                              along=(p, p + ww), dense=True)
                break
            if placed:
                break
        if placed is None:
            if blocked_by is not None:
                far = blocked_by["rect"][1] if side in ("n", "s") else blocked_by["rect"][3]
                step = max(4.0, far - p)
            else:
                step = 4.0
            L.dense_open[last] = L.dense_open.get(last, 0.0) + min(step, hi - p)
            p += step
            continue
        placed["id"] = f"urban_intro.{block['id']}.{side}{idx:02d}"
        placer.buildings.append(placed)
        steer.add(placed)
        made.append(placed)
        idx += 1
        p = placed["along"][1]
        gap_kind = pick(DENSE_GAPS[cls], rnd(key, 2))
        g = gap_length(gap_kind, key)
        if g > 0 and p + g < hi:
            gaps_out.append(dict(block=block["id"], side=side, kind=gap_kind, along=(p, p + g), cls=cls,
                                 rect=fp(p, g, 0.4, 0.0), yaw=yaw, dense=True))
        p += g
    return made


def close_runs(L, block, side, cls):
    """Every free stretch of a densified side that no gap record covers is closed as fill_flank
    closes a short side: yard walls (brick streets) or board fences (the timber fringe), a few left open."""
    runs, fp, yaw = free_runs(L.placer, block, side)
    spans = sorted(g["along"] for g in L.gaps if g["block"] == block["id"] and g["side"] == side)
    for (a, b) in runs:
        cur = a
        pieces = []
        for (ga, gb) in spans:
            if gb <= cur or ga >= b:
                continue
            if ga > cur:
                pieces.append((cur, ga))
            cur = max(cur, gb)
        if cur < b:
            pieces.append((cur, b))
        for (pa, pb) in pieces:
            if pb - pa < 4.0 - 1e-6:
                continue
            key = f"dense.close.{block['id']}.{side}.{int(pa)}"
            if cls == "fringe":
                kind = "fence" if rnd(key, 3) < 0.7 else "open"
            else:
                kind = "wall" if rnd(key, 3) < 0.8 else ("gate" if rnd(key, 4) < 0.5 else "open")
            L.gaps.append(dict(block=block["id"], side=side, kind=kind, along=(pa, pb), cls=cls,
                               rect=fp(pa, pb - pa, 0.4, 0.0), yaw=yaw, dense=True))


def next_index(buildings, block_id, tag):
    """The next free two-digit index of ids urban_intro.<block>.<tag>NN."""
    pat = re.compile(r"^urban_intro\.%s\.%s(\d\d)$" % (re.escape(block_id), re.escape(tag)))
    used = [int(m.group(1)) for m in (pat.match(b["id"]) for b in buildings) if m]
    return max(used) + 1 if used else 0


def densify_frontage(L, steer, keep):
    """Short sides become street fronts (their yard walls and gates only where no building fits),
    and a vacant lot on a long side takes a building where one fits between its neighbours."""
    placer = L.placer
    changed = []
    for b in L.blocks:
        longs = long_sides(b)
        for side in SIDES:
            cls = b["side_cls"][side]
            if cls is None:
                continue
            runs, fp, yaw = free_runs(placer, b, side)
            if side in longs:
                opens = [g["along"] for g in L.gaps if g["block"] == b["id"] and g["side"] == side and g["kind"] == "open"]
                targets = []
                for (ga, gb) in opens:
                    for (ra, rb) in runs:
                        a, c = max(ga, ra), min(gb, rb)
                        if c - a >= 8.0 - 1e-6:
                            targets.append((a, c))
            else:
                targets = [r for r in runs if r[1] - r[0] >= 8.0 - 1e-6]
            made = []
            for (a, c) in sorted(targets):
                idx = next_index(placer.buildings, b["id"], side)
                made += fill_run(L, steer, b, side, cls, a, c, idx, keep, L.gaps)
            if not made:
                continue
            # old gap records the new fronts stand on give way; what stays free is closed again
            spans = [m["along"] for m in made]
            L.gaps = [g for g in L.gaps if not (g["block"] == b["id"] and g["side"] == side and not g.get("dense")
                                                and any(g["along"][0] < s1 - 1e-6 and s0 < g["along"][1] - 1e-6
                                                        for s0, s1 in spans))]
            close_runs(L, b, side, cls)
            changed.append((b["id"], side, len(made)))
    return changed


def yard_rect(host, back, w, d, lateral=0.0):
    """A yard building's footprint behind a street building: `back` metres behind its rear wall,
    `w` along the frontage (offset `lateral` from the host's middle), `d` deep."""
    x0, x1, z0, z1 = host["rect"]
    side = host["side"]
    if side == "n":
        m = (x0 + x1) / 2 + lateral
        return (m - w / 2, m + w / 2, z1 + back, z1 + back + d)
    if side == "s":
        m = (x0 + x1) / 2 + lateral
        return (m - w / 2, m + w / 2, z0 - back - d, z0 - back)
    if side == "w":
        m = (z0 + z1) / 2 + lateral
        return (x1 + back, x1 + back + d, m - w / 2, m + w / 2)
    m = (z0 + z1) / 2 + lateral
    return (x0 - back - d, x0 - back, m - w / 2, m + w / 2)


def plot_depth_limit(block, host):
    """How deep a host's plot runs: to the block's middle across its long axis (plots back to back,
    R1 §4) for a long-side host, and a third of the block for a short-side host."""
    x0, x1, z0, z1 = block["rect"]
    side = host["side"]
    across = (x1 - x0) if side in ("w", "e") else (z1 - z0)
    return across / 2 if side in long_sides(block) else across / 3


def fill_outbuildings(L, steer, keep):
    """Yard buildings behind the brick street ranges (R1 §4, §5.3): a brick store or workshop as an
    annex against the rear wall or free-standing in the yard (grammar workshops, main buildings of
    R1's workshop typology), or a summer kitchen, the plot's timber yard cottage (grammar wooden
    houses, burned to plinth and stove like the street's; counted apart as yard contents)."""
    placer = L.placer
    bmap = {b["id"]: b for b in L.blocks}
    hosts = sorted((h for h in placer.buildings if h["kind"] == "street" and not bmap[h["block"]]["edge_col"]
                    and h["typology"] in ("merchant_brick_house", "apartment_block")), key=lambda h: h["id"])
    made = []
    for host in hosts:
        blk = bmap[host["block"]]
        cls = "core" if host["cls"] == "core" else "ordinary"
        key = host["id"] + ".yard2"
        what = pick(YARD_BUILDING_P[cls], rnd(key, 1))
        if what == "none":
            continue
        if what == "store" and not steer.allowed("workshop", 64.0):
            what = "kitchen"     # the workshops' share is full (R1 §4: 0.10-0.20 by count)
        depth_limit = plot_depth_limit(blk, host)
        cands = []
        if what == "store":
            w = float(pick({8: .5, 12: .35, 16: .15}, rnd(key, 2)))
            w = min(w, host["width_m"])
            d = float(pick({8: .7, 12: .3}, rnd(key, 3)))
            if rnd(key, 4) < 0.55:   # an annex against the rear wall (S10r: "a considerable number of annexes")
                for lat in (0.0, (host["width_m"] - w) / 2, -(host["width_m"] - w) / 2):
                    cands.append((0.0, w, d, lat))
            for back in (4.0, 8.0, 12.0, 16.0, 20.0):
                for lat in (0.0, (host["width_m"] - w) / 2, -(host["width_m"] - w) / 2):
                    cands.append((back, w, d, lat))
            typ, kind, tag = "workshop", "yard", "y"
        else:
            w, d = 8.0, 8.0
            for back in (5.0, 9.0, 13.0, 17.0, 21.0):
                for lat in ((host["width_m"] - w) / 2, -(host["width_m"] - w) / 2, 0.0):
                    cands.append((back, w, d, lat))
            typ, kind, tag = "wooden_house", OUTBUILDING, "k"
        if kind in MAIN_KINDS and not steer.allowed(typ, w * d):
            continue
        for (back, w, d, lat) in cands:
            if host["depth_m"] + back + d > depth_limit:
                continue
            rect = tuple(float(round(v)) for v in yard_rect(host, back, w, d, lat))
            if (rect[0] < blk["rect"][0] + YARD_EDGE_INSET_M or rect[1] > blk["rect"][1] - YARD_EDGE_INSET_M
                    or rect[2] < blk["rect"][2] + YARD_EDGE_INSET_M or rect[3] > blk["rect"][3] - YARD_EDGE_INSET_M):
                continue
            if in_keepout(rect, keep):
                continue
            ok, why, info = placer.check(rect)
            if not ok:
                continue
            if any(rect_gap(rect, n["rect"]) < YARD_CLEAR_M and not (back == 0.0 and n is host) for n in info):
                continue
            pad, ideal = placer.pad_for(rect, info, step=DENSE_PAD_STEP_M)
            if pad is None:
                continue
            idx = next_index(placer.buildings, blk["id"], tag)
            nb = dict(id=f"urban_intro.{blk['id']}.{tag}{idx:02d}", typology=typ, width_m=w, depth_m=d, storeys=1,
                      cellar=False, rect=rect, yaw_deg=host["yaw_deg"], block=blk["id"], side=host["side"], cls="yard",
                      pad=pad, ideal=ideal, kind=kind, along=None, dense=True, host=host["id"])
            placer.buildings.append(nb)
            steer.add(nb)
            made.append(nb)
            break
    return made


def assign_new(L, new):
    """Razing and conditions for the density pass's buildings, so the whole district keeps R1's
    shares (razed 5-15 % of the masonry, §6.2 conditions per typology by largest remainder) while
    every building placed before keeps what it had."""
    for b in new:
        cx = (b["rect"][0] + b["rect"][1]) / 2
        cz = (b["rect"][2] + b["rect"][3]) / 2
        b["intensity"] = destruction_intensity(L, cx, cz)
        xm = [s for s in L.streets if s["cls"] == "main"][0]["c"]
        half = max(xm - L.extent["x0"], L.extent["x1"] - xm)
        b["fringe_score"] = abs(cx - xm) / half + (0.5 if b["cls"] == "fringe" else 0.0) + (1.0 - b["intensity"])
        b["razed"] = False
    masonry = [b for b in L.buildings if b["typology"] in MASONRY]
    n_razed = int(math.floor(pv("rubble.razed_share_of_masonry") * len(masonry)))
    have = sum(1 for b in masonry if b["razed"])
    cands = sorted((b for b in new if b["typology"] in MASONRY and b["kind"] == "street"),
                   key=lambda b: (rnd(b["id"], 41) - 1.0 * b["intensity"], b["id"]))
    for b in cands[:max(n_razed - have, 0)]:
        b["razed"] = True
    new_ids = set(b["id"] for b in new)
    for typ in TYPOLOGIES:
        group = [b for b in L.buildings if b["typology"] == typ and not b["razed"]]
        fresh = [b for b in group if b["id"] in new_ids]
        if not fresh:
            continue
        n = len(group)
        shares = CONDITION_SHARES[typ]
        raw = {c: shares[c] * n for c in CONDITIONS}
        counts = {c: int(math.floor(raw[c])) for c in CONDITIONS}
        left = n - sum(counts.values())
        for c in sorted(CONDITIONS, key=lambda c: (-(raw[c] - counts[c]), CONDITIONS.index(c)))[:left]:
            counts[c] += 1
        old = {c: sum(1 for b in group if b["id"] not in new_ids and b["condition"] == c) for c in CONDITIONS}
        want = {c: max(counts[c] - old[c], 0) for c in CONDITIONS}
        while sum(want.values()) > len(fresh):     # the old buildings already hold more of a condition
            c = max(CONDITIONS, key=lambda c: (want[c] - (raw[c] - old[c]), -CONDITIONS.index(c)))
            want[c] -= 1
        while sum(want.values()) < len(fresh):
            c = max(CONDITIONS, key=lambda c: ((raw[c] - old[c]) - want[c], -CONDITIONS.index(c)))
            want[c] += 1
        fringe_first = sorted(fresh, key=lambda b: (-b["fringe_score"], rnd(b["id"], 42), b["id"]))
        intact = set(x["id"] for x in fringe_first[:want["intact"]])
        rest = sorted((b for b in fresh if b["id"] not in intact),
                      key=lambda b: (rnd(b["id"], 43) + 0.5 * (1.0 - b["intensity"]), b["id"]))
        seq = ["partly_collapsed"] * want["partly_collapsed"] + ["shelled"] * want["shelled"] + ["burned"] * want["burned"]
        for b in fresh:
            if b["id"] in intact:
                b["condition"] = "intact"
        for b, c in zip(rest, seq):
            b["condition"] = c
    for b in new:
        if b["razed"]:
            b["condition"] = "razed"


def balance_new_area_shares(L, new):
    """balance_area_shares, giving up bays only of the density pass's buildings."""
    new_ids = set(b["id"] for b in new)
    shrunk = []
    for _ in range(400):
        st = [b for b in L.buildings if not b["razed"] and b["kind"] in MAIN_KINDS]
        total = sum(b["width_m"] * b["depth_m"] for b in st)
        over = [t for t in TYPOLOGIES
                if sum(b["width_m"] * b["depth_m"] for b in st if b["typology"] == t) / total > SHARE_BY_AREA[t][1] - 0.004]
        if not over:
            break
        t = over[0]
        cands = sorted((b for b in st if b["typology"] == t and b["id"] in new_ids and b["kind"] == "street"
                        and b["width_m"] - 4.0 >= DESIGN_LIMITS[t]["width"][0] - 1e-6),
                       key=lambda b: (-b["width_m"], b["id"]))
        if not cands:
            break
        b = cands[0]
        x0, x1, z0, z1 = b["rect"]
        along_x = int(round(b["yaw_deg"] / 90.0)) % 2 == 0
        if along_x:
            b["rect"] = (x0, x1 - 4.0, z0, z1)
            freed = (x1 - 4.0, x1, z0, z1)
        else:
            b["rect"] = (x0, x1, z0, z1 - 4.0)
            freed = (x0, x1, z1 - 4.0, z1)
        b["width_m"] -= 4.0
        b["along"] = (b["along"][0], b["along"][1] - 4.0)
        lo, hi = b["along"][1], b["along"][1] + 4.0
        side = b["side"]
        line = {"n": (lo, hi, freed[2], freed[2] + 0.4), "s": (lo, hi, freed[3] - 0.4, freed[3]),
                "w": (freed[0], freed[0] + 0.4, lo, hi), "e": (freed[1] - 0.4, freed[1], lo, hi)}[side]
        L.gaps.append(dict(block=b["block"], side=side, kind="gate", along=(lo, hi), cls=b["cls"],
                           rect=line, yaw=b["yaw_deg"], balanced=True, dense=True))
        shrunk.append(b["id"])
    return shrunk


def densify_slabs(L, steer, keep):
    """R1 §4 arrangement / P L2 [07-01] [04-08]: 1930s slabs also stand free in the yards of the
    blocks between the district's end rows, set back from the street in parallel with the street
    ranges; added while the apartment blocks' count share is low (their frontage share is small)."""
    placer = L.placer
    made = []
    has_slab = set(b["block"] for b in placer.buildings if b["kind"] == "slab")
    cands = sorted((b for b in L.blocks if not b["edge_col"] and b["id"] not in has_slab),
                   key=lambda b: (rnd(b["id"] + ".slab2", 1), b["id"]))
    for blk in cands:
        if len(made) >= DENSE_SLABS_MAX:
            break
        x0, x1, z0, z1 = blk["rect"]
        key = blk["id"] + ".slab2"
        storeys = int(pick({4: .6, 5: .4}, rnd(key, 3)))
        d = float(pick({12: .6, 16: .4}, rnd(key, 2)))
        placed = None
        for w in sorted({float(pick({40: .3, 48: .4, 56: .3}, rnd(key, 1))), 40.0, 32.0}, reverse=True):
            for off in (0.0, -8.0, 8.0, -16.0, 16.0):
                cx = float(round((x0 + x1) / 2 + off))
                zc = float(round((z0 + z1) / 2))
                rect = (cx - d / 2, cx + d / 2, zc - w / 2, zc + w / 2)
                if in_keepout(rect, keep) or not steer.allowed("apartment_block", w * d):
                    continue
                ok, why, info = placer.check(rect)
                if not ok or any(rect_gap(rect, n["rect"]) < 8.0 for n in info):
                    continue
                pad, ideal = placer.pad_for(rect, info, step=DENSE_PAD_STEP_M)
                if pad is None:
                    continue
                side = "e" if off >= 0 else "w"
                placed = dict(id=f"urban_intro.{blk['id']}.r{next_index(placer.buildings, blk['id'], 'r'):02d}",
                              typology="apartment_block", width_m=w, depth_m=d, storeys=storeys,
                              cellar=rnd(key, 5) < SIZES["apartment_block"]["cellar"], rect=rect,
                              yaw_deg=90.0 if side == "e" else 270.0, block=blk["id"], side=side, cls="core",
                              pad=pad, ideal=ideal, kind="slab", along=None, dense=True)
                break
            if placed:
                break
        if placed:
            placer.buildings.append(placed)
            steer.add(placed)
            made.append(placed)
    return made


def densify(L):
    keep = tableau_keepouts(L)
    L.tableau_keepouts = keep
    L.dense_open = {}
    before = set(b["id"] for b in L.buildings)
    steer = ShareSteer(L.placer.buildings)
    slabs = densify_slabs(L, steer, keep)
    frontage = densify_frontage(L, steer, keep)
    yards = fill_outbuildings(L, steer, keep)
    new = [b for b in L.placer.buildings if b["id"] not in before]
    for b in new:
        b["seed"] = fnv1a32(b["id"]) ^ (SEED & 0xFFFFFFFF)
    assign_new(L, new)
    shrunk = balance_new_area_shares(L, new)
    L.buildings = sorted(L.placer.buildings, key=lambda b: b["id"])
    L.density = dict(new_buildings=len(new), new_slabs=len(slabs),
                     new_street_fronts=sum(1 for b in new if b["kind"] == "street"),
                     new_yard_workshops=sum(1 for b in new if b["kind"] == "yard"),
                     new_outbuildings=sum(1 for b in new if b["kind"] == OUTBUILDING),
                     new_razed=sum(1 for b in new if b["razed"]), sides_densified=len(frontage),
                     shrunk_for_area_shares=shrunk,
                     tableau_keepouts=[dict(host=h, rect=[r3(v) for v in k]) for h, k in keep])


# ════════════════════════════════════════════════════════════════════════════════════════════════
# layout
# ════════════════════════════════════════════════════════════════════════════════════════════════
RING_M = 150.0                 # the open winter ground dressed around the district
SPAWN_CLEAR_BUILDINGS_M = 22.0
MISSION_CLEAR_M = {"wreck": 16.0, "withdraw": 18.0}
MAP_SITE_CLEAR_M = 15.0
EXISTING_OBJECT_CLEAR_M = 1.0  # beyond a footprint's one-cell skirt


class Layout:
    pass


def build_layout(src, rust, grid):
    L = Layout()
    L.grid = grid
    L.rust = rust
    L.extent, L.masks = choose_extent(src, grid, src.heights, rust)
    ext = L.extent
    L.streets, L.diag = plan_streets(ext, src, rust)
    L.blocks, L.square = plan_blocks(ext, L.streets)
    edge = grid.world_half - 10.0
    ring = (max(ext["x0"] - RING_M, -edge), min(ext["x1"] + RING_M, edge),
            max(ext["z0"] - RING_M, -edge), min(ext["z1"] + RING_M, edge))
    L.ring = ring
    L.objects = [o for o in src.objects() if ring[0] <= o[0] <= ring[1] and ring[2] <= o[2] <= ring[3]]
    cell = grid.sp
    discs = [(src.spawn[0], src.spawn[1], SPAWN_CLEAR_BUILDINGS_M, "spawn"),
             (rust["wreck"][0], rust["wreck"][1], MISSION_CLEAR_M["wreck"], "mission_wreck"),
             (rust["withdraw"][0], rust["withdraw"][1], MISSION_CLEAR_M["withdraw"], "mission_withdraw")]
    L.map_sites = []
    for site in src.level_design.get("military_map_sites", []):
        x, z = site["position_xz_m"]
        if ring[0] <= x <= ring[1] and ring[2] <= z <= ring[3]:
            L.map_sites.append(dict(id=site["id"], x=float(x), z=float(z), hint=site.get("placement_hint", "")))
            discs.append((float(x), float(z), MAP_SITE_CLEAR_M, "map_site"))
    for (x, y, z, stem, origin) in L.objects:
        discs.append((x, z, cell + EXISTING_OBJECT_CLEAR_M, "existing_object"))
    pad = src.courtyard_pad
    ap = src.courtyard_apron
    L.courtyard_keep = (pad[0] - ap, pad[1] + ap, pad[2] - ap, pad[3] + ap)
    # every sample whose cell touches the keep keeps its source height, so the courtyard's surface is
    # exactly the prepared one; footprints keep their own one-cell skirts outside that frozen zone
    L.courtyard_frozen = (L.courtyard_keep[0] - cell, L.courtyard_keep[1] + cell,
                          L.courtyard_keep[2] - cell, L.courtyard_keep[3] + cell)
    k = cell + 0.5
    prects = [((L.courtyard_frozen[0] - k, L.courtyard_frozen[1] + k, L.courtyard_frozen[2] - k,
                L.courtyard_frozen[3] + k), "courtyard")]
    L.discs = discs
    L.G = gaussian(src.heights.astype(np.float64), STREET_GRADE_SIGMA_M / grid.sp)
    L.placer = Placer(grid, src.heights, L.masks, discs, prects, L.diag, L.streets, L.square, L.G)
    L.gaps = []
    main = [s for s in L.streets if s["cls"] == "main"][0]
    # phase 1: the long sides (plots back to back), most important street first
    # phase 2: short sides that face the square are full street fronts too
    # phase 3: 1930s slabs in parallel rows behind the tram-street ranges
    # phase 4: the remaining short sides: flanks, yard walls, gates, fences
    # phase 5: workshops in the yards
    order = []
    for b in L.blocks:
        longs = long_sides(b)
        for side in SIDES:
            cls = b["side_cls"][side]
            if cls is None:
                continue
            phase = 1 if side in longs else (2 if cls == "core" else 4)
            order.append((phase, -CLASS_PRIORITY[cls], b["id"], SIDES.index(side), b, side, cls))
    order.sort(key=lambda t: t[:4])
    for phase, _, _, _, b, side, cls in order:
        if phase == 4 and not getattr(L, "_slabs_done", False):
            fill_slabs(L.placer, L.blocks, main)
            L._slabs_done = True
        if phase in (1, 2):
            fill_side(L.placer, b, side, cls, L.gaps)
        else:
            fill_flank(L.placer, b, side, cls, L.gaps)
    if not getattr(L, "_slabs_done", False):
        fill_slabs(L.placer, L.blocks, main)
    fill_yards(L.placer, L.blocks)
    L.buildings = L.placer.buildings
    xm = main["c"]
    half = max(xm - ext["x0"], ext["x1"] - xm)
    for b in L.buildings:
        cx = (b["rect"][0] + b["rect"][1]) / 2
        cz = (b["rect"][2] + b["rect"][3]) / 2
        b["intensity"] = destruction_intensity(L, cx, cz)
        b["fringe_score"] = abs(cx - xm) / half + (0.5 if b["cls"] == "fringe" else 0.0) + (1.0 - b["intensity"])
    choose_razed(L.buildings)
    L.buildings.sort(key=lambda b: b["id"])
    for b in L.buildings:
        b["seed"] = fnv1a32(b["id"]) ^ (SEED & 0xFFFFFFFF)
    L.src_spawn = src.spawn
    L.ridge = choose_ridge(L)
    L.ridge_forced = False
    L.ridge_restored = None
    if L.ridge is None:
        # no razed block faces a side street: the tallest standing one there loses its facade
        bmap = {b["id"]: b for b in L.blocks}
        cands = sorted((b for b in L.buildings if not b["razed"] and b["kind"] == "street"
                        and b["typology"] == "apartment_block" and b["storeys"] >= 4
                        and (bmap[b["block"]]["sides"][b["side"]] or {}).get("cls") == "ordinary"),
                       key=lambda b: (-(b["storeys"] * b["width_m"]), b["id"]))
        for b in cands:
            b["razed"] = True
            L.ridge = choose_ridge(L)
            if L.ridge:
                L.ridge_forced = True
                # keep the razed count at R1's share: the least-destroyed razed plot stands again
                back = sorted((x for x in L.buildings if x["razed"] and x["id"] != b["id"]),
                              key=lambda x: (x["intensity"], x["id"]))
                if back:
                    back[0]["razed"] = False
                    L.ridge_restored = back[0]["id"]
                break
            b["razed"] = False
    assign_conditions(L.buildings)
    balance_area_shares(L)
    densify(L)
    for b in L.buildings:
        x0, x1, z0, z1 = b["rect"]
        b["x"] = (x0 + x1) / 2
        b["z"] = (z0 + z1) / 2
    L.craters = place_craters(L)
    return L


def building_instances(L):
    out = []
    for b in L.buildings:
        if b["razed"]:
            continue
        out.append({"id": b["id"], "typology": b["typology"], "seed": int(b["seed"]),
                    "width_m": float(b["width_m"]), "depth_m": float(b["depth_m"]), "storeys": int(b["storeys"]),
                    "cellar": bool(b["cellar"]), "condition": b["condition"], "x": float(b["x"]),
                    "z": float(b["z"]), "yaw_deg": float(b["yaw_deg"])})
    return out


def footprint_of_instance(inst):
    q = int(round(inst["yaw_deg"] / 90.0)) % 2
    w, d = (inst["width_m"], inst["depth_m"]) if q == 0 else (inst["depth_m"], inst["width_m"])
    return (inst["x"] - w / 2, inst["x"] + w / 2, inst["z"] - d / 2, inst["z"] + d / 2)


# ════════════════════════════════════════════════════════════════════════════════════════════════
# terrain: pads and aprons, razed-plot mounds, the fallen-facade ridge, craters
# ════════════════════════════════════════════════════════════════════════════════════════════════
# R1 §6.3 mass balance (loose = solid brickwork x 1.5 bulking): volume per m2 of footprint and
# the crest the photographs show. [I] from R1's worked examples; the workshop line is inferred
# from its thinner walls (R1 §3.4).
MOUND_MODEL = {
    "merchant_brick_house": {1: (0.70, 1.2, 2.0), 2: (1.10, 2.2, 2.0)},   # (m3/m2, crest m, apron m)
    "workshop": {1: (0.50, 0.9, 2.0)},
}
APARTMENT_MOUND_PER_STOREY = (0.757, 0.85, 4.0)   # 48 x 13 m, 4 storeys: 1,888 m3, crest 3-4 m


def mound_model(b):
    if b["typology"] == "apartment_block":
        v, c, a = APARTMENT_MOUND_PER_STOREY
        return v * b["storeys"], c * b["storeys"], a
    tab = MOUND_MODEL[b["typology"]]
    return tab[min(b["storeys"], max(tab))]


def dome(X, Z, rect, apron, crest, volume):
    """Rubble dome over rect + apron whose analytic volume is `volume` with peak `crest`."""
    x0, x1, z0, z1 = rect[0] - apron, rect[1] + apron, rect[2] - apron, rect[3] + apron
    area = (x1 - x0) * (z1 - z0)
    # a plot that lost most of its rubble to the street keeps a lower mound, not a thin spike
    crest = min(crest, volume / (0.30 * area))
    mean_f = min(max(volume / max(crest * area, 1e-6), 0.12), 0.8)
    s = math.sqrt(mean_f)
    p = min(max(s / (1.0 - s), 0.8), 6.0)
    cx, cz = (x0 + x1) / 2, (z0 + z1) / 2
    u = np.abs(X - cx) / ((x1 - x0) / 2)
    v = np.abs(Z - cz) / ((z1 - z0) / 2)
    f = np.clip(1.0 - u ** p, 0.0, None) * np.clip(1.0 - v ** p, 0.0, None)
    return crest * f, dict(apron=apron, crest=r3(crest), exponent=r3(p), analytic_volume_m3=r3(crest * area * (p / (p + 1)) ** 2))


def crater_profile(X, Z, x, z, r, d):
    """gen_world.crater_at: a depression with a raised rim, carved into the heightfield."""
    q2 = ((X - x) ** 2 + (Z - z) ** 2) / (r * r)
    out = np.zeros(q2.shape)
    m = q2 < 9.0
    q = q2[m]
    out[m] = -d * np.exp(-q * 1.8) + d * 0.35 * np.exp(-(np.sqrt(q) - 1.4) ** 2 * 2.2)
    return out


def choose_ridge(L):
    """R1 §10 / §6.3: at least one street closed by a facade that fell outward. The tallest razed
    block facing an ordinary side street, away from the tram street, the old road and the mission
    points, gives the ridge; its facade chunks become collidable wall stubs across the street."""
    bmap = {b["id"]: b for b in L.blocks}
    keep_open = [L.src_spawn, L.rust["wreck"], L.rust["withdraw"]]
    cands = []
    for b in L.buildings:
        if not b["razed"] or b["kind"] != "street":
            continue
        if b["typology"] != "apartment_block" or b["storeys"] < 4:
            continue   # R1 §6.3: the street-closing ridges come from 4-5-storey facades
        st = bmap[b["block"]]["sides"][b["side"]]
        if st is None or st["cls"] != "ordinary":
            continue
        cx, cz = (b["rect"][0] + b["rect"][1]) / 2, (b["rect"][2] + b["rect"][3]) / 2
        if min(math.hypot(cx - p[0], cz - p[1]) for p in keep_open) < 70.0:
            continue
        if seg_dist(cx, cz, L.diag["a"], L.diag["b"]) < 40.0:
            continue
        if L.square and rect_gap(ridge_rect(dict(building=b, street=st)), L.square["rect"]) < 25.0:
            continue   # a closure beside the open square would be no closure at all
        cands.append((-(b["storeys"] * b["width_m"]), -b["intensity"], b["id"], b, st))
    if not cands:
        return None
    cands.sort(key=lambda t: t[:3])
    _, _, _, b, st = cands[0]
    return dict(building=b, street=st)


RIDGE_SHARE_OF_RUBBLE = 0.65   # the facade and the floors it pulled with it fell outward [I]


def ridge_field(X, Z, ridge, crest=1.0):
    """Height added by the fallen facade: highest against the plot, thinning across the street.
    Returned for a unit crest unless `crest` is given; compose_terrain scales it by mass balance."""
    b = ridge["building"]
    st = ridge["street"]
    x0, x1, z0, z1 = b["rect"]
    side = b["side"]
    W = st["w"]
    if side in ("e", "w"):
        out = 1.0 if side == "e" else -1.0
        line = x1 if side == "e" else x0
        t = (X - line) * out                       # 0 at the facade line, W at the far building line
        s_lo, s_hi = z0, z1
        s = Z
    else:
        out = 1.0 if side == "s" else -1.0
        line = z1 if side == "s" else z0
        t = (Z - line) * out
        s_lo, s_hi = x0, x1
        s = X
    across = np.where(t < 0, 0.0, np.where(t <= W, 1.0 - 0.6 * t / W,
                                            0.4 * np.clip(1.0 - (t - W) / 3.0, 0.0, 1.0)))
    taper = 5.0
    along = np.clip(np.minimum(s - s_lo, s_hi - s) / taper, 0.0, 1.0)
    along = np.sin(along * math.pi / 2) ** 2
    return crest * across * along


def place_craters(L):
    """Craters of 5-9 m diameter (the resolvable part of R1 §6.4's population) in open ground: the
    square, yards, razed plots, side streets and the open ground around the district."""
    ext, ring = L.extent, L.ring
    target_density = pv("crater.density_per_km2_resolved")
    out = []
    step = 12.0
    x = ring[0] + 6.0
    ix = 0
    while x < min(ring[1], L.grid.world_half - 20.0):
        z = ring[2] + 6.0
        iz = 0
        while z < ring[3]:
            key = f"crater.{ix}.{iz}"
            inside = ext["x0"] <= x <= ext["x1"] and ext["z0"] <= z <= ext["z1"]
            p_accept = target_density * step * step / 1e6
            if rnd(key, 1) < p_accept:
                cx = x + (rnd(key, 2) - 0.5) * step
                cz = z + (rnd(key, 3) - 0.5) * step
                r = 2.5 + 2.0 * rnd(key, 4)
                d = 0.5 + 0.6 * rnd(key, 5)
                if crater_ok(L, cx, cz, r):
                    out.append(dict(x=r3(cx), z=r3(cz), r=r3(r), d=r3(d), inside=inside))
            z += step
            iz += 1
        x += step
        ix += 1
    return out


def crater_ok(L, x, z, r):
    reach = 3.0 * r
    for b in L.buildings:
        if rect_dist(b["rect"], x, z) < reach + (0.0 if b["razed"] else L.grid.sp):
            return False
    rk = L.courtyard_frozen
    if rect_dist(rk, x, z) < reach + 2.0:
        return False
    if math.hypot(x - L.src_spawn[0], z - L.src_spawn[1]) < 30.0 + reach:
        return False
    for p in (L.rust["wreck"], L.rust["withdraw"]):
        if math.hypot(x - p[0], z - p[1]) < 20.0 + reach:
            return False
    main = [s for s in L.streets if s["cls"] == "main"][0]["rect"]
    if rect_dist(main, x, z) < reach:
        return False
    if seg_dist(x, z, L.diag["a"], L.diag["full_end"]) < L.diag["w"] / 2 + reach:
        return False
    for route in L.rust["routes"]:
        for a, b in zip(route, route[1:] + route[:1]):
            if seg_dist(x, z, a, b) < 25.0 + reach:
                return False
    for g in L.rust["at_gun_sites"]:
        if math.hypot(x - g[0], z - g[1]) < 25.0 + reach:
            return False
    for (ox, oy, oz, stem, origin) in L.objects:
        if math.hypot(x - ox, z - oz) < reach + 2.0:
            return False
    if L.ridge and rect_dist(ridge_rect(L.ridge), x, z) < reach + 4.0:
        return False
    for site in L.map_sites:
        if math.hypot(x - site["x"], z - site["z"]) < 15.0 + reach:
            return False
    return True


def ridge_rect(ridge):
    b = ridge["building"]
    W = ridge["street"]["w"]
    x0, x1, z0, z1 = b["rect"]
    return {"e": (x1, x1 + W + 3, z0, z1), "w": (x0 - W - 3, x0, z0, z1),
            "s": (x0, x1, z1, z1 + W + 3), "n": (x0, x1, z0 - W - 3, z0)}[b["side"]]


def compose_terrain(L, src):
    """Pads (exact inside footprint + one sample cell), 8 m aprons, razed-plot mounds, the ridge and
    craters on the courtyard terrain. Every other sample keeps its source height; the courtyard's
    pad and apron are left exactly as prepared."""
    grid = L.grid
    H0 = src.heights.astype(np.float64)
    wx0, wx1 = L.ring[0] - 30.0, min(L.ring[1] + 30.0, grid.world_half)
    wz0, wz1 = L.ring[2] - 30.0, L.ring[3] + 30.0
    rs, cs = grid.window(wx0, wx1, wz0, wz1)
    X = np.broadcast_to(grid.axis[cs][None, :], (rs.stop - rs.start, cs.stop - cs.start))
    Z = np.broadcast_to(grid.axis[rs][:, None], X.shape)
    base = H0[rs, cs].copy()
    add = np.zeros_like(base)
    cell_area = grid.sp * grid.sp
    L.mound_records = []
    ridge_volume = 0.0
    if L.ridge:
        unit = ridge_field(X, Z, L.ridge, 1.0)
        rb = L.ridge["building"]
        vpm2, _, _ = mound_model(rb)
        v_total = vpm2 * rb["width_m"] * rb["depth_m"]
        crest = min(pv("rubble.ridge_crest_m"), RIDGE_SHARE_OF_RUBBLE * v_total / max(float(unit.sum() * cell_area), 1e-6))
        rf = unit * crest
        add += rf
        ridge_volume = float(rf.sum() * cell_area)
        L.ridge["crest_m"] = r3(crest)
        L.ridge["rubble_volume_m3"] = r3(v_total)
    for b in L.buildings:
        if not b["razed"]:
            continue
        vpm2, crest, apron = mound_model(b)
        area = b["width_m"] * b["depth_m"]
        volume = vpm2 * area
        plot_volume = volume
        if L.ridge and L.ridge["building"]["id"] == b["id"]:
            plot_volume = max(volume - ridge_volume, 0.25 * volume)
        field, rec = dome(X, Z, b["rect"], apron, crest, plot_volume)
        add += field
        grid_volume = float(field.sum() * cell_area)
        rec.update(id=b["id"], typology=b["typology"], storeys=b["storeys"], rubble_volume_m3=r3(volume),
                   plot_volume_m3=r3(plot_volume), heightfield_volume_m3=r3(grid_volume),
                   heightfield_volume_ratio=r3(grid_volume / max(plot_volume, 1e-6)))
        L.mound_records.append(rec)
    if L.ridge:
        L.ridge["volume_m3"] = r3(ridge_volume)
    # streets, the square and the old road are graded to the smoothed street surface (a city street
    # is at grade: the old road's 1.5-3 m cut is gen_world's road grading, not a street), fading in
    # over 3 m at their sides and back to the open ground at the district edge
    G = L.G[rs, cs]
    w_st = np.zeros_like(base)
    rects = [st["rect"] for st in L.streets] + ([L.square["rect"]] if L.square else [])
    for x0, x1, z0, z1 in rects:
        inside = np.minimum(np.minimum(X - x0, x1 - X), np.minimum(Z - z0, z1 - Z))
        w_st = np.maximum(w_st, smoothstep(inside / STREET_SIDE_FEATHER_M))
    a, b = np.array(L.diag["a"]), np.array(L.diag["b"])
    ab = b - a
    t = np.clip(((X - a[0]) * ab[0] + (Z - a[1]) * ab[1]) / float(ab @ ab), 0.0, 1.0)
    dist = np.hypot(X - (a[0] + t * ab[0]), Z - (a[1] + t * ab[1]))
    w_st = np.maximum(w_st, smoothstep((L.diag["w"] / 2 + 2.0 - dist) / STREET_SIDE_FEATHER_M))
    ext = L.extent
    edge_in = np.minimum(np.minimum(X - ext["x0"], ext["x1"] - X), np.minimum(Z - ext["z0"], ext["z1"] - Z))
    w_st *= smoothstep(edge_in / STREET_EDGE_FEATHER_M)
    for c in L.craters:
        add += crater_profile(X, Z, c["x"], c["z"], c["r"], c["d"])
    target = base + w_st * (G - base) + add
    cell = grid.sp + 1e-6
    skirt_sum = np.zeros_like(base)
    skirt_cnt = np.zeros_like(base)
    w_sum = np.zeros_like(base)
    p_sum = np.zeros_like(base)
    apron_d = np.full_like(base, 1e9)
    for b in L.buildings:
        if b["razed"]:
            continue
        x0, x1, z0, z1 = b["rect"]
        dx = np.maximum(np.maximum(x0 - X, X - x1), 0.0)
        dz = np.maximum(np.maximum(z0 - Z, Z - z1), 0.0)
        dinf = np.maximum(dx, dz)
        sk = dinf <= cell
        skirt_sum[sk] += b["pad"]
        skirt_cnt[sk] += 1.0
        ap = (~sk) & (dinf < cell + APRON_M)
        w = 1.0 / np.maximum(dinf[ap] - cell, 0.05) ** 2
        w_sum[ap] += w
        p_sum[ap] += w * b["pad"]
        apron_d[ap] = np.minimum(apron_d[ap], dinf[ap])
    # aprons: an inverse-distance blend of the pads in reach (continuous, equal to a pad at its
    # skirt), relaxed to the ground by the distance to the nearest skirt
    final = target.copy()
    has_ap = w_sum > 0
    t = smoothstep((apron_d - cell) / APRON_M)
    blend = p_sum[has_ap] / w_sum[has_ap]
    final[has_ap] = blend * (1.0 - t[has_ap]) + target[has_ap] * t[has_ap]
    sk = skirt_cnt > 0
    final[sk] = skirt_sum[sk] / skirt_cnt[sk]
    kx0, kx1, kz0, kz1 = L.courtyard_frozen
    keep = (X >= kx0) & (X <= kx1) & (Z >= kz0) & (Z <= kz1)
    final[keep] = base[keep]
    H = src.heights.copy()
    H[rs, cs] = final.astype(np.float32)
    delta = final - base
    L.terrain_stats = dict(
        window_xz=[r3(wx0), r3(wx1), r3(wz0), r3(wz1)],
        changed_samples=int(np.count_nonzero(H != src.heights)),
        max_raise_m=r3(delta.max()), max_cut_m=r3(-delta.min()),
        skirt_samples=int(sk.sum()), apron_samples=int((has_ap & ~sk).sum()),
        shared_skirt_samples=int((skirt_cnt > 1).sum()),
        craters=len(L.craters), razed_mounds=len(L.mound_records),
        ridge_volume_m3=r3(ridge_volume))
    return H


def pad_flatness(L, H):
    """Max |terrain - pad| over each standing footprint, sampled bilinearly every 0.5 m."""
    worst = []
    for b in L.buildings:
        if b["razed"]:
            continue
        x0, x1, z0, z1 = b["rect"]
        xs = np.arange(x0, x1 + 1e-6, 0.5)
        zs = np.arange(z0, z1 + 1e-6, 0.5)
        err = 0.0
        for z in zs:
            for x in xs:
                err = max(err, abs(L.grid.sample(H, x, z) - b["pad"]))
        worst.append((err, b["id"]))
    return worst


# ════════════════════════════════════════════════════════════════════════════════════════════════
# dressing ops
# ════════════════════════════════════════════════════════════════════════════════════════════════
OP_BUDGET = 6800   # new add ops (the courtyard workshop already carries 873); 4200 before the density pass


def faces(rect):
    """(name, start point, tangent, outward normal, length, yaw aligning local X with the face)."""
    x0, x1, z0, z1 = rect
    return [("n", (x0, z0), (1.0, 0.0), (0.0, -1.0), x1 - x0, 0.0),
            ("s", (x0, z1), (1.0, 0.0), (0.0, 1.0), x1 - x0, 0.0),
            ("w", (x0, z0), (0.0, 1.0), (-1.0, 0.0), z1 - z0, 90.0),
            ("e", (x1, z0), (0.0, 1.0), (1.0, 0.0), z1 - z0, 90.0)]


class Dresser:
    def __init__(self, L, H, src):
        self.L = L
        self.H = H
        self.grid = L.grid
        self.ops = []
        self.meta = []
        self.counts = {}
        self.hard = []
        self.standing = [b["rect"] for b in L.buildings if not b["razed"]]
        self.bins = {}
        for r in self.standing:
            for cx in range(int(math.floor(r[0] / 32)), int(math.floor(r[1] / 32)) + 1):
                for cz in range(int(math.floor(r[2] / 32)), int(math.floor(r[3] / 32)) + 1):
                    self.bins.setdefault((cx, cz), []).append(r)
        self.objects = L.objects
        self.obj_bins = {}
        for (x, y, z, stem, origin) in L.objects:
            self.obj_bins.setdefault((int(math.floor(x / 16)), int(math.floor(z / 16))), []).append((x, z))
        self.src = src

    def ground(self, x, z):
        return self.grid.sample(self.H, x, z)

    def hard_ok(self, x, z, pad=1.5):
        """No hard object may stand in the old road (the Act 1 breakout spine stays open)."""
        L = self.L
        return seg_dist(x, z, L.diag["a"], L.diag["full_end"]) >= L.diag["w"] / 2 + pad

    def in_standing(self, x, z, margin=0.0):
        for r in self.bins.get((int(math.floor(x / 32)), int(math.floor(z / 32))), ()):
            if r[0] - margin <= x <= r[1] + margin and r[2] - margin <= z <= r[3] + margin:
                return True
        return False

    def near_object(self, x, z, r):
        cx, cz = int(math.floor(x / 16)), int(math.floor(z / 16))
        for i in (-1, 0, 1):
            for j in (-1, 0, 1):
                for (ox, oz) in self.obj_bins.get((cx + i, cz + j), ()):
                    if (ox - x) ** 2 + (oz - z) ** 2 < r * r:
                        return True
        return False

    def blocked(self, x, z, margin=0.3, obj_r=1.2, spawn_r=9.0):
        L = self.L
        if self.in_standing(x, z, margin):
            return True
        k = L.courtyard_keep
        if k[0] - 1.0 <= x <= k[1] + 1.0 and k[2] - 1.0 <= z <= k[3] + 1.0:
            return True
        if math.hypot(x - L.src_spawn[0], z - L.src_spawn[1]) < spawn_r:
            return True
        if obj_r > 0 and self.near_object(x, z, obj_r):
            return True
        return False

    def add(self, cls, key, x, z, deg=0.0, s=(1.0, 1.0, 1.0), y=None, dy=0.0, hard_extent=None):
        if y is None:
            y = self.ground(x, z) + dy
        self.ops.append({"asset": ASSET[key], "deg": round(deg % 360.0, 2), "op": "add",
                         "s": [r3(v) for v in s], "x": r3(x), "y": r3(y), "z": r3(z)})
        self.meta.append((cls, key))
        self.counts[cls] = self.counts.get(cls, 0) + 1
        if key in HARD_ASSETS:
            self.hard.append(dict(cls=cls, key=key, x=r3(x), z=r3(z), deg=round(deg % 360.0, 2), s=[r3(v) for v in s],
                                  extent=hard_extent))


def dress_ruins(D):
    """P G17 / L7: low debris at the foot of every damaged masonry shell, densest where it
    partly collapsed ([04-09] [04-11] [07-16] [04-04] [04-12] [04-01]). The tall rubble cones and
    berms come from the building's own failed modules in the runtime (G17: rubble from failed
    modules); this dressing stays below 0.35 m and is crushable."""
    steps = {"burned": 12.0, "shelled": 8.0, "partly_collapsed": 5.0}
    for b in D.L.buildings:
        if b["razed"] or b["typology"] == "wooden_house" or b["condition"] == "intact":
            continue
        step = steps[b["condition"]]
        for (fname, p0, tg, nrm, length, fyaw) in faces(b["rect"]):
            n = int(length // step)
            for k in range(n):
                key = f"{b['id']}.{fname}.{k}"
                t = (k + 0.5 + (rnd(key, 1) - 0.5) * 0.6) * step
                off = 0.8 + 1.8 * rnd(key, 2)
                x = p0[0] + tg[0] * t + nrm[0] * off
                z = p0[1] + tg[1] * t + nrm[1] * off
                if D.blocked(x, z):
                    continue
                u = rnd(key, 3)
                D.add("ruin_debris", "debris" if rnd(key, 4) < 0.5 else "debris2", x, z,
                      fyaw + (rnd(key, 5) - 0.5) * 50.0, (1.0 + 0.7 * u, 1.2 + 2.0 * rnd(key, 6), 1.0 + 0.7 * u), dy=-0.02)
                if rnd(key, 7) < 0.45:
                    bx, bz = x + (rnd(key, 8) - 0.5) * 2.4, z + (rnd(key, 9) - 0.5) * 2.4
                    if not D.blocked(bx, bz):
                        D.add("ruin_debris", "brick_stack", bx, bz, rnd(key, 10) * 360.0, (1.0 + 0.3 * rnd(key, 11),) * 3)
                if rnd(key, 12) < (0.5 if b["condition"] == "partly_collapsed" else 0.28):
                    gx, gz = x + nrm[0] * 0.6, z + nrm[1] * 0.6
                    if not D.blocked(gx, gz):
                        gs = 0.9 + 0.5 * rnd(key, 13)
                        D.add("ruin_debris", "gravel" if rnd(key, 14) < 0.5 else "gravel2", gx, gz, rnd(key, 15) * 360.0,
                              (gs, 0.7 + 0.4 * rnd(key, 16), gs), dy=-0.03)
                if b["condition"] == "burned" and rnd(key, 17) < 0.22:
                    jx, jz = x + nrm[0] * 0.5, z + nrm[1] * 0.5
                    if not D.blocked(jx, jz):
                        D.add("ruin_timber", "joist", jx, jz, fyaw + (rnd(key, 18) - 0.5) * 30.0,
                              (0.3 + 0.2 * rnd(key, 19), 0.7, 0.8), dy=-0.05)


def dress_wooden(D):
    """G13 / L2: a burned timber house leaves its stove and chimney (the grammar's); around the
    plinth lie charred boards and joists ([04-27] [04-07] [04-23])."""
    for b in D.L.buildings:
        if b["razed"] or b["typology"] != "wooden_house":
            continue
        key = b["id"] + ".ash"
        n = 1 + int(rnd(key, 1) * 2)
        for k in range(n):
            fname, p0, tg, nrm, length, fyaw = faces(b["rect"])[int(rnd(key, 10 + k) * 4) % 4]
            t = length * (0.2 + 0.6 * rnd(key, 20 + k))
            off = 0.6 + 1.6 * rnd(key, 30 + k)
            x, z = p0[0] + tg[0] * t + nrm[0] * off, p0[1] + tg[1] * t + nrm[1] * off
            if D.blocked(x, z):
                continue
            if b["condition"] in ("burned", "partly_collapsed"):
                D.add("timber_ash", "house_joist" if rnd(key, 40 + k) < 0.5 else "joist", x, z,
                      fyaw + (rnd(key, 50 + k) - 0.5) * 70.0, (0.25 + 0.15 * rnd(key, 60 + k), 0.6, 0.7), dy=-0.05)
                if rnd(key, 70 + k) < 0.6:
                    D.add("timber_ash", "ash_bricks", x + nrm[0] * 0.8, z + nrm[1] * 0.8, rnd(key, 80 + k) * 360.0,
                          (0.7 + 0.4 * rnd(key, 90 + k), 1.0, 0.7 + 0.4 * rnd(key, 95 + k)), dy=-0.02)
            else:
                D.add("timber_ash", "plank", x, z, rnd(key, 50 + k) * 360.0, (1.0, 1.0, 1.0))


def dress_razed(D):
    """R1 §6.2 razed plots: a snow-rounded rubble mound (the terrain) with loose brick, a bonded wall
    stub about 1 m high and burned floor timbers on top ([04-01] [04-02] [04-12])."""
    for b in D.L.buildings:
        if not b["razed"]:
            continue
        x0, x1, z0, z1 = b["rect"]
        key = b["id"] + ".razed"
        gx = np.arange(x0 + 2.0, x1 - 1.0, 4.5)
        gz = np.arange(z0 + 2.0, z1 - 1.0, 4.5)
        k = 0
        for x in gx:
            for z in gz:
                k += 1
                if rnd(key, 100 + k) > 0.55:
                    continue
                px = x + (rnd(key, 200 + k) - 0.5) * 2.0
                pz = z + (rnd(key, 300 + k) - 0.5) * 2.0
                if D.blocked(px, pz, margin=0.2):
                    continue
                u = rnd(key, 400 + k)
                D.add("razed_rubble", "debris" if u < 0.5 else "debris2", px, pz, rnd(key, 500 + k) * 360.0,
                      (1.3 + 0.6 * u, 2.0 + 2.0 * rnd(key, 600 + k), 1.3 + 0.6 * rnd(key, 700 + k)), dy=-0.05)
        for j in range(2 + int(rnd(key, 1) * 4)):
            px = x0 + (x1 - x0) * rnd(key, 10 + j)
            pz = z0 + (z1 - z0) * rnd(key, 20 + j)
            if not D.blocked(px, pz, margin=0.2):
                D.add("razed_rubble", "brick_stack", px, pz, rnd(key, 30 + j) * 360.0, (1.1,) * 3)
        for j in range(1 + int(rnd(key, 2) * 2)):
            px = x0 + (x1 - x0) * (0.2 + 0.6 * rnd(key, 40 + j))
            pz = z0 + (z1 - z0) * (0.2 + 0.6 * rnd(key, 50 + j))
            if not D.blocked(px, pz, margin=0.2):
                D.add("razed_timber", "joist", px, pz, rnd(key, 60 + j) * 360.0, (0.35 + 0.2 * rnd(key, 70 + j), 0.7, 0.8),
                      dy=-0.1)
        # a bonded stub of the street wall, 0.75-1.35 m, standing on its line
        fname, p0, tg, nrm, length, fyaw = [f for f in faces(b["rect"]) if f[0] == b["side"]][0]
        for j in range(1 + int(rnd(key, 3) * 2)):
            t = length * (0.15 + 0.7 * rnd(key, 80 + j))
            x, z = p0[0] + tg[0] * t - nrm[0] * 0.3, p0[1] + tg[1] * t - nrm[1] * 0.3
            if D.blocked(x, z, margin=0.2):
                continue
            sx = 0.5 + 0.4 * rnd(key, 90 + j)
            sy = 0.25 + 0.2 * rnd(key, 95 + j)
            D.add("razed_wall_stub", "yard_wall", x, z, fyaw, (sx, sy, 1.0), dy=-0.25,
                  hard_extent=[r3(4.0 * sx), r3(3.0 * sy), 0.4])
        # density pass: the bonded stub of one party wall returning from the street line into the plot
        # (P L1 [04-01] [04-12]: flattened areas with low wall fragments), and loose brick heaps on it
        if rnd(key, 110) < 0.6:
            end = [f for f in faces(b["rect"]) if f[0] in (("w", "e") if b["side"] in ("n", "s") else ("n", "s"))]
            ename, e0, etg, enrm, elen, eyaw = end[int(rnd(key, 111) * 2) % 2]
            # near the street end of the side face
            t = elen * (0.2 + 0.25 * rnd(key, 112))
            t = t if (b["side"] in ("n", "w")) else elen - t
            x, z = e0[0] + etg[0] * t - enrm[0] * 0.3, e0[1] + etg[1] * t - enrm[1] * 0.3
            if not D.blocked(x, z, margin=0.2):
                sx = 0.45 + 0.35 * rnd(key, 113)
                sy = 0.25 + 0.2 * rnd(key, 114)
                D.add("razed_wall_stub", "yard_wall", x, z, eyaw, (sx, sy, 1.0), dy=-0.25,
                      hard_extent=[r3(4.0 * sx), r3(3.0 * sy), 0.4])
        for j in range(2):
            px = x0 + (x1 - x0) * (0.15 + 0.7 * rnd(key, 120 + j))
            pz = z0 + (z1 - z0) * (0.15 + 0.7 * rnd(key, 130 + j))
            if not D.blocked(px, pz, margin=0.2):
                gs = 1.2 + 0.6 * rnd(key, 140 + j)
                D.add("razed_rubble", "gravel" if j == 0 else "gravel2", px, pz, rnd(key, 150 + j) * 360.0,
                      (gs, 0.8 + 0.5 * rnd(key, 160 + j), gs), dy=-0.03)


def dress_ridge(D):
    """R1 §6.3 / §8.5 / §10: the facade that fell across the street. The terrain ridge carries it;
    bonded facade chunks (collidable brick) lie across the whole carriageway, so a tank cannot pass
    (S5 pp. 205-206, 211: rubble "which no tank could penetrate")."""
    ridge = D.L.ridge
    if not ridge:
        return
    b = ridge["building"]
    st = ridge["street"]
    x0, x1, z0, z1 = b["rect"]
    W = st["w"]
    side = b["side"]
    key = b["id"] + ".ridge"
    if side in ("e", "w"):
        out = 1.0 if side == "e" else -1.0
        line = x1 if side == "e" else x0
        s_mid = (z0 + z1) / 2
        rows = [s_mid - 3.0, s_mid + 4.0]
        for r_i, s in enumerate(rows):
            for m, t in enumerate(np.arange(2.0, W, 4.0)):
                x = line + out * (t + (0.5 if r_i else 0.0))
                z = s + (rnd(key, 10 * r_i + m) - 0.5) * 1.2
                sy = 0.38 + 0.16 * rnd(key, 50 + 10 * r_i + m)
                D.add("ridge_chunks", "yard_wall", x, z, (rnd(key, 80 + 10 * r_i + m) - 0.5) * 24.0,
                      (1.0, sy, 1.0), dy=-0.3, hard_extent=[4.0, r3(3.0 * sy), 0.4])
        span = (z0 + 5.0, z1 - 5.0)
        for m in range(26):
            t = W * rnd(key, 200 + m)
            s = span[0] + (span[1] - span[0]) * rnd(key, 300 + m)
            x, z = line + out * t, s
            D.add("ridge_rubble", "debris" if m % 2 else "debris2", x, z, rnd(key, 400 + m) * 360.0,
                  (1.4 + 0.5 * rnd(key, 500 + m), 2.5 + 1.5 * rnd(key, 600 + m), 1.4 + 0.5 * rnd(key, 700 + m)), dy=-0.05)
        for m in range(4):
            x = line + out * W * (0.2 + 0.6 * rnd(key, 800 + m))
            z = span[0] + (span[1] - span[0]) * rnd(key, 900 + m)
            D.add("ridge_rubble", "joist", x, z, rnd(key, 950 + m) * 360.0, (0.4, 0.7, 0.8), dy=-0.1)
    else:
        out = 1.0 if side == "s" else -1.0
        line = z1 if side == "s" else z0
        s_mid = (x0 + x1) / 2
        for r_i, s in enumerate([s_mid - 3.0, s_mid + 4.0]):
            for m, t in enumerate(np.arange(2.0, W, 4.0)):
                z = line + out * (t + (0.5 if r_i else 0.0))
                x = s + (rnd(key, 10 * r_i + m) - 0.5) * 1.2
                sy = 0.38 + 0.16 * rnd(key, 50 + 10 * r_i + m)
                D.add("ridge_chunks", "yard_wall", x, z, 90.0 + (rnd(key, 80 + 10 * r_i + m) - 0.5) * 24.0,
                      (1.0, sy, 1.0), dy=-0.3, hard_extent=[4.0, r3(3.0 * sy), 0.4])


def dress_gaps(D):
    """Gaps between street buildings: brick yard walls with gates and gate piers in the brick
    streets (R1 §5.5: S10o, S10g; P L12 [04-09] [07-01]); board fences with boards missing in the
    timber fringe (R1 §5.5; P L5 [03-04] [04-05])."""
    for g in D.L.gaps:
        a, b = g["along"]
        side = g["side"]
        key = f"gap.{g['block']}.{side}.{int(a)}"
        horiz = side in ("n", "s")
        yaw = 0.0 if horiz else 90.0
        inward = {"n": 1.0, "s": -1.0, "w": 1.0, "e": -1.0}[side]
        line = g["rect"][2] if side == "n" else g["rect"][3] if side == "s" else g["rect"][0] if side == "w" else g["rect"][1]

        def at(t, off):
            if horiz:
                return a + t, line + inward * off
            return line + inward * off, a + t

        n = int(round((b - a) / 4.0))
        if g["kind"] in ("wall", "gate"):
            gate_idx = 0 if g["kind"] == "gate" else int(rnd(key, 1) * n)
            for m in range(n):
                cx, cz = at(4.0 * m + 2.0, 0.2)
                if m == gate_idx:
                    for end in (0.35, 3.65):
                        px, pz = at(4.0 * m + end, 0.25)
                        if not D.blocked(px, pz, margin=0.0, obj_r=0.8) and D.hard_ok(px, pz):
                            D.add("gate_pier", "pole", px, pz, yaw, (0.9, 0.85, 0.9), dy=-0.1,
                                  hard_extent=[0.54, 2.55, 0.54])
                    continue
                if D.blocked(cx, cz, margin=0.0, obj_r=1.0) or not D.hard_ok(cx, cz, 3.5):
                    continue
                u = rnd(key, 10 + m)
                if u < 0.20:
                    continue                                              # breached / fallen
                sy = 0.3 + 0.2 * rnd(key, 30 + m) if u < 0.40 else 0.8   # shot down to a stub, or 2.4 m
                D.add("yard_wall", "yard_wall", cx, cz, yaw, (1.0, sy, 1.0), dy=-0.1,
                      hard_extent=[4.0, r3(3.0 * sy), 0.4])
        elif g["kind"] == "fence":
            for m in range(n):
                cx, cz = at(4.0 * m + 2.0, 0.1)
                if D.blocked(cx, cz, margin=0.0, obj_r=1.0):
                    continue
                u = rnd(key, 40 + m)
                if u < 0.35:
                    continue                                              # boards gone
                sy = pv("fence.low_h_m") / 2.03 if u < 0.65 else pv("fence.board_h_m") / 2.03
                D.add("fence", "fence_board", cx, cz, yaw + (180.0 if rnd(key, 60 + m) < 0.5 else 0.0),
                      (1.0, sy, 1.0), dy=-0.05)


def wire_span(D, cls, key, a, b, top_a, top_b, sag_scale=2.5, asset="wire"):
    """A level wire mesh stretched between two poles (the manifest transform has no pitch): it hangs
    at the lower pole's attachment and runs to the higher pole below its top, as from a bracket."""
    dx, dz = b[0] - a[0], b[1] - a[1]
    length = math.hypot(dx, dz)
    if length < 1.0:
        return
    if abs(top_b - top_a) > WIRE_MAX_DROP_M:
        D.wires_skipped_steep = getattr(D, "wires_skipped_steep", 0) + 1
        return   # a steep span cannot be a level mesh without cutting into the ground; left down
    deg = math.degrees(math.atan2(-dz, dx))
    y = min(top_a, top_b) + WIRE_TOP_OFFSET * sag_scale
    D.add(cls, asset, a[0] + dx / 2, a[1] + dz / 2, deg, (length / 4.0, sag_scale, 1.0), y=y)
    D.wire_drop_max = max(getattr(D, "wire_drop_max", 0.0), abs(top_b - top_a))


def pole_state(key, p_intact=0.62, p_stump=0.23):
    u = rnd(key, 1)
    return "intact" if u < p_intact else ("stump" if u < p_intact + p_stump else "fallen")


def put_pole(D, cls, key, x, z, height, toward, p_intact=0.62, p_stump=0.23):
    """A pole (the editor's reviewed utility pole), a broken stump, or one lying fallen."""
    state = pole_state(key, p_intact, p_stump)
    if not D.hard_ok(x, z):
        return "skipped", None
    g = D.ground(x, z)
    if state == "intact":
        D.add(cls, "pole", x, z, 0.0, (0.22, height / POLE_BASE_H, 0.22), y=g - 0.2, hard_extent=[0.132, r3(height), 0.132])
        return state, g - 0.2 + height
    if state == "stump":
        h = 1.2 + 1.4 * rnd(key, 2)
        D.add(cls, "pole", x, z, 0.0, (0.22, h / POLE_BASE_H, 0.22), y=g - 0.2, hard_extent=[0.132, r3(h), 0.132])
        return state, None
    ang = math.atan2(toward[1], toward[0]) + (rnd(key, 3) - 0.5) * 1.2
    deg = math.degrees(math.atan2(-math.sin(ang), math.cos(ang)))
    D.add(cls + "_fallen", "log", x, z, deg, (2.3, 0.18, 0.4), dy=-0.05)
    return state, None


def dress_utilities(D, src):
    """Tram poles and span wires on the tram street (R1 §5.4: span wires between side poles in the
    centre, S15; 30-40 m, 5.5-6 m inferred), telegraph poles and wires along the side streets and the
    old road (P L4 [04-14] [08-12] [06-01] [04-04] [08-13] [07-14]). The network was destroyed (S15;
    S3 p. 89: streets "blocked by fallen poles, wires, walls"): stumps, fallen poles, missing spans."""
    L = D.L
    main = [s for s in L.streets if s["cls"] == "main"][0]
    ew_rects = [s["rect"] for s in L.streets if s["axis"] == "ew"]
    xm, wm = main["c"], main["w"]
    pitch = pv("tram.pole_pitch_m")
    stations = []
    z = main["lo"] + 14.0
    k = 0
    while z < main["hi"] - 10.0:
        zz = z
        for r in ew_rects:
            if r[2] - 1.0 <= zz <= r[3] + 1.0:
                zz = r[3] + 2.0 if (zz - r[2]) > (r[3] - zz) else r[2] - 2.0
        stations.append((k, zz))
        z += pitch
        k += 1
    sides = (xm - wm / 2 + 1.0, xm + wm / 2 - 1.0)
    span_ok = []
    for k, zs in stations:
        tops = []
        for j, xs in enumerate(sides):
            key = f"tram.{k}.{j}"
            if D.blocked(xs, zs, margin=0.0, obj_r=1.5, spawn_r=9.0):
                tops.append(None)
                continue
            # the centre's span-wire poles were steel tubes (R1 §5.4, S15): fewer broken than timber
            state, top = put_pole(D, "tram_pole", key, xs, zs, 8.0, (xm - xs, 0.0), p_intact=0.78, p_stump=0.14)
            tops.append(top)
        ok = all(t is not None for t in tops)
        if ok:
            h = (tops[0] + tops[1]) / 2 - 8.0 + pv("tram.span_wire_h_m")
            wire_span(D, "tram_wire", f"tram.span.{k}", (sides[0], zs), (sides[1], zs), h, h)
        span_ok.append((zs, ok, (tops[0] + tops[1]) / 2 - 8.0 if ok else None))
    half = pv("tram.track_centres_m") / 2
    for i in range(len(span_ok) - 1):
        z0_, ok0, g0 = span_ok[i]
        z1_, ok1, g1 = span_ok[i + 1]
        if not (ok0 and ok1):
            continue
        for j, off in enumerate((-half, half)):
            key = f"tram.contact.{i}.{j}"
            if rnd(key, 1) < 0.3:
                continue
            h0, h1 = g0 + pv("tram.contact_wire_h_m"), g1 + pv("tram.contact_wire_h_m")
            wire_span(D, "tram_wire", key, (xm + off, z0_), (xm + off, z1_), h0, h1, sag_scale=3.0)
    # telegraph lines along the other streets (one side)
    tp = pv("pole.telegraph_pitch_m")
    for st in L.streets:
        if st["cls"] == "main":
            continue
        cross = [s["rect"] for s in L.streets if s["axis"] != st["axis"]]
        pos = st["c"] + st["w"] / 2 - 1.0
        along = st["lo"] + 12.0
        prev = None
        m = 0
        while along < st["hi"] - 8.0:
            x, z = (pos, along) if st["axis"] == "ns" else (along, pos)
            key = f"tele.{st['id']}.{m}"
            skip = any(r[0] - 1.0 <= x <= r[1] + 1.0 and r[2] - 1.0 <= z <= r[3] + 1.0 for r in cross)
            skip = skip or D.blocked(x, z, margin=0.0, obj_r=1.5, spawn_r=10.0)
            skip = skip or seg_dist(x, z, L.diag["a"], L.diag["b"]) < L.diag["w"] / 2 + 1.5
            skip = skip or (L.ridge is not None and rect_dist(ridge_rect(L.ridge), x, z) < 2.0)
            skip = skip or math.hypot(x - L.rust["withdraw"][0], z - L.rust["withdraw"][1]) < 8.0
            if skip:
                prev = None
            else:
                toward = (-1.0, 0.0) if st["axis"] == "ns" else (0.0, -1.0)
                state, top = put_pole(D, "telegraph_pole", key, x, z, pv("pole.height_m"), toward)
                if top is not None and prev is not None and rnd(key, 9) >= 0.3:
                    wire_span(D, "telegraph_wire", key + ".w", prev[0], (x, z), prev[1] - 0.15, top - 0.15,
                              sag_scale=2.0, asset="wire")
                prev = ((x, z), top) if top is not None else None
            along += tp
            m += 1
    # the old road's existing act-1 poles get their wires back (base pillar_hr_2 poles, s=[0.22,2.55,0.22])
    assets = src.manifest["assets"]
    poles = []
    for i, p in enumerate(src.manifest["placements"]):
        if Path(assets[p[0]]).stem != "pillar_hr_2" or not isinstance(p[5], list):
            continue
        if abs(p[5][1] - 2.55) > 1e-3 or abs(p[5][0] - 0.22) > 1e-3:
            continue
        x, z = float(p[1]), float(p[3])
        if not (L.ring[0] <= x <= L.ring[1] and L.ring[2] <= z <= L.ring[3]):
            continue
        if seg_dist(x, z, L.diag["a"], L.diag["full_end"]) > 12.0:
            continue
        poles.append((float((x - L.diag["a"][0]) * L.diag["dir"][0] + (z - L.diag["a"][1]) * L.diag["dir"][1]),
                      x, z, float(p[2]) + 3.0 * 2.55, i))
    poles.sort()
    for (t0, xa, za, ta, ia), (t1, xb, zb, tb, ib) in zip(poles, poles[1:]):
        if t1 - t0 > 45.0 or rnd(f"oldroad.{ia}", 1) < 0.3:
            continue
        wire_span(D, "telegraph_wire", f"oldroad.{ia}", (xa, za), (xb, zb), ta - 0.2, tb - 0.2, sag_scale=2.0,
                  asset="wire")
    D.old_road_poles = len(poles)


def dress_trees(D):
    """The square's garden: trees stripped to trunks and main limbs, 10-15 m, some broken to
    stumps, some fallen (P L6 [04-10] [07-14] [04-02] [04-01]; R1 §6.4 S5 p. 464 "half-shattered
    trees"). Fringe gardens keep a stripped fruit tree or a stump ([04-23])."""
    L = D.L
    if L.square:
        x0, x1, z0, z1 = L.square["rect"]
        inset = 5.0
        pts = []
        for x in np.arange(x0 + inset, x1 - inset + 1e-6, 9.0):
            pts += [(x, z0 + inset), (x, z1 - inset)]
        for z in np.arange(z0 + inset + 9.0, z1 - inset - 1e-6, 9.0):
            pts += [(x0 + inset, z), (x1 - inset, z)]
        for i, (x, z) in enumerate(sorted(pts)):
            key = f"square.tree.{i}"
            if D.blocked(x, z, margin=0.0, obj_r=2.0, spawn_r=12.0):
                continue
            if seg_dist(x, z, L.diag["a"], L.diag["b"]) < L.diag["w"] / 2 + 2.0:
                continue
            u = rnd(key, 1)
            if u < 0.45:
                if rnd(key, 2) < 0.7:
                    D.add("square_tree", "tree_tall", x, z, rnd(key, 3) * 360.0, (1.3 + 0.3 * rnd(key, 4),) * 3)
                else:
                    D.add("square_tree", "tree_small", x, z, rnd(key, 3) * 360.0, (1.8 + 0.7 * rnd(key, 4),) * 3)
            elif u < 0.70:
                D.add("square_stump", "stump", x, z, rnd(key, 3) * 360.0, (1.2 + 0.4 * rnd(key, 4),) * 3)
            elif u < 0.85:
                D.add("square_stump", "log", x, z, rnd(key, 3) * 360.0, (1.6, 0.8, 0.9), dy=-0.05)
    for b in L.buildings:
        if b["typology"] != "wooden_house" or b["razed"]:
            continue
        key = b["id"] + ".garden"
        if rnd(key, 1) > 0.33:
            continue
        fname, p0, tg, nrm, length, fyaw = [f for f in faces(b["rect"]) if f[0] == b["side"]][0]
        back = (-nrm[0], -nrm[1])
        depth = b["depth_m"]
        x = p0[0] + tg[0] * length * 0.5 + back[0] * (depth + 5.0 + 4.0 * rnd(key, 2))
        z = p0[1] + tg[1] * length * 0.5 + back[1] * (depth + 5.0 + 4.0 * rnd(key, 2))
        if D.blocked(x, z, margin=0.5, obj_r=1.5):
            continue
        if rnd(key, 3) < 0.6:
            D.add("garden_tree", "tree_small", x, z, rnd(key, 4) * 360.0, (1.2 + 0.4 * rnd(key, 5),) * 3)
        else:
            D.add("garden_tree", "stump", x, z, rnd(key, 4) * 360.0, (1.0,) * 3)


def free_spot(D, cands, clear_m):
    for (x, z) in cands:
        if D.blocked(x, z, margin=clear_m, obj_r=clear_m + 3.0, spawn_r=25.0) or not D.hard_ok(x, z, 6.0):
            continue
        return x, z
    return None


def dress_wrecks(D):
    """Knocked-out tanks in a square and along one side of a street, each in a halo of dropped kit
    and planks 5-15 m across (P L11 [08-12] [08-13] [06-08] [01-25] [05-01]; R1 §6.4 S3 p. 393)."""
    L = D.L
    D.wrecks = []
    plans = []
    if L.square:
        x0, x1, z0, z1 = L.square["rect"]
        cands = []
        for i in range(40):
            key = f"wreck.square.{i}"
            x = x0 + 14.0 + (x1 - x0 - 28.0) * rnd(key, 1)
            z = z0 + 14.0 + (z1 - z0 - 28.0) * rnd(key, 2)
            if seg_dist(x, z, L.diag["a"], L.diag["b"]) < L.diag["w"] / 2 + 6.0:
                continue
            cands.append((x, z))
        spot = free_spot(D, cands, 4.0)
        if spot:
            plans.append(("wreck_kv1", spot, 60.0 + 50.0 * rnd("wreck.kv1", 3)))
    street_b = [s for s in L.streets if s["id"] == "st.b"]
    if street_b:
        st = street_b[0]
        e1 = [s for s in L.streets if s["id"] == "st.e1"]
        lo = (e1[0]["c"] + e1[0]["w"] / 2 + 12.0) if e1 else st["lo"] + 40.0
        cands = [(lo + 6.0 * i, st["c"] - st["w"] / 2 + 3.2) for i in range(12)]
        spot = free_spot(D, cands, 2.5)
        if spot:
            plans.append(("wreck_pz3", spot, (rnd("wreck.pz3", 3) - 0.5) * 16.0))
    for key, (x, z), deg in plans:
        D.add("wreck", key, x, z, deg, (1.0, 1.0, 1.0), dy=-0.25, hard_extent=[8.1, 3.3, 4.0])
        D.wrecks.append(dict(asset=key, x=r3(x), z=r3(z), deg=r3(deg)))
        for m in range(10):
            k = f"{key}.halo.{m}"
            a = rnd(k, 1) * math.tau
            r = 5.0 + 10.0 * rnd(k, 2)
            hx, hz = x + math.cos(a) * r, z + math.sin(a) * r
            if D.blocked(hx, hz, margin=0.2):
                continue
            item = ["board", "plank", "crate", "brick_stack", "joist", "plank"][m % 6]
            s = (0.35, 0.7, 0.8) if item == "joist" else (1.0, 1.0, 1.0)
            D.add("wreck_halo", item, hx, hz, rnd(k, 3) * 360.0, s, dy=-0.03)


def dress_drifts(D):
    """Snow drifted in the lee of walls and wrecks, scoured on the windward side (P V11 [05-13]
    [05-06]; L8). The lee is downwind of Act 1's westerly (src/world.rs weather_wind_profile)."""
    wx, wz = D.L.rust["wind"]
    lo_h, hi_h = pv("snow.drift_h_m")
    for b in D.L.buildings:
        if b["razed"] or (b["typology"] == "wooden_house" and b["condition"] != "intact"):
            continue
        for (fname, p0, tg, nrm, length, fyaw) in faces(b["rect"]):
            if nrm[0] * wx + nrm[1] * wz < 0.35:
                continue
            n = int(length // 7.0)
            for k in range(n):
                key = f"{b['id']}.drift.{fname}.{k}"
                t = (k + 0.5) * 7.0 + (rnd(key, 1) - 0.5) * 1.5
                off = 1.3 + 0.6 * rnd(key, 2)
                x, z = p0[0] + tg[0] * t + nrm[0] * off, p0[1] + tg[1] * t + nrm[1] * off
                if D.blocked(x, z, margin=0.2):
                    continue
                h = lo_h + (hi_h - lo_h) * rnd(key, 3)
                D.add("snow_drift", "snow_drift", x, z, fyaw + (rnd(key, 4) - 0.5) * 10.0,
                      (2.0 + 0.8 * rnd(key, 5), h / 0.13, 0.9 + 0.4 * rnd(key, 6)), dy=-0.04)
    for h_ in D.hard:
        if h_["cls"] not in ("yard_wall", "wreck"):
            continue
        key = f"drift.hard.{h_['x']}.{h_['z']}"
        if rnd(key, 1) < 0.4:
            continue
        x, z = h_["x"] + wx * 1.6, h_["z"] + wz * 1.6
        if D.blocked(x, z, margin=0.2):
            continue
        h = lo_h + (hi_h - lo_h) * rnd(key, 3)
        D.add("snow_drift", "snow_drift", x, z, h_["deg"], (1.6, h / 0.13, 0.9), dy=-0.04)


def dress_yards(D):
    """R1 §5.3: yards held sheds, stores and summer kitchens (S10o, S10o2, S1, S9); by March 1943 the
    yard buildings are gone (S28). Their remains: ash, charred joists, splinters behind the ranges."""
    for b in D.L.buildings:
        if b["kind"] != "street" or b["typology"] == "workshop":
            continue
        key = b["id"] + ".shed"
        if rnd(key, 1) > (0.5 if b["typology"] == "wooden_house" else 0.6):
            continue
        fname, p0, tg, nrm, length, fyaw = [f for f in faces(b["rect"]) if f[0] == b["side"]][0]
        back = (-nrm[0], -nrm[1])
        dist = b["depth_m"] + 4.0 + 8.0 * rnd(key, 2)
        t = length * (0.15 + 0.7 * rnd(key, 3))
        x = p0[0] + tg[0] * t + back[0] * dist
        z = p0[1] + tg[1] * t + back[1] * dist
        if D.blocked(x, z, margin=1.0, obj_r=1.5):
            continue
        D.add("shed_remains", "ash_bricks", x, z, rnd(key, 4) * 360.0, (0.9 + 0.4 * rnd(key, 5), 1.0, 0.9 + 0.4 * rnd(key, 6)),
              dy=-0.02)
        for j in range(1 + int(rnd(key, 7) * 2)):
            jx, jz = x + (rnd(key, 10 + j) - 0.5) * 3.0, z + (rnd(key, 20 + j) - 0.5) * 3.0
            if not D.blocked(jx, jz, margin=0.5):
                D.add("shed_remains", "house_joist" if j else "joist", jx, jz, rnd(key, 30 + j) * 360.0,
                      (0.25 + 0.15 * rnd(key, 40 + j), 0.6, 0.7), dy=-0.05)
        if rnd(key, 50) < 0.5:
            D.add("shed_remains", "splinter", x + 1.0, z - 0.8, rnd(key, 51) * 360.0, (1.0, 1.0, 1.0), dy=-0.02)


def in_tableau_keep(L, x, z):
    return any(k[0] <= x <= k[1] and k[2] <= z <= k[3] for _, k in getattr(L, "tableau_keepouts", ()))


def dress_yard_remains(D):
    """R1 §4 / §5.3: a plot held 1-4 sheds or stores, timber stores against its walls and board
    fences to its neighbours (S10o, S10o2, S9, S1); the timber burned before January 1943 (S28: by
    March the yard buildings are gone; P L12 [04-14]: sheds of timber posts with their roofs caved in).
    Behind every street building: its sheds' ash with charred planks and joists over it, now and then
    one board wall still standing, and the plot's boundary fence running back into the yard with most
    of its boards gone (P L5 [03-04] [04-05]). Nothing inside a tableau host's band."""
    L = D.L
    bmap = {b["id"]: b for b in L.blocks}
    for b in L.buildings:
        if b["kind"] != "street":
            continue
        blk = bmap[b["block"]]
        fname, p0, tg, nrm, length, fyaw = [f for f in faces(b["rect"]) if f[0] == b["side"]][0]
        back = (-nrm[0], -nrm[1])
        depth_lim = plot_depth_limit(blk, b)
        key = b["id"] + ".remains"
        n = pick({0: .15, 1: .45, 2: .30, 3: .10}, rnd(key, 1))
        for j in range(n):
            k2 = f"{key}.{j}"
            dist = b["depth_m"] + 2.0 + max(depth_lim - b["depth_m"] - 5.0, 2.0) * rnd(k2, 2)
            t = length * (0.1 + 0.8 * rnd(k2, 3))
            x, z = p0[0] + tg[0] * t + back[0] * dist, p0[1] + tg[1] * t + back[1] * dist
            if D.blocked(x, z, margin=1.0, obj_r=1.5) or in_tableau_keep(L, x, z):
                continue
            yaw = fyaw + (rnd(k2, 4) - 0.5) * 20.0
            D.add("yard_shed_ash", "scorch", x, z, yaw, (1.1 + 0.5 * rnd(k2, 5), 1.0, 0.9 + 0.4 * rnd(k2, 6)), dy=-0.03)
            for m in range(1 + int(rnd(k2, 7) * 2)):
                jx, jz = x + (rnd(k2, 10 + m) - 0.5) * 3.0, z + (rnd(k2, 15 + m) - 0.5) * 3.0
                if D.blocked(jx, jz, margin=0.5):
                    continue
                if m % 2 == 0:
                    D.add("yard_shed_timber", "charred_plank", jx, jz, rnd(k2, 30 + m) * 360.0,
                          (1.0 + 0.8 * rnd(k2, 20 + m), 1.0, 1.0), dy=-0.01)
                else:
                    D.add("yard_shed_timber", "joist", jx, jz, rnd(k2, 30 + m) * 360.0,
                          (0.25 + 0.15 * rnd(k2, 20 + m), 0.6, 0.7), dy=-0.05)
            if rnd(k2, 40) < 0.3:
                wx, wz = x + back[0] * 1.2, z + back[1] * 1.2
                if not D.blocked(wx, wz, margin=0.3):
                    D.add("yard_shed_wall", "fence_board", wx, wz, fyaw, (0.5 + 0.3 * rnd(k2, 41), 0.9, 1.0), dy=-0.05)
        if rnd(key, 50) >= 0.4:
            continue
        end = 0.0 if rnd(key, 51) < 0.5 else length
        d, m = b["depth_m"] + 2.5, 0
        while d < depth_lim - 1.0:
            x, z = p0[0] + tg[0] * end + back[0] * d, p0[1] + tg[1] * end + back[1] * d
            u = rnd(f"{key}.fence.{m}", 1)
            if u >= 0.55 and not D.blocked(x, z, margin=0.3, obj_r=1.0) and not in_tableau_keep(L, x, z):
                sy = (pv("fence.low_h_m") if u < 0.7 else pv("fence.board_h_m")) / 2.03
                D.add("yard_fence", "fence_board", x, z, fyaw + 90.0, (1.0, sy, 1.0), dy=-0.05)
            d += 4.0
            m += 1


def dress_open_lots(D):
    """A vacant lot on a street line burned out before January 1943 (P L1: flattened areas of low
    mounds; R1 §6.2: timber houses left as heaps of ash): ash, charred boards and loose brick on it."""
    L = D.L
    for g in L.gaps:
        if g["kind"] != "open":
            continue
        a, b = g["along"]
        side = g["side"]
        key = f"lot.{g['block']}.{side}.{int(a)}"
        horiz = side in ("n", "s")
        inward = {"n": 1.0, "s": -1.0, "w": 1.0, "e": -1.0}[side]
        line = g["rect"][2] if side == "n" else g["rect"][3] if side == "s" else g["rect"][0] if side == "w" else g["rect"][1]
        for j in range(1 + int((b - a) // 10.0)):
            k2 = f"{key}.{j}"
            along = a + (b - a) * (0.2 + 0.6 * rnd(k2, 1))
            depth = 3.0 + 7.0 * rnd(k2, 2)
            x, z = (along, line + inward * depth) if horiz else (line + inward * depth, along)
            if D.blocked(x, z, margin=0.5) or in_tableau_keep(L, x, z):
                continue
            D.add("lot_ash", "scorch", x, z, rnd(k2, 3) * 360.0, (1.4 + 0.6 * rnd(k2, 4), 1.0, 1.2 + 0.5 * rnd(k2, 5)), dy=-0.03)
            for m in range(2):
                jx, jz = x + (rnd(k2, 10 + m) - 0.5) * 3.5, z + (rnd(k2, 12 + m) - 0.5) * 3.5
                if D.blocked(jx, jz, margin=0.3):
                    continue
                if g["cls"] == "fringe" or m == 0:
                    D.add("lot_timber", "charred_plank", jx, jz, rnd(k2, 20 + m) * 360.0, (1.2 + 0.6 * rnd(k2, 22 + m), 1.0, 1.0), dy=-0.01)
                else:
                    D.add("lot_rubble", "debris" if rnd(k2, 24) < 0.5 else "debris2", jx, jz, rnd(k2, 25) * 360.0,
                          (1.1 + 0.5 * rnd(k2, 26), 1.2 + 1.2 * rnd(k2, 27), 1.1 + 0.4 * rnd(k2, 28)), dy=-0.03)


def dress_battle(D):
    """The traces of five months of fighting over the centre, sourced and restrained (R1 §6.3-§7.5;
    P L7, L10, L11, S3, S6, S7): the small shell and bomb craters' dark earth thrown onto the snow
    (the >= 5 m craters are carved in the terrain), soot and ash fallen out of burned buildings,
    knocked-out and abandoned tanks along one side of the side streets inside their debris halos,
    and at firing positions in the ruins the boxes, boards and a helmet left behind. The dead, the
    wounded and the graves stay with the tableaux (R2); dead horses, wire, hedgehogs, trenches and
    spent cases have no reviewed asset here or are below the terrain's resolution (LIMITATIONS)."""
    L = D.L
    ext = L.extent
    D.battle = {}
    # 1. small craters: only their ejecta is laid (the bowl is below the heightfield's resolution)
    density = pv("crater.density_per_km2_all") * (1.0 - pv("crater.share_resolved"))
    step = 10.0
    lo_d, hi_d = pv("crater.small_diameter_m")
    n_small = 0
    for ix, x in enumerate(np.arange(ext["x0"] + step / 2, ext["x1"], step)):
        for iz, z in enumerate(np.arange(ext["z0"] + step / 2, ext["z1"], step)):
            key = f"crater.small.{ix}.{iz}"
            if rnd(key, 1) >= density * step * step / 1e6:
                continue
            cx = float(x + (rnd(key, 2) - 0.5) * step)
            cz = float(z + (rnd(key, 3) - 0.5) * step)
            dia = lo_d + (hi_d - lo_d) * rnd(key, 4) ** 1.6      # most are the small shell craters
            if D.blocked(cx, cz, margin=0.4, obj_r=1.5, spawn_r=14.0) or in_tableau_keep(L, cx, cz):
                continue
            span = min(1.6 * dia, 5.0) / 2.2
            D.add("crater_ejecta", "scorch", cx, cz, rnd(key, 5) * 360.0,
                  (span * (0.85 + 0.3 * rnd(key, 6)), 1.0, span * (0.85 + 0.3 * rnd(key, 7))), dy=-0.03)
            n_small += 1
    # the carved craters' ejecta lies round their rims, each patch on its own ground
    n_rim = 0
    for c in L.craters:
        if not c["inside"]:
            continue
        key = f"crater.rim.{c['x']}.{c['z']}"
        for m in range(3):
            a = (m / 3.0 + rnd(key, m) * 0.25) * math.tau
            rx, rz = c["x"] + math.cos(a) * c["r"] * 1.35, c["z"] + math.sin(a) * c["r"] * 1.35
            if D.blocked(rx, rz, margin=0.3) or in_tableau_keep(L, rx, rz):
                continue
            D.add("crater_ejecta", "scorch", rx, rz, math.degrees(a), (1.3, 1.0, 0.8), dy=-0.03)
            n_rim += 1
    # 2. soot and ash fallen out of burned buildings onto the snow at their street fronts
    n_soot = 0
    for b in L.buildings:
        if b["razed"] or b["condition"] != "burned" or b["kind"] == "slab":
            continue
        fname, p0, tg, nrm, length, fyaw = [f for f in faces(b["rect"]) if f[0] == b["side"]][0]
        key = b["id"] + ".soot"
        for j in range(1 + int(length // 40.0)):
            t = length * (0.15 + 0.7 * rnd(key, 10 + j))
            off = 1.6 + 1.2 * rnd(key, 20 + j)
            x, z = p0[0] + tg[0] * t + nrm[0] * off, p0[1] + tg[1] * t + nrm[1] * off
            if D.blocked(x, z, margin=0.3) or in_tableau_keep(L, x, z):
                continue
            D.add("soot", "scorch", x, z, fyaw + (rnd(key, 30 + j) - 0.5) * 30.0,
                  (1.3 + 0.6 * rnd(key, 40 + j), 1.0, 0.7 + 0.3 * rnd(key, 50 + j)), dy=-0.03)
            n_soot += 1
    # 3. more knocked-out and abandoned tanks along one side of the side streets
    have = [(w["x"], w["z"]) for w in getattr(D, "wrecks", [])]
    added = 0
    streets = sorted((s for s in L.streets if s["cls"] == "ordinary"), key=lambda s: (rnd(s["id"] + ".wreck", 1), s["id"]))
    for s in streets:
        if added >= pv("battle.wrecks_added"):
            break
        side = 1.0 if rnd(s["id"] + ".wreck", 2) < 0.5 else -1.0
        kerb = s["c"] + side * (s["w"] / 2 - 2.6)
        cands = []
        for i in range(24):
            t = s["lo"] + 30.0 + (s["hi"] - s["lo"] - 60.0) * rnd(s["id"] + ".wreck", 10 + i)
            x, z = (kerb, t) if s["axis"] == "ns" else (t, kerb)
            cross = [c for c in L.streets if c["axis"] != s["axis"]]
            if any(abs((z if s["axis"] == "ns" else x) - c["c"]) < c["w"] / 2 + 14.0 for c in cross):
                continue
            if any(math.hypot(x - hx, z - hz) < 70.0 for hx, hz in have):
                continue
            if in_tableau_keep(L, x, z):
                continue
            cands.append((x, z))
        spot = free_spot(D, cands, 2.5)
        if not spot:
            continue
        x, z = spot
        asset = ("wreck_pz3_whole", "wreck_kv1", "wreck_pz3", "wreck_kv1_whole")[added % 4]
        deg = (0.0 if s["axis"] == "ew" else 90.0) + (rnd(s["id"] + ".wreck", 3) - 0.5) * 16.0
        D.add("wreck", asset, x, z, deg, (1.0, 1.0, 1.0), dy=-0.25, hard_extent=[8.1, 3.3, 4.0])
        D.wrecks.append(dict(asset=asset, x=r3(x), z=r3(z), deg=r3(deg)))
        have.append((x, z))
        added += 1
        for m in range(8):
            k = f"{asset}.{s['id']}.halo.{m}"
            a = rnd(k, 1) * math.tau
            r = 5.0 + 10.0 * rnd(k, 2)
            hx, hz = x + math.cos(a) * r, z + math.sin(a) * r
            if D.blocked(hx, hz, margin=0.2):
                continue
            item = ["board", "crate", "plank", "joist", "crate", "charred_plank", "helmet", "board"][m]
            if item == "helmet":
                D.add("wreck_halo", "helmet", hx, hz, rnd(k, 3) * 360.0, (1.3, 1.3, 1.3), dy=-HELMET_MESH_Y * 1.3 - 0.02)
                continue
            s_ = (0.35, 0.7, 0.8) if item == "joist" else (1.0, 1.0, 1.0)
            D.add("wreck_halo", item, hx, hz, rnd(k, 3) * 360.0, s_, dy=-0.03)
    # 4. firing positions in the ruins: what the crews left at a strongpoint's corner
    cands = sorted((b for b in L.buildings if not b["razed"] and b["kind"] == "street" and b["typology"] in MASONRY
                    and b["condition"] != "intact" and not in_tableau_keep(L, b["x"], b["z"])),
                   key=lambda b: (rnd(b["id"] + ".position", 1), b["id"]))
    n_pos = 0
    for b in cands:
        if n_pos >= pv("battle.positions"):
            break
        fname, p0, tg, nrm, length, fyaw = [f for f in faces(b["rect"]) if f[0] == b["side"]][0]
        key = b["id"] + ".position"
        t = 1.5 if rnd(key, 2) < 0.5 else length - 1.5
        x, z = p0[0] + tg[0] * t + nrm[0] * 1.4, p0[1] + tg[1] * t + nrm[1] * 1.4
        if D.blocked(x, z, margin=0.3, obj_r=1.2, spawn_r=20.0):
            continue
        D.add("position_kit", "crate", x, z, fyaw + (rnd(key, 3) - 0.5) * 30.0, (1.0, 1.0, 1.0), dy=-0.02)
        x2, z2 = x + tg[0] * 1.1 * (1 if t < 2 else -1), z + tg[1] * 1.1 * (1 if t < 2 else -1)
        if not D.blocked(x2, z2, margin=0.2):
            D.add("position_kit", "crate", x2, z2, fyaw + (rnd(key, 4) - 0.5) * 50.0, (1.0, 1.0, 1.0), dy=-0.02)
        x3, z3 = x + nrm[0] * 1.3, z + nrm[1] * 1.3
        if not D.blocked(x3, z3, margin=0.2):
            D.add("position_kit", "board" if rnd(key, 5) < 0.6 else "plank", x3, z3, fyaw + (rnd(key, 6) - 0.5) * 40.0,
                  (1.0, 1.0, 1.0), dy=-0.02)
        if rnd(key, 7) < 0.35:
            x4, z4 = x - nrm[0] * 0.6 + tg[0] * 0.5, z - nrm[1] * 0.6 + tg[1] * 0.5
            if not D.blocked(x4, z4, margin=0.1):
                D.add("position_kit", "helmet", x4, z4, rnd(key, 8) * 360.0, (1.3, 1.3, 1.3), dy=-HELMET_MESH_Y * 1.3 - 0.02)
        n_pos += 1
    D.battle = dict(small_crater_ejecta=n_small, carved_crater_rim_patches=n_rim, soot_patches=n_soot,
                    wrecks_added=added, wrecks_total=len(D.wrecks), positions=n_pos)


def encounter_keepout(L, x, z, r_route=30.0, r_gun=28.0):
    for route in L.rust["routes"]:
        for a, b in zip(route, route[1:] + route[:1]):
            if seg_dist(x, z, a, b) < r_route:
                return True
    for g in L.rust["at_gun_sites"]:
        if math.hypot(x - g[0], z - g[1]) < r_gun:
            return True
    for k, route in enumerate(L.rust["routes"]):
        d = (route[1][0] - route[0][0], route[1][1] - route[0][1])
        n = math.hypot(*d)
        for i in range(L.rust["platoon_tanks"][k]):
            mx = route[0][0] - d[0] / n * L.rust["platoon_spacing_m"] * i
            mz = route[0][1] - d[1] / n * L.rust["platoon_spacing_m"] * i
            if math.hypot(x - mx, z - mz) < 20.0:
                return True
    return False


def dress_open_ground(D):
    """The open winter ground around the district (P M1 / L14 [04-01] [04-23] [02-01] [03-10]
    [04-02]): trodden snow with a few poles and fence lines, stumps and debris on top of the snow,
    streets surviving as tracks (the opt-in ground texture only). The T-34 beats, their muster points and the
    45 mm gun sites keep their ground: nothing is placed within 30 m of them."""
    L = D.L
    ext = L.extent
    xmax = min(L.ring[1], L.grid.world_half - 20.0)

    def outside(x, z):
        return not (ext["x0"] - 2.0 <= x <= ext["x1"] + 2.0 and ext["z0"] - 2.0 <= z <= ext["z1"] + 2.0)

    def ok(x, z, r=3.0):
        return (outside(x, z) and L.ring[0] <= x <= xmax and L.ring[2] <= z <= L.ring[3]
                and not encounter_keepout(L, x, z) and not D.blocked(x, z, obj_r=r, spawn_r=30.0)
                and seg_dist(x, z, L.diag["a"], L.diag["full_end"]) > L.diag["w"] / 2 + 2.0)

    # fence remnants of the one-storey quarter beyond the north lane
    for i in range(7):
        key = f"ring.fence.{i}"
        x0 = ext["x0"] + 20.0 + (ext["x1"] - ext["x0"] - 60.0) * rnd(key, 1)
        z = ext["z0"] - 5.0 - 30.0 * rnd(key, 2)
        n = 2 + int(rnd(key, 3) * 5)
        for m in range(n):
            x = x0 + 4.0 * m + 2.0
            if not ok(x, z) or rnd(key, 10 + m) < 0.5:
                continue
            D.add("ring_fence", "fence_board", x, z, 0.0, (1.0, (pv("fence.board_h_m") if rnd(key, 20 + m) < 0.5 else pv("fence.low_h_m")) / 2.03, 1.0), dy=-0.05)
    # a telegraph line along the track north-west toward the embankment's rail cut
    start = (ext["x0"] + 45.0, ext["z0"] - 12.0)
    gap = [t for t in L.masks["ridge"]["gaps"]]
    cz = L.masks["ridge"]["center"][1] + gap[0][0] if gap else -550.0
    target = (L.masks["ridge"]["center"][0], cz)
    d = (target[0] - start[0], target[1] - start[1])
    n = math.hypot(*d)
    u = (d[0] / n, d[1] / n)
    prev = None
    for m in range(8):
        x, z = start[0] + u[0] * 38.0 * m, start[1] + u[1] * 38.0 * m
        key = f"ring.tele.{m}"
        if not ok(x, z):
            prev = None
            continue
        state, top = put_pole(D, "ring_pole", key, x, z, pv("pole.height_m"), (u[1], -u[0]))
        if top is not None and prev is not None and rnd(key, 9) >= 0.35:
            wire_span(D, "ring_wire", key + ".w", prev[0], (x, z), prev[1] - 0.15, top - 0.15, sag_scale=2.0,
                      asset="wire")
        prev = ((x, z), top) if top is not None else None
    # stumps, a few stripped trees, debris lying on the snow
    items = [("stump", 18), ("tree_small", 6), ("debris_item", 26)]
    for name, count in items:
        placed = 0
        for i in range(count * 6):
            if placed >= count:
                break
            key = f"ring.{name}.{i}"
            x = L.ring[0] + (xmax - L.ring[0]) * rnd(key, 1)
            z = L.ring[2] + (L.ring[3] - L.ring[2]) * rnd(key, 2)
            if not ok(x, z):
                continue
            if name == "stump":
                D.add("ring_stump", "stump", x, z, rnd(key, 3) * 360.0, (1.0 + 0.5 * rnd(key, 4),) * 3)
            elif name == "tree_small":
                D.add("ring_tree", "tree_small", x, z, rnd(key, 3) * 360.0, (1.4 + 0.8 * rnd(key, 4),) * 3)
            else:
                item = ["plank", "board", "brick_stack", "joist", "plank", "crate"][i % 6]
                s = (0.35, 0.7, 0.8) if item == "joist" else (1.0, 1.0, 1.0)
                D.add("ring_debris", item, x, z, rnd(key, 3) * 360.0, s, dy=-0.03)
            placed += 1


# ════════════════════════════════════════════════════════════════════════════════════════════════
# ground texture (opt-in, --texture): the act drape with the district's snow, streets, tracks and
# paths. Act 1 never shows it: terrain_material.wgsl's winter branch (mood 0) is a constant snow
# albedo that ignores the drape; kept for spring shading, offline previews and a future winter input.
# ════════════════════════════════════════════════════════════════════════════════════════════════
TEX_NAME = "ground_hires.png"
# the PSX snow tile make_ground_tex.py multiplies into Act 1's winter ground ("bare" class)
SNOW_TILE = ("source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted/PSX Textures v3.1/"
             "PSX Textures/256/Color Maps/snow_1.png")
# multiplicative albedo factors, applied in linear light (P L9 [04-23] [02-08] [01-10] [06-03]
# [04-25]; L8 [04-01] [04-11]); photographs give relative value only, never albedo (P §1.4)
ALBEDO = {
    "street": (0.90, 0.90, 0.91),     # trodden crust over pavement or earth
    "rut": (0.58, 0.54, 0.50),        # vehicle tracks cut to earth and slush
    "path": (0.80, 0.79, 0.79),       # footpaths trodden dark
    "square": (0.94, 0.94, 0.95),
    "debris": (0.84, 0.80, 0.78),     # brick dust and debris through thin snow at ruins
    "razed": (0.74, 0.66, 0.62),
    "ash": (0.70, 0.69, 0.69),        # burned timber plots: ash under snow
    "ejecta": (0.62, 0.58, 0.55),     # a fresh crater's thrown earth
    "track_bed": (0.93, 0.92, 0.92),
}


def srgb_to_linear(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def linear_to_srgb(c):
    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * np.power(c, 1.0 / 2.4) - 0.055)


def build_ground_texture(L, D, src):
    from PIL import Image
    Image.MAX_IMAGE_PIXELS = None
    if not (ROOT / SNOW_TILE).is_file():
        raise SystemExit(f"snow grain tile missing: {SNOW_TILE} (or run without --texture)")
    im = Image.open(ROOT / src.manifest["terrain"]["texture"]).convert("RGB")
    W, Hh = im.size
    world = L.grid.world_half
    mpp = 2.0 * world / W
    margin = 60.0
    rx0, rx1 = L.ring[0] - margin, min(L.ring[1] + margin, world)
    rz0, rz1 = L.ring[2] - margin, L.ring[3] + margin
    c0 = int(math.floor((rx0 + world) / (2 * world) * W))
    c1 = min(W, int(math.ceil((rx1 + world) / (2 * world) * W)))
    r0 = int(math.floor((rz0 + world) / (2 * world) * Hh))
    r1 = min(Hh, int(math.ceil((rz1 + world) / (2 * world) * Hh)))
    region = np.asarray(im.crop((c0, r0, c1, r1)), dtype=np.float64) / 255.0
    lin = srgb_to_linear(region)
    # the drape inside the ring is rebuilt the way make_ground_tex.py builds snow ground: a measured
    # snow colour times the PSX snow tile's grain (1 texel = 1 output pixel, tiled from the image
    # origin so its phase matches the untouched drape around it)
    tile = np.asarray(Image.open(ROOT / SNOW_TILE).convert("RGB"), dtype=np.float64)
    ratio = np.clip(tile / np.maximum(tile.reshape(-1, 3).mean(0), 1.0), 0.62, 1.65)
    grain = ratio[(np.arange(r0, r1) % tile.shape[0])][:, (np.arange(c0, c1) % tile.shape[1])]
    lum = region.mean(-1)
    lo_q, hi_q = np.percentile(lum, 70), np.percentile(lum, 95)
    sel = (lum >= lo_q) & (lum <= hi_q)
    drape_srgb = region[sel].mean(0)
    snow = np.full(3, pv("snow.albedo_linear"))
    snow_srgb = linear_to_srgb(snow)
    base_lin = srgb_to_linear(np.clip(snow_srgb[None, None, :] * grain, 0.0, 1.0))
    xs = (np.arange(c0, c1) + 0.5) / W * 2 * world - world
    zs = (np.arange(r0, r1) + 0.5) / Hh * 2 * world - world
    mod = np.ones(region.shape)

    def sub(rect, pad=0.0):
        a = max(0, int(np.searchsorted(xs, rect[0] - pad)))
        b = int(np.searchsorted(xs, rect[1] + pad))
        c = max(0, int(np.searchsorted(zs, rect[2] - pad)))
        d = int(np.searchsorted(zs, rect[3] + pad))
        return slice(c, d), slice(a, b)

    def apply(mask_fn, rect, pad, colour, strength=1.0):
        rsl, csl = sub(rect, pad)
        if rsl.stop <= rsl.start or csl.stop <= csl.start:
            return
        X = xs[csl][None, :]
        Z = zs[rsl][:, None]
        m = np.clip(mask_fn(X, Z), 0.0, 1.0) * strength
        f = np.array(colour)[None, None, :]
        mod[rsl, csl] *= 1.0 - m[..., None] * (1.0 - f)

    def seg_mask(a, b, half, soft=0.6):
        ax, az = a
        bx, bz = b
        dx, dz = bx - ax, bz - az
        L2 = dx * dx + dz * dz

        def fn(X, Z):
            t = np.clip(((X - ax) * dx + (Z - az) * dz) / max(L2, 1e-9), 0.0, 1.0)
            dist = np.hypot(X - (ax + t * dx), Z - (az + t * dz))
            return 1.0 - smoothstep((dist - half) / soft)
        return fn, (min(ax, bx), max(ax, bx), min(az, bz), max(az, bz))

    def seg(a, b, half, colour, strength=1.0, soft=0.6):
        fn, rect = seg_mask(a, b, half, soft)
        apply(fn, rect, half + soft + 1.0, colour, strength)

    def rect_mask(r, soft):
        def fn(X, Z):
            dx = np.maximum(np.maximum(r[0] - X, X - r[1]), 0.0)
            dz = np.maximum(np.maximum(r[2] - Z, Z - r[3]), 0.0)
            return 1.0 - smoothstep(np.hypot(dx, dz) / max(soft, 1e-6))
        return fn

    # deterministic value noise (1.5 m cells) for mottling
    M32 = np.uint64(0xFFFFFFFF)
    ci = (np.floor(xs / 1.5).astype(np.int64) & 0xFFFFFFFF).astype(np.uint64)[None, :]
    cj = (np.floor(zs / 1.5).astype(np.int64) & 0xFFFFFFFF).astype(np.uint64)[:, None]
    hsh = ((ci * np.uint64(73856093)) ^ (cj * np.uint64(19349663)) ^ np.uint64(SEED)) & M32
    hsh = (hsh * np.uint64(2654435761)) & M32
    hsh ^= hsh >> np.uint64(15)
    hsh = (hsh * np.uint64(2246822519)) & M32
    hsh ^= hsh >> np.uint64(13)
    noise = ((hsh >> np.uint64(8)) & np.uint64(0xFFFF)).astype(np.float64) / 65536.0
    noise = gaussian(noise, 1.2)

    for st in L.streets:
        apply(rect_mask(st["rect"], 1.5), st["rect"], 2.0, ALBEDO["street"])
    seg(L.diag["a"], L.diag["full_end"], L.diag["w"] / 2, ALBEDO["street"])
    main = [s for s in L.streets if s["cls"] == "main"][0]
    half = pv("tram.track_centres_m") / 2
    tb = (main["c"] - half - 1.2, main["c"] + half + 1.2, main["lo"], main["hi"])
    apply(rect_mask(tb, 0.6), tb, 1.0, ALBEDO["track_bed"])
    if L.square:
        apply(rect_mask(L.square["rect"], 2.0), L.square["rect"], 3.0, ALBEDO["square"])
    # vehicle ruts: the tram street, the old road, the track north to the open ground, the courtyard
    lanes = [((main["c"] - 4.0, main["hi"]), (main["c"] - 4.0, L.ring[2])),
             ((main["c"] + 4.0, main["lo"] + 150.0), (main["c"] + 4.0, main["hi"])),
             (L.diag["a"], (L.diag["a"][0] + (L.diag["full_end"][0] - L.diag["a"][0]) * 0.45,
                            L.diag["a"][1] + (L.diag["full_end"][1] - L.diag["a"][1]) * 0.45))]
    for (a, b) in lanes:
        dx, dz = b[0] - a[0], b[1] - a[1]
        n = math.hypot(dx, dz)
        nx, nz = -dz / n, dx / n
        for side in (-1.4, 1.4):
            seg((a[0] + nx * side, a[1] + nz * side), (b[0] + nx * side, b[1] + nz * side), 0.45, ALBEDO["rut"], 0.85)
    for st in L.streets:
        if st["cls"] == "main":
            continue
        a = (st["lo"], st["c"]) if st["axis"] == "ew" else (st["c"], st["lo"])
        b = (st["hi"], st["c"]) if st["axis"] == "ew" else (st["c"], st["hi"])
        if st["axis"] == "ew":
            a, b = (a[0], a[1]), (b[0], b[1])
        key = st["id"] + ".rut"
        strength = 0.35 + 0.35 * rnd(key, 1)
        off = (rnd(key, 2) - 0.5) * 3.0
        for side in (-1.4, 1.4):
            if st["axis"] == "ew":
                seg((a[0], a[1] + off + side), (b[0], b[1] + off + side), 0.4, ALBEDO["rut"], strength)
            else:
                seg((a[0] + off + side, a[1]), (b[0] + off + side, b[1]), 0.4, ALBEDO["rut"], strength)
    # footpaths along the building lines and across the square and razed plots
    for st in L.streets:
        for side in (-1, 1):
            p = st["c"] + side * (st["w"] / 2 - 2.2)
            if st["axis"] == "ns":
                seg((p, st["lo"]), (p, st["hi"]), 0.45, ALBEDO["path"], 0.7)
            else:
                seg((st["lo"], p), (st["hi"], p), 0.45, ALBEDO["path"], 0.7)
    if L.square:
        x0, x1, z0, z1 = L.square["rect"]
        seg((x0, z0), (x1, z1), 0.5, ALBEDO["path"])
        seg((x0, z1), (x1 - 20.0, z0), 0.5, ALBEDO["path"], 0.8)
    seg(L.src_spawn, (L.courtyard_keep[0], (L.courtyard_keep[2] + L.courtyard_keep[3]) / 2), 0.5, ALBEDO["path"])
    for g in L.gaps:
        if g["kind"] not in ("gate", "wall"):
            continue
        a, b = g["along"]
        mid = (a + b) / 2
        side = g["side"]
        r = g["rect"]
        if side in ("n", "s"):
            line = r[2] if side == "n" else r[3]
            out = -1.0 if side == "n" else 1.0
            seg((mid, line - out * 12.0), (mid, line + out * 6.0), 0.45, ALBEDO["path"], 0.8)
        else:
            line = r[0] if side == "w" else r[1]
            out = -1.0 if side == "w" else 1.0
            seg((line - out * 12.0, mid), (line + out * 6.0, mid), 0.45, ALBEDO["path"], 0.8)
    for b in L.buildings:
        r = b["rect"]
        if b["razed"]:
            rsl, csl = sub(r, 4.0)
            fn = rect_mask(r, 3.0)
            m = np.clip(fn(xs[csl][None, :], zs[rsl][:, None]), 0, 1) * (0.55 + 0.45 * noise[rsl, csl])
            mod[rsl, csl] *= 1.0 - m[..., None] * (1.0 - np.array(ALBEDO["razed"]))[None, None, :]
        elif b["typology"] == "wooden_house" and b["condition"] != "intact":
            rsl, csl = sub(r, 3.0)
            fn = rect_mask(r, 2.5)
            m = np.clip(fn(xs[csl][None, :], zs[rsl][:, None]), 0, 1) * (0.5 + 0.5 * noise[rsl, csl])
            mod[rsl, csl] *= 1.0 - m[..., None] * (1.0 - np.array(ALBEDO["ash"]))[None, None, :]
        elif b["condition"] != "intact":
            strength = {"burned": 0.45, "shelled": 0.6, "partly_collapsed": 0.85}[b["condition"]]
            rsl, csl = sub(r, 5.0)
            fn = rect_mask(r, 4.0)
            m = np.clip(fn(xs[csl][None, :], zs[rsl][:, None]), 0, 1) * strength * (0.4 + 0.6 * noise[rsl, csl])
            mod[rsl, csl] *= 1.0 - m[..., None] * (1.0 - np.array(ALBEDO["debris"]))[None, None, :]
    if L.ridge:
        rr = ridge_rect(L.ridge)
        apply(rect_mask(rr, 2.0), rr, 3.0, ALBEDO["razed"], 0.8)
    for c in L.craters:
        if c["d"] < 0.9:
            continue
        cx, cz, r = c["x"], c["z"], c["r"]

        def ring_fn(X, Z, cx=cx, cz=cz, r=r):
            d = np.hypot(X - cx, Z - cz) / r
            return np.clip(1.0 - np.abs(d - 1.5) / 0.8, 0.0, 1.0)
        apply(ring_fn, (cx - 2.5 * r, cx + 2.5 * r, cz - 2.5 * r, cz + 2.5 * r), 0.0, ALBEDO["ejecta"], 0.7)
    # blend back to the untouched drape outside the ring
    X = xs[None, :]
    Z = zs[:, None]
    dx = np.maximum(np.maximum(L.ring[0] - X, X - L.ring[1]), 0.0)
    dz = np.maximum(np.maximum(L.ring[2] - Z, Z - L.ring[3]), 0.0)
    w = (1.0 - smoothstep(np.hypot(dx, dz) / (margin - 5.0)))[..., None]
    new_lin = base_lin * mod
    out_lin = lin * (1.0 - w) + new_lin * w
    out = np.clip(np.round(linear_to_srgb(out_lin) * 255.0), 0, 255).astype(np.uint8)
    im.paste(Image.fromarray(out), (c0, r0))
    stats = dict(region_px=[c0, r0, c1, r1], metres_per_px=r3(mpp), snow_linear_rgb=[r3(v) for v in snow],
                 snow_srgb=[r3(v) for v in snow_srgb], drape_snow_srgb_measured=[r3(v) for v in drape_srgb],
                 grain_tile=SNOW_TILE, source=src.manifest["terrain"]["texture"],
                 albedo_factors=ALBEDO)
    return im, stats


def dress(L, H, src):
    D = Dresser(L, H, src)
    dress_razed(D)
    dress_ridge(D)
    dress_gaps(D)
    dress_utilities(D, src)
    dress_wrecks(D)
    dress_trees(D)
    dress_wooden(D)
    dress_ruins(D)
    dress_yards(D)
    dress_yard_remains(D)
    dress_open_lots(D)
    dress_battle(D)
    dress_drifts(D)
    dress_open_ground(D)
    return D


# ════════════════════════════════════════════════════════════════════════════════════════════════
# plan image
# ════════════════════════════════════════════════════════════════════════════════════════════════
def plan_overlay(L, D, review, street):
    """Dressing, terrain features, conflicts and the street encounter drawn over the plan."""
    L.legend_extra = [
        ("brown x-box: razed plot (terrain mound + rubble)", (205, 150, 100)),
        ("red box: fallen facade closing a street", (230, 90, 70)),
        ("rings: craters (terrain)", (150, 190, 230)),
        ("dark red lines: brick yard walls, piers, facade chunks", (170, 60, 50)),
        ("tan lines: board fences", (200, 170, 110)),
        ("white dots / lines: poles / wires", (240, 240, 240)),
        ("green dots: stripped trees; black: wrecks", (120, 200, 120)),
        ("cyan ticks: snow drifts in the lee", (160, 220, 255)),
        ("green: street encounter kill zone (tram street thick)", (90, 230, 120)),
        ("green dots: candidate 45 mm sites (game probes them)", (90, 230, 120)),
        ("yellow dashed: T-34 street beats A, B (applied)", (240, 210, 80)),
    ]
    wire_classes = {"tram_wire", "telegraph_wire", "ring_wire"}

    def extras(d, P, s, font, small):
        for m in getattr(L, "mound_records", []):
            pass
        for c in L.craters:
            a = P(c["x"] - c["r"], c["z"] - c["r"])
            b = P(c["x"] + c["r"], c["z"] + c["r"])
            d.ellipse([a[0], a[1], b[0], b[1]], outline=(150, 190, 230, 255), width=2)
        if L.ridge:
            r = ridge_rect(L.ridge)
            a, b = P(r[0], r[2]), P(r[1], r[3])
            d.rectangle([a[0], a[1], b[0], b[1]], fill=(230, 90, 70, 110), outline=(230, 90, 70, 255), width=3)
        for op, (cls, key) in zip(D.ops, D.meta):
            x, z = op["x"], op["z"]
            if cls in wire_classes:
                length = op["s"][0] * 4.0
                a = math.radians(op["deg"])
                ux, uz = math.cos(a), -math.sin(a)
                d.line([P(x - ux * length / 2, z - uz * length / 2), P(x + ux * length / 2, z + uz * length / 2)],
                       fill=(240, 240, 240, 150), width=1)
            elif key == "pole":
                c = P(x, z)
                col = (240, 240, 240, 255) if cls != "gate_pier" else (170, 60, 50, 255)
                d.ellipse([c[0] - 2, c[1] - 2, c[0] + 2, c[1] + 2], fill=col)
            elif key == "yard_wall":
                length = 4.0 * op["s"][0]
                a = math.radians(op["deg"])
                ux, uz = math.cos(a), -math.sin(a)
                d.line([P(x - ux * length / 2, z - uz * length / 2), P(x + ux * length / 2, z + uz * length / 2)],
                       fill=(170, 60, 50, 255), width=3)
            elif key == "fence_board":
                a = math.radians(op["deg"])
                ux, uz = math.cos(a), -math.sin(a)
                d.line([P(x - ux * 2, z - uz * 2), P(x + ux * 2, z + uz * 2)], fill=(200, 170, 110, 255), width=2)
            elif key in ("tree_tall", "tree_small"):
                c = P(x, z)
                d.ellipse([c[0] - 3, c[1] - 3, c[0] + 3, c[1] + 3], fill=(120, 200, 120, 255))
            elif key == "snow_drift":
                c = P(x, z)
                d.line([c, (c[0] + 3, c[1])], fill=(160, 220, 255, 200), width=2)
            elif key in ("wreck_kv1", "wreck_pz3"):
                c = P(x, z)
                d.rectangle([c[0] - 6, c[1] - 4, c[0] + 6, c[1] + 4], fill=(10, 10, 10, 255), outline=(255, 255, 255, 255))
        for line in street["kill_zone"]:
            pts = [P(*p) for p in line["points"]]
            d.line(pts, fill=(90, 230, 120, 170), width=5 if line["role"] == "tram_street" else 2)
        for g in street["gun_sites"]:
            c = P(g["x"], g["z"])
            d.ellipse([c[0] - 1.5, c[1] - 1.5, c[0] + 1.5, c[1] + 1.5], fill=(90, 230, 120, 255))
        for beat in street["beats"]:
            pts = [P(*p) for p in beat["points"]] + [P(*beat["points"][0])]
            for a, b in zip(pts, pts[1:]):
                for k in range(0, 30, 2):
                    p0 = (a[0] + (b[0] - a[0]) * k / 30, a[1] + (b[1] - a[1]) * k / 30)
                    p1 = (a[0] + (b[0] - a[0]) * (k + 1) / 30, a[1] + (b[1] - a[1]) * (k + 1) / 30)
                    d.line([p0, p1], fill=(240, 210, 80, 230), width=3)
            d.text((pts[0][0] + 6, pts[0][1] + 4), "T-34 street beat " + beat["platoon"], fill=(240, 210, 80, 255), font=small)
    return extras


def render_plan(L, src, path, H=None, extras=None):
    from PIL import Image, ImageDraw, ImageFont
    ext = L.extent
    ring = (L.ring[0], min(L.ring[1], L.grid.world_half), L.ring[2], min(L.ring[3], L.grid.world_half))
    m = 40.0
    x0, x1, z0, z1 = ring[0] - m, ring[1] + m, ring[2] - m, min(ring[3] + m, L.grid.world_half)
    x1 = min(x1, L.grid.world_half)
    s = 1.6  # px per metre
    W, Hh = int((x1 - x0) * s), int((z1 - z0) * s)
    legend_w = 520
    img = Image.new("RGB", (W + legend_w, Hh), (22, 22, 24))
    d = ImageDraw.Draw(img)
    try:
        font = ImageFont.truetype("/usr/share/fonts/noto/NotoSans-Regular.ttf", 15)
        bold = ImageFont.truetype("/usr/share/fonts/noto/NotoSans-Bold.ttf", 19)
        small = ImageFont.truetype("/usr/share/fonts/noto/NotoSans-Regular.ttf", 12)
    except OSError:
        font = bold = small = ImageFont.load_default()

    def P(x, z):
        return ((x - x0) * s, (z - z0) * s)

    def R(r):
        a = P(r[0], r[2])
        b = P(r[1], r[3])
        return [a[0], a[1], b[0], b[1]]

    # hillshade of the (final or source) terrain
    Hsrc = H if H is not None else src.heights
    rs, cs = L.grid.window(x0, x1, z0, z1)
    sub = Hsrc[rs, cs].astype(np.float64)
    gz, gx = np.gradient(sub, L.grid.sp)
    shade = np.clip(0.55 + (-gx * 0.6 - gz * 0.8) * 0.9, 0, 1)
    lo, hi = np.percentile(sub, 2), np.percentile(sub, 98)
    tone = np.clip((sub - lo) / max(hi - lo, 1e-6), 0, 1)
    base = (0.35 * shade + 0.25 * tone + 0.2)
    rgb = np.stack([base * 214, base * 218, base * 226], -1).astype(np.uint8)
    terr = Image.fromarray(rgb).resize((int(sub.shape[1] * L.grid.sp * s), int(sub.shape[0] * L.grid.sp * s)),
                                       Image.BILINEAR)
    ox, oz = P(L.grid.axis[cs.start], L.grid.axis[rs.start])
    img.paste(terr, (int(round(ox)), int(round(oz))))
    d = ImageDraw.Draw(img, "RGBA")
    # extent and ring
    d.rectangle(R((ring[0], ring[1], ring[2], ring[3])), outline=(90, 160, 90, 255), width=2)
    d.rectangle(R((ext["x0"], ext["x1"], ext["z0"], ext["z1"])), outline=(240, 240, 240, 255), width=3)
    # rail corridor edge
    rx = L.extent["rail_corridor_east_edge_x"]
    d.line([P(rx, z0), P(rx, z1)], fill=(160, 100, 200, 200), width=2)
    # streets
    for st in L.streets:
        col = {"main": (70, 70, 74, 235), "ordinary": (92, 92, 96, 230), "lane": (110, 110, 112, 220)}[st["cls"]]
        d.rectangle(R(st["rect"]), fill=col)
    if L.square:
        d.rectangle(R(L.square["rect"]), fill=(150, 140, 120, 170), outline=(220, 200, 160, 255), width=2)
    # old road
    a, b = L.diag["a"], L.diag["b"]
    d.line([P(*a), P(*b)], fill=(92, 92, 96, 235), width=int(L.diag["w"] * s))
    fe = L.diag["full_end"]
    far = (a[0] + (fe[0] - a[0]) * 0.35, a[1] + (fe[1] - a[1]) * 0.35)
    d.line([P(*b), P(*far)], fill=(92, 92, 96, 140), width=int(L.diag["w"] * s))
    # tram tracks on the main street
    main = [st for st in L.streets if st["cls"] == "main"][0]
    for off in (-pv("tram.track_centres_m") / 2, pv("tram.track_centres_m") / 2):
        d.line([P(main["c"] + off, main["lo"]), P(main["c"] + off, main["hi"])], fill=(40, 40, 44, 255), width=2)
    # courtyard keep and pad
    d.rectangle(R(L.courtyard_keep), outline=(255, 210, 60, 255), width=2)
    pad = src.courtyard_pad
    d.rectangle(R(pad), fill=(255, 210, 60, 90), outline=(255, 210, 60, 255), width=2)
    # existing objects
    for (x, y, z, stem, origin) in L.objects:
        c = P(x, z)
        d.point(c, fill=(255, 200, 80, 255))
    # gaps (walls / fences)
    for g in L.gaps:
        col = {"gate": (180, 90, 60, 255), "wall": (180, 90, 60, 255), "fence": (160, 130, 80, 255),
               "open": (120, 120, 120, 120)}.get(g["kind"], (200, 200, 200, 255))
        d.rectangle(R(g["rect"]), fill=col)
    # buildings
    cond_alpha = {"intact": 255, "burned": 225, "shelled": 200, "partly_collapsed": 170}
    for bld in L.buildings:
        r = bld["rect"]
        if bld["razed"]:
            d.rectangle(R(r), fill=(120, 84, 60, 150), outline=(200, 140, 90, 255), width=1)
            d.line([P(r[0], r[2]), P(r[1], r[3])], fill=(200, 140, 90, 255), width=1)
            d.line([P(r[0], r[3]), P(r[1], r[2])], fill=(200, 140, 90, 255), width=1)
            continue
        c = TYPOLOGY_COLOUR[bld["typology"]]
        a_ = cond_alpha[bld["condition"]]
        d.rectangle(R(r), fill=c + (a_,), outline=(20, 20, 20, 255), width=1)
        # condition glyph
        cx, cz = P((r[0] + r[1]) / 2, (r[2] + r[3]) / 2)
        glyph = {"intact": "I", "burned": "B", "shelled": "S", "partly_collapsed": "P"}[bld["condition"]]
        d.text((cx - 4, cz - 7), glyph, fill=(255, 255, 255, 255), font=small)
        # facade tick
        yaw = math.radians(bld["yaw_deg"])
        nx, nz = math.sin(yaw), math.cos(yaw)
        fx = (r[0] + r[1]) / 2 + nx * ((r[1] - r[0]) / 2)
        fz = (r[2] + r[3]) / 2 + nz * ((r[3] - r[2]) / 2)
        d.line([P(fx, fz), P(fx + nx * 3, fz + nz * 3)], fill=(255, 255, 255, 255), width=2)
    if extras:
        extras(d, P, s, font, small)
    # mission points, spawn
    for name, (x, z), col in (("SPAWN", src.spawn, (80, 255, 80)), ("HOUSE", L.rust["house"], (255, 230, 80)),
                              ("WRECK", L.rust["wreck"], (255, 230, 80)), ("WITHDRAW", L.rust["withdraw"], (255, 230, 80))):
        c = P(x, z)
        d.ellipse([c[0] - 6, c[1] - 6, c[0] + 6, c[1] + 6], outline=col + (255,), width=3)
        d.text((c[0] + 8, c[1] - 8), name, fill=col + (255,), font=font)
    for site in L.map_sites:
        c = P(site["x"], site["z"])
        d.rectangle([c[0] - 5, c[1] - 5, c[0] + 5, c[1] + 5], outline=(120, 220, 255, 255), width=2)
        d.text((c[0] + 8, c[1] - 8), "map site", fill=(120, 220, 255, 255), font=small)
    # encounter routes and anti-tank guns
    for k, route in enumerate(L.rust["routes"]):
        pts = [P(*p) for p in route] + [P(*route[0])]
        d.line(pts, fill=(255, 120, 30, 255), width=3)
        d.text((pts[0][0] + 6, pts[0][1] + 4), f"courtyard T-34 beat {'AB'[k]}", fill=(255, 150, 60, 255), font=font)
    rm = L.rust["route_mid"]
    for k, site in enumerate(L.rust["at_gun_sites"]):
        c = P(*site)
        d.polygon([(c[0], c[1] - 9), (c[0] - 8, c[1] + 6), (c[0] + 8, c[1] + 6)], outline=(255, 60, 60, 255),
                  fill=(255, 60, 60, 120))
        d.line([c, P(*rm)], fill=(255, 60, 60, 160), width=1)
        d.text((c[0] + 9, c[1] - 6), f"courtyard 45 mm gun {k + 1}", fill=(255, 90, 90, 255), font=font)
    c = P(*rm)
    d.ellipse([c[0] - 4, c[1] - 4, c[0] + 4, c[1] + 4], outline=(255, 60, 60, 255), width=2)
    # scale bar and north arrow
    sb = P(ring[0] - 20, ring[3] + 25)
    d.line([sb, (sb[0] + 100 * s, sb[1])], fill=(240, 240, 240, 255), width=3)
    d.text((sb[0], sb[1] + 4), "100 m", fill=(240, 240, 240, 255), font=font)
    na = (W - 40, 40)
    d.line([na, (na[0], na[1] + 40)], fill=(240, 240, 240, 255), width=3)
    d.polygon([(na[0], na[1] - 12), (na[0] - 8, na[1] + 4), (na[0] + 8, na[1] + 4)], fill=(240, 240, 240, 255))
    d.text((na[0] - 6, na[1] + 44), "N (-z)", fill=(240, 240, 240, 255), font=small)
    # legend
    lx = W + 18
    ly = 16
    d.text((lx, ly), "Urban intro district, January 1943 (plan)", fill=(255, 255, 255, 255), font=bold)
    ly += 30
    for line in (
            f"extent x {ext['x0']:.0f}..{ext['x1']:.0f}, z {ext['z0']:.0f}..{ext['z1']:.0f} (white)",
            f"open winter ground ring {RING_M:.0f} m (green)",
            "grey: streets (tram street dark, lanes light)",
            "tan: square; dark line: old road (breakout spine)",
            "purple line: rail corridor edge (embankment crest)",
            "yellow: courtyard pad + keep; dots: existing objects"):
        d.text((lx, ly), line, fill=(220, 220, 220, 255), font=font)
        ly += 21
    ly += 8
    for t in TYPOLOGIES:
        c = TYPOLOGY_COLOUR[t]
        d.rectangle([lx, ly + 2, lx + 22, ly + 16], fill=c + (255,), outline=(20, 20, 20, 255))
        n = sum(1 for b in L.buildings if b["typology"] == t and not b["razed"])
        d.text((lx + 30, ly), f"{t} ({n})", fill=(230, 230, 230, 255), font=font)
        ly += 22
    d.rectangle([lx, ly + 2, lx + 22, ly + 16], fill=(120, 84, 60, 200), outline=(200, 140, 90, 255))
    d.text((lx + 30, ly), f"razed plot: rubble mound ({sum(1 for b in L.buildings if b['razed'])})",
           fill=(230, 230, 230, 255), font=font)
    ly += 26
    d.text((lx, ly), "condition glyph: I intact, B burned, S shelled,", fill=(230, 230, 230, 255), font=font)
    ly += 20
    d.text((lx, ly), "P partly collapsed; fill fades with damage;", fill=(230, 230, 230, 255), font=font)
    ly += 20
    d.text((lx, ly), "white tick = street facade (+Z local)", fill=(230, 230, 230, 255), font=font)
    ly += 26
    d.text((lx, ly), "orange: the courtyard layout's T-34 beats", fill=(255, 150, 60, 255), font=font)
    ly += 20
    d.text((lx, ly), "red: the courtyard layout's 45 mm sites + route_mid", fill=(255, 90, 90, 255), font=font)
    ly += 20
    d.text((lx, ly), "(unused here; conflicts in encounter_review)", fill=(255, 90, 90, 255), font=font)
    ly += 26
    if hasattr(L, "legend_extra"):
        for line, col in L.legend_extra:
            d.text((lx, ly), line, fill=col + (255,), font=font)
            ly += 20
    img.save(path)


# ════════════════════════════════════════════════════════════════════════════════════════════════
# analysis: clear lanes, open ground, encounter conflicts
# ════════════════════════════════════════════════════════════════════════════════════════════════
def hard_boxes(L, D):
    """World AABBs of everything that stops a tank: standing footprints and hard dressing."""
    out = []
    for b in L.buildings:
        if not b["razed"]:
            out.append((b["rect"][0], b["rect"][1], b["rect"][2], b["rect"][3], "building:" + b["id"]))
    for h in D.hard:
        lx, ly, lz = h["extent"] or (0.2, 1.0, 0.2)
        lx *= 1.0
        a = math.radians(h["deg"])
        c, s = abs(math.cos(a)), abs(math.sin(a))
        hx = c * lx / 2 + s * lz / 2
        hz = s * lx / 2 + c * lz / 2
        out.append((h["x"] - hx, h["x"] + hx, h["z"] - hz, h["z"] + hz, h["cls"]))
    return out


def clear_lanes(L, D):
    """Widest free interval across every street carriageway, sampled every 2 m along it (R1 §8.2:
    the game's 4.12 m Tiger collision box needs about 5 m; 6 m is required here)."""
    boxes = hard_boxes(L, D)
    need = pv("tiger.min_clear_lane_m")
    results = []
    ridge_r = ridge_rect(L.ridge) if L.ridge else None
    for st in L.streets:
        lo_c, hi_c = st["c"] - st["w"] / 2, st["c"] + st["w"] / 2
        worst = (1e9, None)
        closed = []
        for along in np.arange(st["lo"] + 1.0, st["hi"] - 1.0 + 1e-6, 2.0):
            blocked = []
            for (x0, x1, z0, z1, cls) in boxes:
                if st["axis"] == "ns":
                    if z0 <= along <= z1 and x1 > lo_c and x0 < hi_c:
                        blocked.append((max(x0, lo_c), min(x1, hi_c)))
                else:
                    if x0 <= along <= x1 and z1 > lo_c and z0 < hi_c:
                        blocked.append((max(z0, lo_c), min(z1, hi_c)))
            blocked.sort()
            best, cur = 0.0, lo_c
            for a, b in blocked:
                best = max(best, a - cur)
                cur = max(cur, b)
            best = max(best, hi_c - cur)
            px, pz = (st["c"], along) if st["axis"] == "ns" else (along, st["c"])
            in_ridge = ridge_r is not None and ridge_r[0] - 1 <= px <= ridge_r[1] + 1 and ridge_r[2] - 1 <= pz <= ridge_r[3] + 1
            if best < need:
                closed.append((r3(along), r3(best), "fallen facade (intended)" if in_ridge else "blocked"))
            if best < worst[0] and not in_ridge:
                worst = (best, along)
        unintended = [c for c in closed if c[2] == "blocked"]
        results.append(dict(street=st["id"], cls=st["cls"], width_m=st["w"], min_clear_m=r3(worst[0]),
                            at=r3(worst[1]) if worst[1] is not None else None,
                            closed_stations=len(closed), unintended_closed_stations=len(unintended),
                            intended_closure=bool(closed) and not unintended))
    # the old road: no hard object may stand in its carriageway
    inside = [h for h in D.hard if seg_dist(h["x"], h["z"], L.diag["a"], L.diag["b"]) < L.diag["w"] / 2]
    results.append(dict(street=L.diag["id"], cls="old_road", width_m=L.diag["w"], hard_objects_in_carriageway=len(inside)))
    return results


def open_ground_stats(L):
    """Distance from ground points to the nearest standing masonry (P L1 asks for open ground of
    50-250 m between ruins in the most destroyed parts of the centre)."""
    ext = L.extent
    masonry = [b["rect"] for b in L.buildings if not b["razed"] and b["typology"] in MASONRY]
    ds = []
    for x in np.arange(ext["x0"] + 2.5, ext["x1"], 5.0):
        for z in np.arange(ext["z0"] + 2.5, ext["z1"], 5.0):
            if any(r[0] <= x <= r[1] and r[2] <= z <= r[3] for r in masonry):
                continue
            ds.append(min(rect_dist(r, x, z) for r in masonry))
    ds = np.array(ds)
    return dict(points=int(ds.size), median_m=r3(np.median(ds)), p90_m=r3(np.percentile(ds, 90)), max_m=r3(ds.max()),
                note="distance from open ground to the nearest standing masonry shell; open gaps are "
                     "twice these values (P L1: 50-250 m between ruins in [04-01] [06-12])")


def terrain_los(L, H, a, b, eye=2.37, target_h=1.5, step=4.0):
    """Max terrain height above the sight line from a Tiger gun (2.37 m, R1 §8.1) to a hull target."""
    ga = L.grid.sample(H, *a) + eye
    gb = L.grid.sample(H, *b) + target_h
    n = int(math.hypot(b[0] - a[0], b[1] - a[1]) / step)
    worst = -1e9
    for i in range(1, n):
        t = i / n
        x, z = a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t
        worst = max(worst, L.grid.sample(H, x, z) - (ga + (gb - ga) * t))
    return worst


def encounter_review(L, D, H, src):
    rust = L.rust
    standing = [b for b in L.buildings if not b["razed"]]
    boxes = hard_boxes(L, D)
    ext = L.extent
    out = dict(at_gun_sites=[], routes=[], musters=[], notes=[])

    def inside_ext(x, z):
        return ext["x0"] <= x <= ext["x1"] and ext["z0"] <= z <= ext["z1"]

    rm = rust["route_mid"]
    for k, g in enumerate(rust["at_gun_sites"]):
        in_fp = [b["id"] for b in standing if rect_dist(b["rect"], *g) == 0.0]
        near = min(standing, key=lambda b: rect_dist(b["rect"], *g))
        blockers = sorted(set(bx[4] if not bx[4].startswith("building:") else bx[4][9:] for bx in boxes
                              if rect_seg_dist(bx[:4], g, rm, 64) < 0.5))
        street = [s["id"] for s in L.streets if s["rect"][0] <= g[0] <= s["rect"][1] and s["rect"][2] <= g[1] <= s["rect"][3]]
        terr_before = terrain_los(L, src.heights.astype(np.float64), g, rm, eye=1.0, target_h=1.5)
        terr = terrain_los(L, H.astype(np.float64), g, rm, eye=1.0, target_h=1.5)
        out["at_gun_sites"].append(dict(
            index=k, site=list(g), route_mid=list(rm), in_district=inside_ext(*g),
            on_district_edge=min(abs(g[0] - ext["x0"]), abs(g[0] - ext["x1"]), abs(g[1] - ext["z0"]),
                                 abs(g[1] - ext["z1"])) < 1.0,
            on_street=street, inside_footprint=in_fp, nearest_building=near["id"],
            nearest_building_m=r3(rect_dist(near["rect"], *g)),
            line_of_fire_to_route_mid_blocked_by=blockers,
            terrain_above_line_of_fire_m=dict(before=r3(terr_before), after=r3(terr)),
            conflict=bool(in_fp or blockers or terr > 0.2)))
    for k, route in enumerate(rust["routes"]):
        segs = []
        for a, b in zip(route, route[1:] + route[:1]):
            hit = sorted(set(bx[4] for bx in boxes if rect_seg_dist(bx[:4], a, b, 64) < 3.0))
            dist = min(rect_seg_dist((ext["x0"], ext["x1"], ext["z0"], ext["z1"]), a, b, 64), 1e9)
            segs.append(dict(a=list(a), b=list(b), blocked_by=hit, distance_to_district_m=r3(dist)))
        out["routes"].append(dict(platoon="AB"[k], segments=segs, conflict=any(s["blocked_by"] for s in segs)))
        d = (route[1][0] - route[0][0], route[1][1] - route[0][1])
        n = math.hypot(*d)
        for i in range(rust["platoon_tanks"][k]):
            m = (route[0][0] - d[0] / n * rust["platoon_spacing_m"] * i, route[0][1] - d[1] / n * rust["platoon_spacing_m"] * i)
            near = min(rect_dist(bx[:4], *m) for bx in boxes)
            out["musters"].append(dict(platoon="AB"[k], place=i, at=[r3(m[0]), r3(m[1])], nearest_hard_m=r3(near),
                                       conflict=near < 6.0))
    main = [s for s in L.streets if s["cls"] == "main"][0]
    route_a = rust["routes"][0]
    cross = None
    for a, b in zip(route_a, route_a[1:] + route_a[:1]):
        if (a[0] - main["c"]) * (b[0] - main["c"]) < 0:
            t = (main["c"] - a[0]) / (b[0] - a[0])
            cross = (main["c"], a[1] + (b[1] - a[1]) * t)
    if cross:
        before = terrain_los(L, src.heights.astype(np.float64), src.spawn, cross)
        after = terrain_los(L, H.astype(np.float64), src.spawn, cross)
        out["tram_street_sightline"] = dict(
            from_spawn=list(src.spawn), to_beat_a_crossing=[r3(cross[0]), r3(cross[1])],
            terrain_above_line_m_before=r3(before), terrain_above_line_m_after=r3(after),
            note="positive = the terrain (a hump north of the courtyard) hides a T-34 hull from a Tiger at the "
                 "spawn; the district does not change it (buildings stand off the tram street)")
    out["notes"].append(
        "The T-34 beats and musters lie outside the district by construction (its north edge is 60 m short "
        "of the beats). When platoons leave their beats to investigate or pursue, they will drive into the "
        "district: their clear-point and manoeuvre probes (live_playtest.rs winter_patrol_point_clear / "
        "winter_safe_maneuver) test story_cover, destructibles and occluders, so the building runtime must "
        "register every instance there (F1/F2).")
    return out


# ════════════════════════════════════════════════════════════════════════════════════════════════
# the winter encounter in the streets (applied: the game reads manifest_edits.json "street_encounter")
# ════════════════════════════════════════════════════════════════════════════════════════════════
STREET_ENCOUNTER_KEY = "street_encounter"
STREET_ENCOUNTER_SCHEMA = "kessel.street-encounter"
# The route north continues this far into the open ground beyond the district's north edge (the
# courtyard's platoon beats lie beyond it).
KILL_ZONE_BEYOND_EDGE_M = 60.0
# Beyond point blank of the spawn (at_guns::POINT_BLANK_M is 200 m; the game's probe also refuses any
# site that can see the spawn or the house within its 400 m opening range).
GUN_SITE_MIN_SPAWN_M = 150.0
# A gun beside a building corner stands this far off the building's faces (a 53-K's shield is 1.2 m
# wide; its crew works behind it).
CORNER_OFFSET_M = 2.5
# A gun does not stand in its own tanks' beat: this far from any beat leg (off the middle of the
# carriageway the T-34s drive; a hull is 3.6 m wide).
GUN_OFF_BEAT_M = 12.0
# Beats by street crossings (the street ids of plan_streets; "west_edge" = where the street leaves the
# district's west side, "square" = the square's centre). Platoon A (three tanks) walks the northern
# streets round the blocks either side of the tram street; platoon B (two) comes in along street B from
# the west edge, down the side street W1 and across the square, and goes back out along street D and the
# open ground outside the west edge. Beats keep to the 18 m streets: the 12 m lanes, with poles 1 m inside
# their kerbs, leave a 3.6 m hull little room, and the game musters a platoon on its first corner only
# where a T-34 has room to turn (live_playtest winter_patrol_point_clear, a 4.46 m half-box).
STREET_BEATS = (
    ("A", (("st.e1", "st.a"), ("st.w1", "st.a"), ("st.w1", "st.b"), ("st.e1", "st.b"))),
    ("B", (("west_edge", "st.b"), ("st.w1", "st.b"), ("st.w1", "st.c"), "square", ("st.w1", "st.d"),
           ("west_edge", "st.d"))),
)
# How far inside the district's west edge a beat's west-edge corner stands.
WEST_EDGE_INSET_M = 5.0


def street_encounter(L, D):
    """What the winter encounter needs to fight in these streets, written for the game to measure
    (src/street_encounter.rs): the kill zone, the candidate 45 mm gun sites and the two platoons'
    beats. The game chooses the two gun sites among the candidates by a field-of-fire probe on the
    real district (its own sight lines through the built buildings, terrain and dressing), so nothing
    here claims a line of fire. Doctrine (docs/research/SOVIET_AT_WEAPONS_BRIEF_20260928.md; R1 §7.1):
    anti-tank fire in the town came "mainly from ambushes, using breaches in fences and walls,
    doorways, gates, windows" (S3 p. 298), flanking fire from neighbouring buildings covered the
    approaches (S3 p. 296); guns were sited for flanking fire on the tanks' route."""
    ids = {s["id"]: s for s in L.streets}
    main = [s for s in L.streets if s["cls"] == "main"][0]
    xm = main["c"]
    ext = L.extent
    house_z = L.rust["house"][1]
    spawn = L.src_spawn
    z_out = ext["z0"] - KILL_ZONE_BEYOND_EDGE_M
    ns = sorted((s for s in L.streets if s["axis"] == "ns" and s["cls"] != "main"), key=lambda s: s["c"])
    north_ew = sorted((s for s in L.streets if s["axis"] == "ew" and s["c"] < house_z), key=lambda s: s["c"])
    if not ns or not north_ew:
        raise SystemExit("street encounter: the district has no side streets or no street north of the house")
    west = max((s for s in ns if s["c"] < xm), key=lambda s: s["c"])
    east = min((s for s in ns if s["c"] > xm), key=lambda s: s["c"])
    first_north = north_ew[-1]
    # kill zone: the tram street north out of the courtyard (the way the Tiger must go to reach the
    # tanks) and the Tiger's ways out north: the streets that leave it and the side streets' exits.
    kill = [dict(id=main["id"], role="tram_street", weight=1.0, points=[[r3(xm), r3(house_z)], [r3(xm), r3(z_out)]])]
    for s in north_ew:
        kill.append(dict(id=s["id"], role="way_out", weight=0.5,
                         points=[[r3(west["c"]), r3(s["c"])], [r3(east["c"]), r3(s["c"])]]))
    for s in ns:
        kill.append(dict(id=s["id"], role="way_out", weight=0.5 if s["cls"] == "ordinary" else 0.25,
                         points=[[r3(s["c"]), r3(first_north["c"])], [r3(s["c"]), r3(z_out)]]))
    # beats
    beats = []
    sq = L.square["rect"] if L.square else None
    for platoon, spec in STREET_BEATS:
        pts = []
        for item in spec:
            if item == "square":
                if sq is None:
                    raise SystemExit("street encounter: beat %s crosses a square the district does not have" % platoon)
                pts.append([r3((sq[0] + sq[1]) / 2), r3((sq[2] + sq[3]) / 2)])
                continue
            a, b = item
            if a == "west_edge":
                if b not in ids or ids[b]["axis"] != "ew":
                    raise SystemExit(f"street encounter: beat {platoon} leaves the west edge by no E-W street: {item}")
                pts.append([r3(ext["x0"] + WEST_EDGE_INSET_M), r3(ids[b]["c"])])
                continue
            if a not in ids or b not in ids:
                raise SystemExit(f"street encounter: beat {platoon} names a street the district lacks: {item}")
            n, e = (ids[a], ids[b]) if ids[a]["axis"] == "ns" else (ids[b], ids[a])
            pts.append([r3(n["c"]), r3(e["c"])])
        beats.append(dict(platoon=platoon, points=pts))
    legs = [(tuple(a), tuple(b)) for beat in beats for a, b in zip(beat["points"], beat["points"][1:] + beat["points"][:1])]
    # candidate gun sites: rubble of the razed plots and the corners of standing buildings
    boxes = hard_boxes(L, D)
    standing = [b for b in L.buildings if not b["razed"]]

    def free(x, z, margin=0.5):
        return not any(bx[0] - margin <= x <= bx[1] + margin and bx[2] - margin <= z <= bx[3] + margin for bx in boxes)

    def usable(x, z):
        return (ext["x0"] - 1.0 <= x <= ext["x1"] + 1.0 and ext["z0"] - 1.0 <= z <= ext["z1"] + 1.0
                and math.hypot(x - spawn[0], z - spawn[1]) >= GUN_SITE_MIN_SPAWN_M
                and not (main["rect"][0] <= x <= main["rect"][1] and main["rect"][2] <= z <= main["rect"][3])
                and all(seg_dist(x, z, a, b) >= GUN_OFF_BEAT_M for a, b in legs)
                and free(x, z))

    sites = {}
    for b in L.buildings:
        if not b["razed"]:
            continue
        x0, x1, z0, z1 = b["rect"]
        for x in np.arange(x0 + 2.0, x1 - 2.0 + 1e-6, 4.0):
            for z in np.arange(z0 + 2.0, z1 - 2.0 + 1e-6, 4.0):
                if usable(float(x), float(z)):
                    sites[(r3(x), r3(z))] = ("rubble", b["id"])
    for b in standing:
        x0, x1, z0, z1 = b["rect"]
        for cx, sx in ((x0, -1.0), (x1, 1.0)):
            for cz, sz in ((z0, -1.0), (z1, 1.0)):
                for ox, oz in ((CORNER_OFFSET_M * sx, CORNER_OFFSET_M * sz), (CORNER_OFFSET_M * sx, -2.0 * sz),
                               (-2.0 * sx, CORNER_OFFSET_M * sz)):
                    x, z = cx + ox, cz + oz
                    if usable(x, z) and (r3(x), r3(z)) not in sites:
                        sites[(r3(x), r3(z))] = ("corner", b["id"])
    gun_sites = [dict(kind=k, on=on, x=x, z=z) for (x, z), (k, on) in sorted(sites.items(), key=lambda kv: (kv[1][0], kv[0]))]
    # the ways a tank may use inside the district: its streets, the square and the old road (yards,
    # plots and gaps between buildings are not ways); outside the district the open ground is free
    ways = dict(district=[r3(ext["x0"]), r3(ext["x1"]), r3(ext["z0"]), r3(ext["z1"])],
                rects=[[r3(v) for v in s["rect"]] for s in L.streets] + ([[r3(v) for v in sq]] if sq else []),
                corridors=[dict(a=[r3(v) for v in L.diag["a"]], b=[r3(v) for v in L.diag["b"]], width_m=r3(L.diag["w"]))])
    return dict(
        schema=STREET_ENCOUNTER_SCHEMA, version=1, tool=TOOL, tool_version=TOOL_VERSION,
        kill_zone=kill, gun_sites=gun_sites, beats=beats, ways=ways,
        rules=dict(kill_zone_beyond_edge_m=KILL_ZONE_BEYOND_EDGE_M, gun_site_min_spawn_m=GUN_SITE_MIN_SPAWN_M,
                   corner_offset_m=CORNER_OFFSET_M, gun_off_beat_m=GUN_OFF_BEAT_M,
                   candidates="razed-plot rubble on a 4 m grid 2 m inside the plot; three points outside each "
                              "corner of each standing building; outside every hard box (0.5 m margin), the tram "
                              "street's carriageway and the beats, at least the minimum from the spawn"),
        provenance=dict(doctrine=["docs/research/SOVIET_AT_WEAPONS_BRIEF_20260928.md (flanking fire from ambush; "
                                  "town fighting: guns in breaches and at corners)",
                                  "R1 §7.1 (S3 pp. 296-298: ambush openings, flanking fire from neighbouring buildings)"],
                        platoons=["docs/WINTER_PLATOONS_20260928.md (column 30-50 m apart, halts, the leader's radio)"]),
        note=("Applied by the game (src/street_encounter.rs) when a manifest revision carries this block; the "
              "courtyard workshop has none and keeps its open-ground encounter. The game picks the two gun sites "
              "among gun_sites by its field-of-fire probe on the built district and drives the beats along the "
              "streets it measures clear for a T-34."),
    )


def street_encounter_summary(street):
    """The receipt's and features' record of the street encounter block (the candidates themselves
    are in manifest_edits.json)."""
    kinds = {}
    for g in street["gun_sites"]:
        kinds[g["kind"]] = kinds.get(g["kind"], 0) + 1
    return dict(key=STREET_ENCOUNTER_KEY, schema=street["schema"], version=street["version"],
                kill_zone=street["kill_zone"], beats=street["beats"], ways=street["ways"], gun_site_candidates=kinds,
                rules=street["rules"], note=street["note"])


def module_estimate(b):
    """Rough upper bound of kit modules per building (walls x storeys + floors + roof + partitions)
    for TLAS budgeting; burned timber floors and roofs are absent in the grammar, so fewer render."""
    bx, bz = b["width_m"] / 4.0, b["depth_m"] / 4.0
    s = b["storeys"]
    if b["typology"] == "wooden_house":
        return 6 if b["condition"] != "intact" else int(2 * (bx + bz) + bx * bz + 4)
    walls = 2 * (bx + bz) * s
    floors = bx * bz * (s + (1 if b["cellar"] else 0))
    roof = bx * bz
    return int(walls + floors + roof + (bx + bz) * s * 0.5)


# ════════════════════════════════════════════════════════════════════════════════════════════════
# provenance per element (R1 = STALINGRAD_URBAN_FABRIC_20260928.md, P = PHOTO_REFERENCE_ANALYSIS_...)
# ════════════════════════════════════════════════════════════════════════════════════════════════
PROVENANCE = {
    "extent": dict(R1=["§2 Ermansky district between the Tsaritsa gorge, the railway and the Volga (S19)"],
                   P=["L13 slopes inside the city", "L14 open winter ground"], photos=["[04-21]", "[04-26]"]),
    "streets": dict(R1=["§5.2 facade-to-facade 17.6-29 m (S8, S9); summary for L", "§5.4 tram up Sovetskaya (S10p)"],
                    P=["L3 widths", "L4 street furniture"], photos=["[07-01]", "[08-12]", "[04-27]", "[03-04]"]),
    "blocks_and_plots": dict(R1=["§5.1 blocks (S6, S7, S8)", "§4 block recipes and arrangement [I]",
                                 "§3.1-3.4 footprints (S8, S9, S11, S13)"],
                             P=["L2 typology mix"], photos=["[07-01]", "[04-08]", "[04-07]", "[04-27]"]),
    "conditions": dict(R1=["§6.2 inferred shares (S1, S2, S3 p. 105, S5 pp. 202-203, 464-466, S28, S10)"],
                       P=["L1 composition", "G8-G13 failure by typology"], photos=["[04-01]", "[06-12]", "[04-02]", "[07-01]"]),
    "razed_plots": dict(R1=["§6.2 razed plots 5-15 % (S13 Zabolotny's house, milk house; S11)", "§6.3 mass balance"],
                        P=["G17 rubble budget", "L1 low mounds 0.3-1.5 m"], photos=["[04-01]", "[04-02]", "[04-12]"]),
    "fallen_facade_ridge": dict(R1=["§6.3 ridges 2-4 m close streets (S5 pp. 205-206, 211)", "§8.5", "§10"],
                                P=["G17 slopes 1.5-3 m against 4-5 storey ruins"], photos=["[04-04]", "[04-12]"]),
    "ruin_debris": dict(R1=["§6.3 (S2: broken brick, twisted rods)"], P=["G17", "L7"],
                        photos=["[04-09]", "[04-11]", "[07-16]", "[04-04]", "[03-03]", "[04-17]"]),
    "craters": dict(R1=["§6.4 >150,000 city-wide (S2); diameters inferred"], P=["L10"],
                    photos=["[04-11]", "[04-08]", "[04-16]"]),
    "tram_and_poles": dict(R1=["§5.4 gauge, span wires in the centre, network destroyed (S15, S16, S5 p. 464, S3 p. 89)"],
                           P=["L4"], photos=["[04-14]", "[08-12]", "[06-01]", "[04-04]", "[08-13]", "[07-14]", "[04-05]", "[07-16]"]),
    "yard_walls_and_gates": dict(R1=["§5.5 high brick yard walls, brick gates (S10o, S10g)"], P=["L12"],
                                 photos=["[04-09]", "[07-01]", "[08-12]"]),
    "fences": dict(R1=["§5.5 picket 0.8-1.1 m, board 1.6-2.0 m [P]"], P=["L5"], photos=["[03-04]", "[04-05]"]),
    "density_pass": dict(R1=["§0.1 yards full of sheds, stores, stables, summer kitchens and workshops (S14, S10i, S10p, S5 p. 464)",
                             "§4 recipes: occupancy core 0.75-0.9, ordinary 0.6-0.8, fringe 0.4-0.6; merchant houses in "
                             "continuous party-wall rows; blocks packed with small buildings on the street lines (S28)",
                             "§5.3 annexes deep into the yard (S10r), timber stores against yard walls (S10o)"],
                         P=["L2 one-storey brick houses on the street line, blocks set back in parallel rows [07-01] [04-08]",
                            "L12 sheds of timber posts with roofs caving in [04-14]"], photos=["[07-01]", "[04-08]", "[04-14]"]),
    "battle_traces": dict(R1=["§6.3 streets heaped with broken brick, twisted rods, wrecked vehicles, dug up by craters (S2)",
                              "§6.4 >150,000 craters, 750-1,900 per km2 inferred; wrecks (S3 p. 393)",
                              "§7.1 strongpoints in stone buildings, ambush fire from breaches and gates (S3 pp. 296-298)"],
                          P=["L7 dressing on top of the snow", "L10", "L11 wrecks in 5-15 m halos, one side of a road",
                             "S3 positions: boxes, spent cases", "S6 abandonment: kit dropped on the last snow",
                             "S7 knocked-out tanks: a halo of kit, planks and track links"],
                          photos=["[04-11]", "[08-12]", "[08-13]", "[06-08]", "[01-25]", "[05-01]", "[03-03]", "[02-13]"]),
    "yard_shed_remains": dict(R1=["§5.3 sheds, stores, summer kitchens (S10o, S10o2, S1, S9); gone by March 1943 (S28)"],
                              P=["L12"], photos=["[04-14]"]),
    "trees": dict(R1=["§6.4 (S3 p. 105; S5 p. 464 half-shattered trees)"], P=["L6"],
                  photos=["[04-10]", "[07-14]", "[04-02]", "[04-01]", "[04-23]"]),
    "wrecks": dict(R1=["§6.4 (S3 p. 393 streets jammed with wrecked vehicles)"], P=["L11"],
                   photos=["[08-12]", "[08-13]", "[06-08]", "[01-25]", "[05-01]"]),
    "snow": dict(R1=["§6.5 (S24, S23); snow depth [P]"], P=["L8", "V11"],
                 photos=["[04-01]", "[04-11]", "[06-12]", "[07-14]", "[08-13]", "[07-10]", "[07-01]", "[05-13]", "[05-06]"]),
    "paths_and_tracks": dict(R1=[], P=["L9"], photos=["[04-23]", "[02-08]", "[01-10]", "[06-03]", "[04-25]"]),
    "open_winter_ground": dict(R1=[], P=["M1", "L14"], photos=["[04-01]", "[04-23]", "[02-01]", "[03-10]", "[04-02]"]),
    "square": dict(R1=["§5.2 squares 60-250 m (S14)", "§7.4 trenches in squares (spoil 0.5-1 m)"], P=["L3", "L6", "L10"],
                   photos=["[04-10]", "[07-16]", "[08-13]"]),
}

LIMITATIONS = [
    "Terrain features (razed-plot mounds, the fallen-facade ridge, craters) live on the 4.29 m heightfield: "
    "features narrower than about two samples are smoothed; each mound's heightfield volume is measured "
    "against its R1 mass-balance volume in the receipt (heightfield_volume_ratio).",
    "Trenches through the square (P L10 [04-10], R1 §7.4) are narrower than the heightfield can carve and "
    "are not built; they need a mesh or a finer terrain.",
    "Poles are the editor's reviewed utility pole (pillar_hr_2, brick-textured), which the game registers as "
    "a static masonry pillar: an indestructible hard stop, where a real timber pole would snap. They stand "
    "1 m inside the carriageway edge. A timber-pole asset with a hinge (scatter) profile is needed.",
    "Wires are level meshes (the manifest transform has no pitch): a span between poles at different "
    "heights hangs at the lower attachment and meets the higher pole below its top, as on a bracket "
    "(the largest drop is measured in the receipt); wires carry no collision.",
    "Yard walls, gate piers and the ridge's facade chunks are manifest walls/pillars: exact-box hard stops "
    "that cannot be breached (the existing P0 'indestructible manifest wall' debt).",
    "Debris, snow drifts, planks, joists, fallen poles and shed remains are crushable visual dressing "
    "without collision; their height stays below about 0.45 m (drifts) and 0.35 m (debris).",
    "The tall rubble berms and cones against standing ruins must come from the runtime's failed modules "
    "(G17); this dressing does not fake them.",
    "Streets, trodden paths, ruts, ash and brick dust are not visible on the ground in Act 1: the winter "
    "terrain shading (shaders/terrain_material.wgsl, mood 0) is a constant fresh-snow albedo (0.91-0.95 "
    "linear) that never samples the drape, and the only runtime ground marks are tank tracks stamped at "
    "runtime (Renderer::stamp_tank_tracks). The opt-in --texture fork paints them into the drape for "
    "spring shading and offline previews only; P L9 in the game needs an engine input (a winter drape "
    "term or authored track stamps). Its factors are relative albedo changes, not measured albedos (P §1.4).",
    "The lee drifts are the authored snow_drift mesh, whose material (albedo 0.76/0.80/0.84) renders darker "
    "and bluer than the winter terrain snow (0.91-0.95) it lies on; one snow needs one material.",
    "Density pass: neighbouring pads that share a heightfield sample (within two 4.29 m cells) must stand within "
    "0.12 m of each other so every footprint stays flat to 0.10 m; on the DEM's 4-7 % slopes a party-wall row "
    "therefore breaks every one or two houses into a gate or yard wall where the next house's street level is "
    "more than about 0.7 m away. The frontage the pass left open, by reason, is in the receipt "
    "(dense_frontage_left_open_m); stepped floors across a party wall need a finer terrain under buildings.",
    "Density pass: the tableau hosts' bands (their yards across the block, catalog/urban_intro_tableaux.v1.json) "
    "take no new building and no new yard dressing, so the stories keep the ground they were authored on.",
    "Battle traces: craters under 5 m (about 90 % of R1's inferred population, size split not sourced) are "
    "laid only as their dark ejecta on the snow (the scorched-earth patch, a flat 0.07 m mesh without collision); "
    "their bowls are below the 4.29 m heightfield's resolution and are not carved. Soot patches at burned fronts "
    "are the same mesh. Trenches and communication trenches (R1 §7.4, P L10) are not dug: terrain openings exist "
    "only for cellars and tableau excavations, not as district data. No reviewed asset exists for barbed wire, "
    "hedgehogs, sandbags, a street barricade, spent cases large enough to resolve, abandoned guns or dead horses "
    "(R1 §7.3; P L10, S3); none is placed. The two added wreck models are the reviewed KV-1 and Pz III (burnt and "
    "unburnt), with the source-surface wreck collision the courtyard's wrecks have.",
    "No reviewed tramcar, rail, hedgehog, globe lamp post or weed asset exists: wrecked tramcars (R1 §5.4), "
    "rails, hedgehogs (P L10), lamp posts (P L4) and weeds through the snow (P M1) are absent.",
    "Streets blocked 'by fallen poles, wires, bricks and walls' (R1 §7.3, S3 p. 89) are the fallen-facade ridge "
    "across E1, fallen poles and debris; a built street barricade (R1 §7.3, S5 pp. 230-231) needs a reviewed "
    "collidable asset (the Pizza barricade props read as modern and carry no collision, so none is placed).",
    "The street encounter block (manifest_edits.json 'street_encounter') offers candidates, not sites: the game "
    "chooses the two 45 mm sites by its own field-of-fire probe on the built district (src/street_encounter.rs), "
    "so the plan draws only the candidates, the kill zone and the beats. Candidates are rubble of razed plots and "
    "points beside building corners; embrasures in windows, breaches and half-basements (R1 §7.1-7.2) are not "
    "offered, since no gun is placed inside a building. The encounter_review above still measures the courtyard "
    "layout's conflicts with footprints and terrain only.",
]


# ════════════════════════════════════════════════════════════════════════════════════════════════
# outputs
# ════════════════════════════════════════════════════════════════════════════════════════════════
OUTPUT_FILES = ["manifest.v1.json", "manifest_edits.json", "heights.npy", "features.json",
                "level_design.v1.json"]
RECEIPT_NAME = "urban-intro.receipt.json"
# Overcast January midday at 48.7 N (sun about 20 deg high, P V2 [06-02] [04-01] [04-25]); the
# exposure is the camera's (the tuning's maximum), not a change to any emitter or medium.
# Since the light foundation (2026-09-29) a lighting record is version 2: an instant of Universal
# Time the ephemeris lights, and the crewman's eye adapts (no camera exposure, no sun direction).
# Noon of 15 January 1943 puts the Sun about 20 deg up over the district.
REVIEW_LIGHTING = {"enabled": True, "fog_density": 0.002, "overcast": 0.7, "storm": 0.05,
                   "utc": "1943-01-15T09:10:00Z", "schema": "kessel.environment-authoring", "version": 2}
# The district's own instant: the courtyard's 7.5 deg storm dusk in January 1943, the afternoon Sun
# at 7.5 deg (tools/apply_ephemeris_lighting_20260929.py, docs/PHYSICAL_TRUTH_AUDIT.md).
DISTRICT_UTC = "1943-01-15T12:27:00Z"
RETIRED_LIGHTING = ("sun_azimuth_deg", "sun_elevation_deg", "camera_exposure", "twilight")


def district_lighting(record):
    """The forked courtyard record as a version-2 record at the district's instant (its fog, cloud
    cover and storm kept)."""
    record = {k: v for k, v in dict(record).items() if k not in RETIRED_LIGHTING}
    record["utc"] = DISTRICT_UTC
    record["version"] = 2
    return record


def dumps(obj, indent=2):
    return json.dumps(obj, indent=indent, ensure_ascii=False) + "\n"


def redrape_objects(L, src, H, ops):
    """Existing objects inside the changed terrain follow it: base placements get `set` ops, the
    courtyard's own add ops get their y moved. Returns (set ops, count of moved add ops)."""
    sets = []
    moved = 0
    Hs = src.heights
    for i, p in enumerate(src.manifest["placements"]):
        x, z = float(p[1]), float(p[3])
        if not (L.ring[0] - 30 <= x <= L.ring[1] + 30 and L.ring[2] - 30 <= z <= L.ring[3] + 30):
            continue
        d = L.grid.sample(H, x, z) - L.grid.sample(Hs, x, z)
        if abs(d) > 0.02:
            sets.append({"deg": p[4], "i": i, "op": "set", "s": p[5], "x": p[1], "y": r3(float(p[2]) + d), "z": p[3]})
    for op in ops:
        if op.get("op") != "add":
            continue
        x, z = float(op["x"]), float(op["z"])
        d = L.grid.sample(H, x, z) - L.grid.sample(Hs, x, z)
        if abs(d) > 0.02:
            op["y"] = r3(float(op["y"]) + d)
            moved += 1
    return sets, moved


def features_block(L, D):
    return {
        "tool": TOOL, "tool_version": TOOL_VERSION, "seed": SEED,
        "extent": {k: v for k, v in L.extent.items()},
        "ring": [r3(v) for v in L.ring],
        "streets": [dict(id=s["id"], cls=s["cls"], axis=s["axis"], centre=s["c"], width_m=s["w"],
                         rect=[r3(v) for v in s["rect"]]) for s in L.streets],
        "old_road": dict(id=L.diag["id"], a=[r3(v) for v in L.diag["a"]], b=[r3(v) for v in L.diag["b"]],
                         continues_to=[r3(v) for v in L.diag["full_end"]], width_m=L.diag["w"]),
        "square": dict(rect=[r3(v) for v in L.square["rect"]]) if L.square else None,
        "blocks": [dict(id=b["id"], rect=[r3(v) for v in b["rect"]], side_class=b["side_cls"]) for b in L.blocks],
        "plots": [dict(id=b["id"], typology=b["typology"], condition=b["condition"], razed=b["razed"], kind=b["kind"],
                       frontage_class=b["cls"], rect=[r3(v) for v in b["rect"]], yaw_deg=b["yaw_deg"],
                       storeys=b["storeys"], pad_y=r3(b["pad"]), intensity=r3(b["intensity"])) for b in L.buildings]
                 + list(getattr(L, "gumrak_plots", [])),
        "gumrak_site": getattr(L, "gumrak_record", None),
        "gaps": [dict(block=g["block"], side=g["side"], kind=g["kind"], rect=[r3(v) for v in g["rect"]]) for g in L.gaps],
        "razed_mounds": L.mound_records,
        "fallen_facade_ridge": (dict(building=L.ridge["building"]["id"], street=L.ridge["street"]["id"],
                                     rect=[r3(v) for v in ridge_rect(L.ridge)], crest_m=L.ridge.get("crest_m"),
                                     building_rubble_m3=L.ridge.get("rubble_volume_m3"),
                                     share_in_street=RIDGE_SHARE_OF_RUBBLE,
                                     heightfield_volume_m3=L.ridge.get("volume_m3"), forced=L.ridge_forced)
                                if L.ridge else None),
        "craters": L.craters,
        "courtyard_keep": [r3(v) for v in L.courtyard_keep],
        "courtyard_frozen": [r3(v) for v in L.courtyard_frozen],
        "protected_discs": [dict(x=r3(x), z=r3(z), r=r3(r), why=w) for (x, z, r, w) in L.discs if w != "existing_object"],
        "existing_objects_protected": sum(1 for d in L.discs if d[3] == "existing_object"),
        "map_sites": L.map_sites,
        "hard_dressing": D.hard,
        "wrecks": D.wrecks,
    }


# ════════════════════════════════════════════════════════════════════════════════════════════════
# the Gumrak stores beside the route west, outside the district (cellars and scenes, 29 Sep 2026)
# ════════════════════════════════════════════════════════════════════════════════════════════════
# Owner decision of 28 September 2026: the historical Gumrak stores block
# (a1_gumrak_burning_stores; tools/worldgen/historical_gumrak_20260925.py; Museum-Reserve KP 25221)
# stands on the route west outside the district. Its store and the domestic house beside it are
# building instances here, so the tableau (catalog/urban_intro_tableaux.v1.json) is authored in
# their rooms and moves with them. The site is where the old road (the act's route west toward the
# grain elevator) comes back to level ground past the valley west of the rail embankment; its
# terrain is the act's, flattened under the two footprints only. A route composite, not Gumrak's
# plan: the source places the fires at Gumrak station, 11 km north-west of the city centre.
GUMRAK_ROAD_T_M = 840.0          # along the old road from the spawn
GUMRAK_BUILDINGS = [
    # (id, typology, seed, width, depth, storeys, condition, side offset from the road centreline
    #  (+ = south-east, toward the poles), yaw)
    ("urban_intro.gumrak.store", "workshop", 0x6E3F2A11, 16.0, 16.0, 1, "burned", -17.0, 0.0),
    ("urban_intro.gumrak.house", "merchant_brick_house", 0x6E3F2A12, 12.0, 8.0, 1, "burned", 14.0, 180.0),
]
GUMRAK_APRON_M = 6.0


def gumrak_site(L, grid, H):
    """Place the Gumrak site's two buildings beside the old road beyond the district and flatten
    their pads into H (pad = the mean ground under the footprint, a 6 m apron back to the ground).
    Returns (instances, plots, record)."""
    a = np.array(L.diag["a"], dtype=np.float64)
    end = np.array(L.diag["full_end"], dtype=np.float64)
    u = (end - a) / np.linalg.norm(end - a)
    n = np.array([u[1], -u[0]])   # (0.667, 0.745): south-east of the road, the side of its poles
    road = a + u * GUMRAK_ROAD_T_M
    instances, plots, rects = [], [], []
    for bid, typ, seed, w, d, storeys, cond, side, yaw in GUMRAK_BUILDINGS:
        c = road + n * side
        x, z = float(round(c[0] * 2) / 2), float(round(c[1] * 2) / 2)
        q = int(round(yaw / 90.0)) % 2
        fw, fd = (w, d) if q == 0 else (d, w)
        rect = (x - fw / 2, x + fw / 2, z - fd / 2, z + fd / 2)
        rs, cs = grid.window(rect[0], rect[1], rect[2], rect[3])
        pad = float(np.mean(H[rs, cs]))
        instances.append({"id": bid, "typology": typ, "seed": int(seed), "width_m": w, "depth_m": d,
                          "storeys": storeys, "cellar": False, "condition": cond, "x": x, "z": z, "yaw_deg": yaw})
        plots.append(dict(id=bid, typology=typ, condition=cond, razed=False, kind="gumrak_site",
                          frontage_class="outside_district", rect=[r3(v) for v in rect], yaw_deg=yaw,
                          storeys=storeys, pad_y=r3(pad), intensity=0.0))
        rects.append((rect, pad))
    # pads: an apron blended back to the ground round each, then every footprint plus one sample
    # cell set exactly to its pad (last, so no apron reaches into another building's pad)
    cell = grid.sp + 1e-6

    def window(rect, reach):
        x0, x1, z0, z1 = rect
        rs, cs = grid.window(x0 - reach, x1 + reach, z0 - reach, z1 + reach)
        X = np.broadcast_to(grid.axis[cs][None, :], (rs.stop - rs.start, cs.stop - cs.start))
        Z = np.broadcast_to(grid.axis[rs][:, None], X.shape)
        dx = np.maximum(np.maximum(x0 - X, X - x1), 0.0)
        dz = np.maximum(np.maximum(z0 - Z, Z - z1), 0.0)
        return rs, cs, np.maximum(dx, dz)

    for rect, pad in rects:
        rs, cs, dinf = window(rect, cell + GUMRAK_APRON_M)
        t = smoothstep(np.clip((dinf - cell) / GUMRAK_APRON_M, 0.0, 1.0))
        H[rs, cs] = (pad * (1.0 - t) + H[rs, cs] * t).astype(H.dtype)
    for rect, pad in rects:
        rs, cs, dinf = window(rect, cell)
        block = H[rs, cs]
        block[dinf <= cell] = pad
        H[rs, cs] = block
    record = dict(
        decision="Owner decision of 28 September 2026: the Gumrak stores on the route west outside the district.",
        scene="a1_gumrak_burning_stores (tools/worldgen/historical_gumrak_20260925.py; "
              "docs/research/HISTORICAL_SCENE_SOURCE_AUDIT_20260925.md A1-06)",
        source="Anonymous 79th Infantry Division diary, Museum-Reserve KP 25221, late November 1942: at Gumrak "
               "houses, food and clothing burning; traffic arriving from Stalingrad. https://stalingrad1942-1943.ru/node/2723",
        road_point=[r3(v) for v in road], road_t_m=GUMRAK_ROAD_T_M,
        buildings=[i["id"] for i in instances],
        pads=[dict(id=pl["id"], pad_y=pl["pad_y"]) for pl in plots],
        caveat="A route composite: the site stands on the act's route west, not at Gumrak station; its layout, "
               "buildings and elapsed time are reconstruction choices. The separate Karpovka-road observation "
               "(deliberate burning, ready weapons) is not part of it.")
    return instances, plots, record


def build_all(args):
    src = Source(ROOT)
    for key, path in ASSET.items():
        if not (ROOT / path).is_file():
            raise SystemExit(f"asset {key} missing on disk: {path}")
    rust = read_rust_constants()
    rust_limits = read_rust_limits()
    grid = Grid(src.res, src.spacing)
    L = build_layout(src, rust, grid)
    H = compose_terrain(L, src)
    D = dress(L, H, src)
    instances = building_instances(L)
    gumrak_instances, gumrak_plots, gumrak_record = gumrak_site(L, grid, H)
    instances = instances + gumrak_instances
    L.gumrak_plots = gumrak_plots
    L.gumrak_record = gumrak_record
    errors = []
    for inst in instances:
        errs = within_limits(inst, rust_limits[inst["typology"]]) + [f"design: {e}" for e in within_limits(inst, DESIGN_LIMITS_AS_RUST[inst["typology"]])]
        if errs:
            errors.append((inst["id"], errs))
    if errors:
        raise SystemExit("building instances outside src/building_grammar.rs limits():\n"
                         + "\n".join(f"  {i}: {e}" for i, e in errors[:20]))
    return src, rust, rust_limits, grid, L, H, D, instances


DESIGN_LIMITS_AS_RUST = {t: dict(width=v["width"], depth=v["depth"], storeys=v["storeys"], bay=v["bay"], cellar=v["cellar"])
                         for t, v in DESIGN_LIMITS.items()}


def frontage_occupancy(L):
    """Share of every block side's street line with a building's facade on it, sampled every metre
    (a building of either range at a corner counts on both streets): per side class over all sides,
    and long and short sides apart. 'planned' counts razed plots (the plan R1 §4's recipes describe),
    'standing' only what stands; 'closed' adds yard walls, gates and fences (a continuous street
    wall). Clear frontage leaves out the mission keep-outs, the old road, the rail edge and the
    courtyard, where nothing may be built."""
    street_b = [b for b in L.buildings if b["kind"] in ("street", "slab")]
    acc = {}
    for blk in L.blocks:
        longs = long_sides(blk)
        own = [b for b in street_b if b["block"] == blk["id"]]
        gaps = [g for g in L.gaps if g["block"] == blk["id"] and g["kind"] in ("wall", "gate", "fence")]
        for side in SIDES:
            cls = blk["side_cls"][side]
            if cls is None:
                continue
            lo, hi, fp, yaw = side_frame(blk["rect"], side)
            for p in np.arange(lo + 0.5, hi, 1.0):
                probe = fp(float(p) - 0.25, 0.5, 0.5, 0.1)
                planned = any(rect_overlap(probe, b["rect"], 0.0) for b in own)
                standing = any(rect_overlap(probe, b["rect"], 0.0) for b in own if not b["razed"])
                closed = planned or any(g["side"] == side and g["along"][0] <= p <= g["along"][1] for g in gaps)
                clear = L.placer.site_reason(fp(float(p) - 0.5, 1.0, 8.0, 0.0)) in ("", "steep", "relief")
                for key in (cls, f"{cls}.{'long' if side in longs else 'short'}"):
                    a = acc.setdefault(key, [0, 0, 0, 0, 0])
                    a[0] += 1
                    a[1] += clear
                    a[2] += planned
                    a[3] += standing
                    a[4] += closed
    out = {}
    for k, a in sorted(acc.items()):
        cls = k.split(".")[0]
        out[k] = dict(frontage_m=a[0], clear_m=a[1], planned=r3(a[2] / a[0]), planned_of_clear=r3(a[2] / max(a[1], 1)),
                      standing=r3(a[3] / a[0]), standing_of_clear=r3(a[3] / max(a[1], 1)), closed=r3(a[4] / a[0]),
                      r1_range=list(RECIPES[cls]["occupancy"]))
    return out


def facade_to_facade(L):
    """Facade-to-facade widths across every street, every 2 m along it where a standing building's
    facade stands on both building lines (R1 §0 / §5.2: 17.6-29 m between pre-1917 facades; lanes
    8-14 m); both_sides is the share of the street's length so enclosed."""
    st_b = [b for b in L.buildings if not b["razed"] and b["kind"] in ("street", "slab")]
    out = []
    for s in L.streets:
        lo_line, hi_line = s["c"] - s["w"] / 2, s["c"] + s["w"] / 2
        widths, n = [], 0
        for t in np.arange(s["lo"] + 1.0, s["hi"], 2.0):
            n += 1
            near, far = None, None
            for b in st_b:
                x0, x1, z0, z1 = b["rect"]
                if s["axis"] == "ns":
                    if not (z0 <= t <= z1):
                        continue
                    if abs(x1 - lo_line) < 0.5:
                        near = x1
                    if abs(x0 - hi_line) < 0.5:
                        far = x0
                else:
                    if not (x0 <= t <= x1):
                        continue
                    if abs(z1 - lo_line) < 0.5:
                        near = z1
                    if abs(z0 - hi_line) < 0.5:
                        far = z0
            if near is not None and far is not None:
                widths.append(far - near)
        w = np.array(widths) if widths else np.array([0.0])
        out.append(dict(street=s["id"], cls=s["cls"], carriageway_m=s["w"], both_sides=r3(len(widths) / max(n, 1)),
                        facade_to_facade_m=[r3(w.min()), r3(np.median(w)), r3(w.max())] if widths else None))
    return out


def measurements(L, D, H, instances, rust_limits):
    st = [b for b in L.buildings if not b["razed"]]
    n = len(st)
    main = [b for b in st if b["kind"] in MAIN_KINDS]
    by_count, by_area = {}, {}
    area_total = sum(b["width_m"] * b["depth_m"] for b in main)
    for t in TYPOLOGIES:
        grp = [b for b in main if b["typology"] == t]
        nm = len(main)
        by_count[t] = dict(n=len(grp), share=r3(len(grp) / nm), r1_range=list(SHARE_BY_COUNT[t]),
                           within=SHARE_BY_COUNT[t][0] - 1e-9 <= len(grp) / nm <= SHARE_BY_COUNT[t][1] + 1e-9)
        a = sum(b["width_m"] * b["depth_m"] for b in grp)
        by_area[t] = dict(m2=r3(a), share=r3(a / area_total), r1_range=list(SHARE_BY_AREA[t]),
                          within=SHARE_BY_AREA[t][0] - 1e-9 <= a / area_total <= SHARE_BY_AREA[t][1] + 1e-9)
    cond = {}
    for t in TYPOLOGIES:
        grp = [b for b in st if b["typology"] == t]
        cond[t] = {c: dict(n=sum(1 for b in grp if b["condition"] == c),
                           share=r3(sum(1 for b in grp if b["condition"] == c) / max(len(grp), 1)),
                           r1=CONDITION_SHARES[t][c]) for c in CONDITIONS}
    masonry_plots = sum(1 for b in L.buildings if b["typology"] in MASONRY)
    razed = sum(1 for b in L.buildings if b["razed"])
    occupancy = frontage_occupancy(L)
    outb = [b for b in st if b["kind"] == OUTBUILDING]
    cut = sorted(abs(b["pad"] - b["ideal"]) for b in st)
    flat = pad_flatness(L, H)
    worst = max(flat)
    modules = sum(module_estimate(b) for b in st)
    return dict(
        buildings=dict(standing=n, main_buildings=len(main), outbuildings=len(outb), razed_plots=razed,
                       masonry_plots=masonry_plots, razed_share_of_masonry=r3(razed / masonry_plots),
                       yard_workshops=sum(1 for b in st if b["kind"] == "yard"),
                       slabs=sum(1 for b in st if b["kind"] == "slab"),
                       note="shares by count and area are over the main buildings (street fronts, slabs and "
                            "yard workshops, R1 §4); summer kitchens and yard cottages are yard contents, "
                            "counted apart"),
        density_pass=L.density, dense_frontage_left_open_m={k: r3(v) for k, v in sorted(L.dense_open.items())},
        battle_traces=getattr(D, "battle", {}),
        typology_by_count=by_count, typology_by_area=by_area, conditions=cond, frontage_occupancy=occupancy,
        facade_to_facade=facade_to_facade(L),
        pads=dict(count=n, tolerance_m=PAD_TOLERANCE_M, worst_flatness_m=r3(worst[0]), worst_id=worst[1],
                  cut_fill_vs_smoothed_ground_m=dict(median=r3(cut[len(cut) // 2]), p90=r3(cut[int(len(cut) * 0.9)]),
                                                    max=r3(cut[-1])), neighbour_step_m=PAD_STEP_M, apron_m=APRON_M),
        terrain=L.terrain_stats,
        ops=dict(total=len(D.ops), by_class=dict(sorted(D.counts.items())), hard_objects=len(D.hard), budget=OP_BUDGET,
                 wire_largest_drop_between_attachments_m=r3(getattr(D, "wire_drop_max", 0.0)),
                 wire_spans_left_down_steep=getattr(D, "wires_skipped_steep", 0)),
        module_estimate=dict(total=modules, note="rough upper bound for TLAS budgeting (module_estimate)"),
        open_ground=open_ground_stats(L),
        clear_lanes=clear_lanes(L, D),
        craters=dict(count=len(L.craters), in_district=sum(1 for c in L.craters if c["inside"]),
                     density_target_per_km2=pv("crater.density_per_km2_resolved")),
    )


def write_outputs(args, src, rust, rust_limits, grid, L, H, D, instances):
    dest = Path(args.dest) if args.dest else ROOT / DEST_REL
    if not dest.is_absolute():
        dest = ROOT / dest
    if dest.resolve() == (ROOT / SOURCE_REL).resolve():
        raise SystemExit("refusing to write into the source workshop")
    guard_existing(dest, args.force)
    dest.mkdir(parents=True, exist_ok=True)
    tex_rel = None
    tex_stats = None
    tex_path = dest / TEX_NAME
    if args.texture:
        im, tex_stats = build_ground_texture(L, D, src)
        im.save(tex_path, format="PNG", compress_level=6)
        tex_rel = rel(tex_path)
    elif tex_path.exists():
        tex_path.unlink()   # this tool's own earlier opt-in output; the manifest no longer names it
    # manifest: the courtyard's, byte for byte, except the terrain texture path
    raw = (src.dir / "manifest.v1.json").read_text()
    if tex_rel:
        old = json.dumps(src.manifest["terrain"]["texture"])
        if raw.count(old) != 1:
            raise SystemExit("cannot locate the terrain texture path in the source manifest")
        raw = raw.replace(old, json.dumps(tex_rel))
    (dest / "manifest.v1.json").write_text(raw)
    shutil.copyfile(src.dir / "level_design.v1.json", dest / "level_design.v1.json")
    np.save(dest / "heights.npy", H.astype(np.float32))
    edits = json.loads((src.dir / "manifest_edits.json").read_text())
    sets, moved = redrape_objects(L, src, H, edits["ops"])
    edits["ops"] = edits["ops"] + sets + D.ops
    edits["building_instances"] = instances
    if "environment_lighting" in edits:
        edits["environment_lighting"] = district_lighting(edits["environment_lighting"])
    meas = measurements(L, D, H, instances, rust_limits)
    review = encounter_review(L, D, H, src)
    street = street_encounter(L, D)
    edits[STREET_ENCOUNTER_KEY] = street
    edits[EDITS_KEY] = dict(
        tool=TOOL, tool_version=TOOL_VERSION, seed=SEED, source=SOURCE_REL,
        preserved_source_ops=len(json.loads((src.dir / "manifest_edits.json").read_text())["ops"]),
        redraped_base_placements=len(sets), redraped_source_ops=moved, district_ops=len(D.ops),
        building_instances=len(instances), receipt=RECEIPT_NAME,
        provenance="R1 STALINGRAD_URBAN_FABRIC_20260928.md; P PHOTO_REFERENCE_ANALYSIS_20260928.md (see receipt)")
    edits_text = dumps(edits)
    (dest / "manifest_edits.json").write_text(edits_text)
    feats = dict(src.features)
    feats[EDITS_KEY] = features_block(L, D)
    feats[EDITS_KEY]["encounter_review"] = review
    feats[EDITS_KEY]["street_encounter"] = street_encounter_summary(street)
    (dest / "features.json").write_text(dumps(feats, indent=1))
    receipt = dict(
        schema="kessel.urban-intro-district.receipt", version=1, tool=TOOL, tool_version=TOOL_VERSION, seed=SEED,
        command="python3 " + TOOL + (" --texture" if args.texture else ""),
        research=dict(R1=R1_DOC, P=P_DOC, location=RESEARCH_DIR,
                      reference_photographs="/home/alex/panzer/reference/wartime_photos_20260928/ (ids [FF-NN] as in P)"),
        source=dict(dir=SOURCE_REL, sha256={f: sha256_file(src.dir / f) for f in
                                            ("manifest.v1.json", "manifest_edits.json", "heights.npy", "features.json",
                                             "level_design.v1.json")},
                    texture=src.manifest["terrain"]["texture"]),
        outputs=dict(dir=rel(dest), sha256={f: sha256_file(dest / f) for f in OUTPUT_FILES + [TEX_NAME]
                                            if (dest / f).exists()}),
        extent=L.extent,
        parameters=PARAMS, recipes=RECIPES, sizes={t: {k: (v if not isinstance(v, dict) else {str(a): b for a, b in v.items()})
                                                      for k, v in SIZES[t].items()} for t in SIZES},
        condition_shares=CONDITION_SHARES, design_limits=DESIGN_LIMITS, rust_limits=rust_limits,
        mission=dict(spawn=list(src.spawn), house=list(rust["house"]), wreck=list(rust["wreck"]),
                     withdraw=list(rust["withdraw"]), courtyard_pad=list(src.courtyard_pad),
                     courtyard_keep=[r3(v) for v in L.courtyard_keep], map_sites=L.map_sites),
        courtyard=dict(preserved_source_ops=edits[EDITS_KEY]["preserved_source_ops"],
                       redraped_base_placements=len(sets), redraped_source_ops=moved),
        measurements=meas, texture=tex_stats,
        provenance=PROVENANCE, limitations=LIMITATIONS,
        encounter_review=review, street_encounter=street_encounter_summary(street),
        conflicts_with_research=[
            "P L1 (open ground of 50-250 m between ruins in [04-01] [06-12]) against R1 §4 frontage occupancy and "
            "§6.2's 5-15 % razed: R1's occupancy is kept, the razed share takes R1's upper bound (15 %) and is "
            "concentrated north and west and round the square; burned timber plots, razed plots, yards and the "
            "square supply the open ground (measurements.open_ground).",
            "R1 main street 22-30 m against P L3 18-22 m (+/-25 %): 24 m, inside both.",
            "R1 recommended merchant widths 12-40 m and apartment storeys 3-6 against the contract's provisional "
            "limits (merchant 8-24 m, 1-2 storeys; apartment 16-64 m, 3-5 storeys): instances use the intersection "
            "(DESIGN_LIMITS) and pass the limits parsed from src/building_grammar.rs."],
    )
    (dest / RECEIPT_NAME).write_text(dumps(receipt))
    return dest, meas, review, street, edits_text


def guard_existing(dest, force):
    """Never overwrite hand edits: an existing workshop is replaced only if it is this tool's
    unmodified output (its manifest_edits.json matches the receipt), or with --force."""
    edits = dest / "manifest_edits.json"
    if not edits.exists() or force:
        return
    receipt = dest / RECEIPT_NAME
    try:
        rec = json.loads(receipt.read_text())
        expected = rec["outputs"]["sha256"]["manifest_edits.json"]
    except (OSError, KeyError, ValueError):
        raise SystemExit(f"{dest} exists and is not this tool's output; pass --force to replace it")
    if sha256_file(edits) != expected:
        raise SystemExit(f"{edits} was edited after generation; keep it or pass --force to regenerate")


def write_review(dest, edits_text):
    """A review-only sibling identical to the workshop but for its environment_lighting: an overcast
    January midday, for renders that are compared against daylight photographs. Not for play."""
    review = dest.parent / (dest.name + REVIEW_SUFFIX)
    review.mkdir(parents=True, exist_ok=True)
    for f in ("manifest.v1.json", "heights.npy", "features.json", "level_design.v1.json"):
        shutil.copyfile(dest / f, review / f)
    edits = json.loads(edits_text)
    edits["environment_lighting"] = dict(REVIEW_LIGHTING)
    edits[EDITS_KEY]["review_variant"] = ("review-only copy of " + rel(dest) + ": identical but environment_lighting "
                                         "(overcast January midday) for evidence renders")
    (review / "manifest_edits.json").write_text(dumps(edits))
    return review


SAMPLE_BLOCKS = ("c2r1", "c3r1", "c0r1")


def sample_document(L, instances):
    keep = [i for i in instances if i["id"].split(".")[1] in SAMPLE_BLOCKS]
    return {
        "schema": "urban-intro-district-sample/1",
        "generated_by": TOOL + " --write-sample",
        "note": ("A few blocks of the generated urban-intro district (a core block on the tram street, the "
                 "courtyard block and a timber-fringe block), in the manifest_edits.json building_instances "
                 "format. Validated by src/urban_district_sample_tests.rs against building_placement and "
                 "building_grammar::check_spec; regenerated identically by tools/worldgen/test_urban_intro_district.py."),
        "blocks": [dict(id=b["id"], rect=[r3(v) for v in b["rect"]], side_class=b["side_cls"])
                   for b in L.blocks if b["id"] in SAMPLE_BLOCKS],
        "streets": [dict(id=s["id"], rect=[r3(v) for v in s["rect"]]) for s in L.streets],
        "building_instances": keep,
    }


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dest", default=None, help="workshop directory (default: %s)" % DEST_REL)
    ap.add_argument("--plan", default=None, help="also write the plan image here")
    ap.add_argument("--review", action="store_true", help="also write the daylight review variant")
    ap.add_argument("--texture", action="store_true",
                    help="also fork the ground texture (spring shading and offline previews only: Act 1's "
                         "winter terrain shading never samples the drape)")
    ap.add_argument("--write-sample", action="store_true", help="refresh %s" % SAMPLE_REL)
    ap.add_argument("--force", action="store_true", help="replace a hand-edited workshop")
    args = ap.parse_args(argv)
    src, rust, rust_limits, grid, L, H, D, instances = build_all(args)
    dest, meas, review, street, edits_text = write_outputs(args, src, rust, rust_limits, grid, L, H, D, instances)
    if args.review:
        rv = write_review(dest, edits_text)
        print("review variant:", rel(rv))
    if args.write_sample:
        (ROOT / SAMPLE_REL).parent.mkdir(parents=True, exist_ok=True)
        (ROOT / SAMPLE_REL).write_text(dumps(sample_document(L, instances)))
        print("sample:", SAMPLE_REL)
    if args.plan:
        render_plan(L, src, args.plan, H=H, extras=plan_overlay(L, D, review, street))
        print("plan:", args.plan)
    b = meas["buildings"]
    print(f"workshop: {rel(dest)}")
    print(f"  extent x {L.extent['x0']:.0f}..{L.extent['x1']:.0f} z {L.extent['z0']:.0f}..{L.extent['z1']:.0f}; "
          f"{b['standing']} building instances, {b['razed_plots']} razed plots; {meas['ops']['total']} dressing ops; "
          f"pads flat to {meas['pads']['worst_flatness_m']} m")
    for g in review["at_gun_sites"]:
        print(f"  45 mm gun {g['index'] + 1} at {g['site']}: conflict={g['conflict']} "
              f"blocked_by={g['line_of_fire_to_route_mid_blocked_by'][:4]}")


if __name__ == "__main__":
    main()
