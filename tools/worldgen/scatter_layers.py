#!/usr/bin/env python3
"""Scatter layers for a workshop: the authored density and rule layers the engine's strewn field
(src/scatter_field.rs, docs/SCATTER.md) places its hundreds of thousands of objects from.

The tool does not place objects. It writes, beside the workshop manifest:

  scatter.v1.json          groups (which meshes stand for what), sources (where the map's human
                           events are: road segments, craters, wrecks, guns, trees, obstacles,
                           fighting sites, authored bodies, buildings) and rules (what each
                           source kind sheds around itself, at what density, orientation, tier)
  scatter_ground.v1.png    ground-class raster (R: 0 snow, 1 road, 2 rubble, 3 scorch, 5 bare
                           rock; G: snow cover 0..255) at a fixed texel pitch over the terrain

The engine then places deterministically per 32 m cell from these layers and the heightfield, so
a run is reproducible from the seed in this file.

    python tools/worldgen/scatter_layers.py [--root PATH] [--dest WORKSHOP] [--seed N] [--density F]
    python tools/worldgen/scatter_layers.py --estimate      # expected counts per rule, nothing written

Sources are derived from the workshop's own authoring (the generated base manifest, the wreckage
overlay's scenes and placements, the course route, the population clusters, the building
instances); nothing here invents a site. Provenance is recorded in the file.
"""
import argparse
import collections
import json
import math
import sys
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from canyon_v2_wreckage import ASSETS as WRECK_ASSETS, Workshop  # noqa: E402

VERSION = 1
TOOL = 'tools/worldgen/scatter_layers.py'
DEFAULT_SEED = 0x20261027
DEST = 'assets/worldgen/workshops/gaea-canyon-run-v2'
WIND = (-0.60, 0.80)
LANE_HALF_M = 12.0
ROAD_HALF_M = 4.2
GROUND_RES = 2048

MP2 = 'assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/'
MP3 = 'assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/'
NAT = 'assets/pizza/_extracted/PSX Nature v1.7.3/PSX Nature (Tree branches separated)/Models/GLB (recommended)/'
BF = 'assets/authoring/battlefield/'
CV = 'assets/authoring/canyon-v2/'
SC = 'assets/authoring/scatter/'
HIST = 'assets/authoring/historical_20260925/'
SOV = 'assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/versions/20260905-user-384ec50b-h175/baked/'
GER = 'assets/flavor/Characters/german-ww2-soldier-cc0/baked/'


def g(asset, w=1.0, tier='small', pushable=False, sink=0.0, conform=1.0, flat=False, casualty=False, lay=None,
      classes=None, cover_height=None):
    """One group entry. `sink` is a fraction of the mesh's height buried below the contact; `lay`
    turns a standing character bake onto the ground ([axis, degrees]); `flat` marks a decal."""
    e = dict(asset=asset, w=w, tier=tier)
    if pushable:
        e['pushable'] = True
    if sink:
        e['sink'] = sink
    if conform != 1.0:
        e['conform'] = conform
    if flat:
        e['flat'] = True
    if casualty:
        e['casualty'] = True
    if lay:
        e['lay'] = lay
    if classes:
        e['classes'] = classes
    return e


def groups():
    """Named prototype sets. Weights are relative within a group. Anachronisms in the kit (cell
    phones, keycards, cassettes, dosimeters, modern pistols) are deliberately absent."""
    cases_small = [g(SC + 'case_792.glb', 5, sink=0.35), g(SC + 'case_762.glb', 4, sink=0.35),
                   g(MP3 + 'Items & Weapons/ammo_pistol_loose_mp_1_casing.glb', 1, sink=0.3),
                   g(MP3 + 'Items & Weapons/ammo_pistol_loose_mp_2_casing.glb', 1, sink=0.3),
                   g(MP3 + 'Items & Weapons/ammo_pistol_loose_1pc_mp_1.glb', 0.5, sink=0.3),
                   g(MP3 + 'Items & Weapons/ammo_pistol_loose_3pcs_mp_1.glb', 0.3, sink=0.3)]
    kit = [g(MP2 + 'Props/luggage_mp_1.glb', 2, sink=0.12), g(MP2 + 'Props/package_hr_1.glb', 1.5, sink=0.12),
           g(MP2 + 'Props/toolbox_mx_1.glb', 0.6, sink=0.1, pushable=True),
           g(MP2 + 'Props/jerrycan_mx_1.glb', 1.5, sink=0.1, pushable=True), g(MP3 + 'Small Props/jerrycan_1.glb', 1, sink=0.1, pushable=True),
           g(MP2 + 'Props/bucket_mx_1.glb', 0.8, sink=0.1, pushable=True), g(MP2 + 'Props/bucket_mx_2.glb', 0.5, sink=0.1, pushable=True),
           g(MP2 + 'Props/bucket_mx_3.glb', 0.5, sink=0.1, pushable=True),
           g(MP3 + 'Items & Weapons/canned_food_mp_1.glb', 1.5, sink=0.25), g(MP3 + 'Items & Weapons/canned_food_mp_2.glb', 1.5, sink=0.25),
           g(MP3 + 'Items & Weapons/canned_food_mp_3.glb', 1, sink=0.25), g(MP3 + 'Items & Weapons/canned_food_mp_5_rusty.glb', 1.5, sink=0.25),
           g(MP3 + 'Small Props/tin_can_mp_1_rusty.glb', 2, sink=0.25), g(MP3 + 'Small Props/tin_can_mp_1_rusty_1.glb', 2, sink=0.25),
           g(MP3 + 'Items & Weapons/glass_bottle_1.glb', 1, sink=0.3), g(MP3 + 'Items & Weapons/glass_bottle_2.glb', 1, sink=0.3),
           g(MP2 + 'Props/glass_bottle_mx_1.glb', 1, sink=0.3), g(MP2 + 'Weapons & Tools/glass_bottle_mx_3_broken.glb', 0.8, sink=0.3),
           g(MP3 + 'Small Props/plate_mp_1.glb', 1, sink=0.3), g(MP3 + 'Small Props/spoon_mp_1.glb', 1, sink=0.3),
           g(MP2 + 'Props/first_aid_kit_hr_1.glb', 0.4, sink=0.15), g(MP2 + 'Props/first_aid_kit_hr_2.glb', 0.4, sink=0.15),
           g(MP3 + 'Small Props/book_mp_1.glb', 0.5, sink=0.2),
           g(MP3 + 'Small Props/book_mp_4.glb', 0.5, sink=0.2), g(MP3 + 'Small Props/open_book_mp_1.glb', 0.4, sink=0.2),
           g(MP3 + 'Items & Weapons/notebook_1.glb', 0.5, sink=0.2),
           g(MP2 + 'Props/cardboard_box_1.glb', 1, sink=0.15), g(MP2 + 'Props/cardboard_box_2.glb', 1, sink=0.15),
           g(MP2 + 'Props/paint_can_mx_1.glb', 0.3, sink=0.2), g(MP3 + 'Items & Weapons/cigs_packet_1.glb', 0.8, sink=0.2),
           g(MP3 + 'Items & Weapons/matchbox_1.glb', 0.6, sink=0.2), g(MP3 + 'Items & Weapons/flashlight_1.glb', 0.3, sink=0.3),
           g(MP2 + 'Weapons & Tools/shovel_mx_1.glb', 0.8, sink=0.2), g(MP2 + 'Weapons & Tools/pick_axe_mx_1.glb', 0.4, sink=0.2),
           g(MP2 + 'Weapons & Tools/axe_mx_1.glb', 0.4, sink=0.2), g(MP3 + 'Items & Weapons/hand_saw_1.glb', 0.3, sink=0.2),
           g(MP2 + 'Weapons & Tools/hammer_mx_1.glb', 0.4, sink=0.3), g(MP2 + 'Weapons & Tools/wrench_mx_1.glb', 0.4, sink=0.3),
           g(MP2 + 'Weapons & Tools/misc_tool_mx_1.glb', 0.3, sink=0.3),
           g(MP2 + 'Props/wooden_board_1.glb', 1, sink=0.2, pushable=True), g(MP2 + 'Props/wooden_board_3.glb', 1, sink=0.2, pushable=True),
           g(MP2 + 'Props/wooden_plank_2.glb', 1, sink=0.2, pushable=True), g(MP2 + 'Props/wooden_plank_4.glb', 1, sink=0.2, pushable=True),
           g(MP2 + 'Props/tire_1.glb', 0.5, sink=0.2, pushable=True), g(MP2 + 'Props/tire_2.glb', 0.3, sink=0.2, pushable=True),
           g(SC + 'mess_tin.glb', 1.5, sink=0.25), g(SC + 'canteen.glb', 1.5, sink=0.25), g(SC + 'gas_mask_canister.glb', 1.2, sink=0.3),
           g(BF + 'greatcoat_heap_a.glb', 1.2, sink=0.1), g(BF + 'greatcoat_heap_b.glb', 1.2, sink=0.1),
           g(HIST + 'gumrak_scorched_luggage_d2bd048056bf.glb', 0.8, sink=0.1)]
    crates = [g(MP2 + 'Props/wooden_crate_2_a.glb', 2, 'medium', sink=0.1, pushable=True),
              g(MP2 + 'Props/wooden_crate_2_b.glb', 2, 'medium', sink=0.1, pushable=True),
              g(MP2 + 'Props/wooden_crate_2_c.glb', 1, 'medium', sink=0.1, pushable=True),
              g(MP2 + 'Props/wooden_crate_1.glb', 1, 'medium', sink=0.1, pushable=True),
              g(MP2 + 'Props/wooden_crate_3.glb', 1, 'medium', sink=0.1, pushable=True),
              g(MP3 + 'Large Props/wooden_crate_4.glb', 0.5, 'medium', sink=0.1, pushable=True),
              g(MP3 + 'Large Props/supply_crate_1_empty.glb', 0.5, 'medium', sink=0.1, pushable=True),
              g(MP2 + 'Props/metal_barrel_hr_1.glb', 1.2, 'medium', sink=0.1, pushable=True),
              g(MP2 + 'Props/metal_barrel_hr_2.glb', 0.8, 'medium', sink=0.1, pushable=True),
              g(MP2 + 'Props/metal_barrel_hr_4.glb', 0.5, 'medium', sink=0.1, pushable=True),
              g(MP2 + 'Props/wooden_barrel_1.glb', 1, 'medium', sink=0.1, pushable=True),
              g(MP2 + 'Props/cement_bag_mp_1.glb', 0.6, sink=0.2),
              g(MP3 + 'Items & Weapons/ammo_box_mp_1.glb', 2, sink=0.15), g(MP3 + 'Items & Weapons/ammo_box_mp_2.glb', 1.5, sink=0.15),
              g(MP3 + 'Items & Weapons/ammo_box_mp_3.glb', 1, sink=0.15), g(MP3 + 'Items & Weapons/ammo_box_mp_4.glb', 1, sink=0.15),
              g(MP3 + 'Items & Weapons/ammo_box_mp_5.glb', 1, sink=0.15), g(MP3 + 'Items & Weapons/ammo_box_1.glb', 1, sink=0.15),
              g(HIST + 'gumrak_scorched_crate_acc43161a401.glb', 0.5, 'medium', sink=0.1)]
    weapons = [g(BF + 'dropped_rifle.glb', 3, sink=0.3), g(SC + 'helmet_m40.glb', 1.2, sink=0.15),
               g(SC + 'helmet_ssh40.glb', 1.2, sink=0.15), g(SC + 'stretcher.glb', 0.15, 'medium', sink=0.1),
               g(SC + 'gas_mask_canister.glb', 0.8, sink=0.3)]
    rubble = [g(MP2 + f'Debris & Misc/brick_mx_{i}.glb', 1.0, sink=0.3) for i in (1, 2, 3, 4)] + \
             [g(MP2 + f'Debris & Misc/brick_mx_{i}_{j}.glb', 0.5, sink=0.3) for i in (1, 2, 3, 4) for j in (0, 1, 2, 3)] + \
             [g(MP2 + 'Debris & Misc/bricks_stacked_mx_1.glb', 0.8, sink=0.2), g(MP2 + 'Debris & Misc/bricks_stacked_mx_2.glb', 0.8, sink=0.2),
              g(MP2 + 'Debris & Misc/bricks_stacked_mx_3.glb', 0.6, sink=0.2), g(MP2 + 'Debris & Misc/bricks_stacked_mx_4.glb', 0.6, sink=0.2),
              g(MP2 + 'Debris & Misc/debris_bricks_mx_1.glb', 1.2, sink=0.15), g(MP2 + 'Debris & Misc/debris_bricks_mx_2.glb', 1.2, sink=0.15),
              g(MP2 + 'Debris & Misc/gravel_pile_hr_1.glb', 0.3, 'large', sink=0.1), g(MP2 + 'Debris & Misc/gravel_pile_hr_2.glb', 0.3, 'large', sink=0.1),
              g(MP2 + 'Props/concrete_block_mx_1.glb', 0.4, 'medium', sink=0.15), g(MP3 + 'Small Props/concrete_block_1.glb', 0.4, 'medium', sink=0.15),
              g(SC + 'masonry_0.glb', 1, sink=0.2), g(SC + 'masonry_1.glb', 1, sink=0.2), g(SC + 'masonry_2.glb', 0.8, 'medium', sink=0.2),
              g(SC + 'masonry_3.glb', 0.5, 'large', sink=0.2),
              g(MP2 + 'Props/wooden_plank_1.glb', 0.6, sink=0.2, pushable=True), g(MP2 + 'Props/wooden_plank_3.glb', 0.6, sink=0.2, pushable=True),
              g(MP2 + 'Props/wooden_board_2.glb', 0.6, sink=0.2, pushable=True),
              g(HIST + 'gumrak_charred_plank_a48a6440323a.glb', 0.8, sink=0.2), g(SC + 'burnt_board_0.glb', 0.5, sink=0.2),
              g(SC + 'burnt_board_1.glb', 0.5, sink=0.2), g(SC + 'burnt_board_2.glb', 0.5, sink=0.2),
              g(MP2 + 'Props/pipe_mx_1.glb', 0.3, sink=0.3),
              g(MP2 + 'Props/scrap_metal_mx_1.glb', 0.5, sink=0.3), g(MP2 + 'Props/scrap_metal_mx_1_1.glb', 0.4, sink=0.3),
              g(MP2 + 'Props/scrap_metal_mx_1_2.glb', 0.4, sink=0.3),
              g(MP2 + 'Props/stool_mx_1.glb', 0.1, sink=0.1, pushable=True), g(MP2 + 'Props/bench_mx_1_1.glb', 0.08, 'medium', sink=0.1, pushable=True),
              g(MP2 + 'Large Props & Machinery/chimney_a_2_1.glb', 0.05, 'medium', sink=0.2)]
    clods = [g(SC + f'clod_{i}.glb', [4.0, 2.0, 0.7, 0.2][i], sink=0.3) for i in range(4)]
    shards = [g(SC + f'shard_{i}.glb', [2.0, 1.5, 1.0, 0.5][i], sink=0.2) for i in range(4)]
    fragments = [g(SC + f'plate_fragment_{i}.glb', 1.0, sink=0.2) for i in range(4)] + \
                [g(SC + 'track_link_tiger.glb', 0.5, sink=0.3), g(SC + 'track_link_t34.glb', 0.8, sink=0.3),
                 g(BF + 'loose_wheel.glb', 0.5, 'medium', sink=0.15, pushable=True), g(MP2 + 'Props/tire_1.glb', 0.5, sink=0.2, pushable=True),
                 g(MP2 + 'Props/gear_mx_2.glb', 0.3, sink=0.3), g(MP2 + 'Props/gear_mx_4.glb', 0.3, sink=0.3),
                 g(MP2 + 'Props/spring_mx_1_large.glb', 0.2, sink=0.3), g(MP2 + 'Props/pipe_mx_2.glb', 0.3, sink=0.3),
                 g(MP2 + 'Props/scrap_metal_mx_1.glb', 0.8, sink=0.3), g(MP2 + 'Props/scrap_metal_mx_1_2.glb', 0.6, sink=0.3),
                 g(MP2 + 'Props/bolt_mx_1.glb', 0.5, sink=0.4), g(MP2 + 'Props/hexagon_nut_mx_1.glb', 0.5, sink=0.4)] + shards[:3]
    foliage = [g(NAT + 'grass_1.glb', 2, sink=0.05, classes=['snow']), g(NAT + 'grass_2.glb', 2, sink=0.05, classes=['snow']),
               g(NAT + 'grass_3.glb', 1.5, sink=0.05, classes=['snow']), g(NAT + 'grass_4.glb', 1.5, sink=0.05, classes=['snow']),
               g(NAT + 'wheat_1.glb', 0.6, sink=0.05, classes=['snow']), g(NAT + 'wheat_1_1.glb', 0.6, sink=0.05, classes=['snow']),
               g(NAT + 'tree_log_1.glb', 0.25, 'large', sink=0.15), g(NAT + 'tree_stump_1.glb', 0.15, 'medium', sink=0.1)]
    rocks = [g(NAT + 'stone_1.glb', 1.0, 'medium', sink=0.3), g(NAT + 'stone_2.glb', 0.12, 'large', sink=0.3),
             g(NAT + 'stone_3.glb', 0.18, 'large', sink=0.3), g(NAT + 'stone_4.glb', 2.0, sink=0.3), g(NAT + 'stone_5.glb', 2.0, sink=0.3),
             g(SC + 'clod_1.glb', 1.0, sink=0.3), g(SC + 'clod_3.glb', 0.8, sink=0.3)]
    snow = [g(SC + f'snow_lump_{i}.glb', [3.0, 2.0, 0.8, 0.25][i], sink=0.2, classes=['snow']) for i in range(4)] + \
           [g(BF + 'snow_drift.glb', 0.5, sink=0.1, classes=['snow'])]
    drifts = [g(SC + 'drift_tail_short.glb', 1.5, sink=0.1, classes=['snow']), g(SC + 'drift_tail_long.glb', 1.0, sink=0.1, classes=['snow']),
              g(BF + 'snow_drift.glb', 1.0, sink=0.1, classes=['snow'])]
    blood = [g(SC + f'blood_{i}.glb', [2.0, 1.5, 1.0, 0.5][i], flat=True) for i in range(4)]
    # death_00/01 are the first frames of the fall (still upright); only the terminal frames lie.
    bodies_soviet = [g(SOV + 'soviet_ww2_soldier_death_02.glb', 1, sink=0.06, casualty=True),
                     g(SOV + 'soviet_ww2_soldier_death_03.glb', 1, sink=0.06, casualty=True)] + \
                    [g(CV + f'casualty_{i}.glb', 0.6, sink=0.06, casualty=True) for i in range(6)]
    bodies_german = [g(GER + 'german_ww2_soldier_relaxed_static.glb', 1, sink=0.06, casualty=True, lay=['x', -90]),
                     g(GER + 'german_ww2_soldier_relaxed_static.glb', 1, sink=0.06, casualty=True, lay=['z', 90]),
                     g(GER + 'infantry_crouch_v1/german_infantry_relaxed_crouch_03.glb', 0.8, sink=0.06, casualty=True, lay=['z', -90]),
                     g(GER + 'infantry_crouch_v1/german_infantry_aim_crouch_04.glb', 0.8, sink=0.06, casualty=True, lay=['z', 90]),
                     g(GER + 'infantry_crouch_v1/german_infantry_relaxed_crouch_04.glb', 0.6, sink=0.06, casualty=True, lay=['x', 90])] + \
                    [g(CV + f'casualty_{i}.glb', 0.8, sink=0.06, casualty=True) for i in range(6)]
    cases_45 = [g(SC + 'case_45mm.glb', 1, sink=0.3)]
    cases_76 = [g(SC + 'case_76mm.glb', 1, sink=0.3)]
    cases_88 = [g(SC + 'case_88mm.glb', 1, sink=0.3)]
    furniture = [g(MP2 + 'Props/stool_mx_1.glb', 1, sink=0.1, pushable=True), g(MP2 + 'Props/bench_mx_1.glb', 0.5, 'medium', sink=0.1, pushable=True),
                 g(MP2 + 'Props/shelf_mx_1.glb', 0.3, 'medium', sink=0.1), g(MP2 + 'Props/old_mattress_mx_1.glb', 0.5, 'medium', sink=0.2),
                 g(MP2 + 'Props/ladder_hr_1_short.glb', 0.3, 'medium', sink=0.1), g(MP2 + 'Props/cardboard_box_2.glb', 1, sink=0.15),
                 g(MP2 + 'Props/wooden_crate_hx_2.glb', 0.5, 'medium', sink=0.1, pushable=True)]
    barricade = [g(MP2 + 'Modular Props/barricade_a_1.glb', 1, 'large', sink=0.1), g(MP2 + 'Modular Props/barricade_b_1.glb', 1, 'large', sink=0.1),
                 g(MP2 + 'Modular Props/barricade_b_3.glb', 0.6, 'large', sink=0.1),
                 g(MP2 + 'Modular Props/fence_a_1.glb', 0.5, 'medium', sink=0.1, pushable=True),
                 g(MP3 + 'Large Props/supply_crate_1.glb', 0.6, 'large', sink=0.1), g(MP2 + 'Props/cement_bags_mp_1_pallet_1.glb', 0.5, 'large', sink=0.1)]
    return dict(cases_small=cases_small, kit=kit, crates=crates, weapons=weapons, rubble=rubble, clods=clods, shards=shards,
                fragments=fragments, foliage=foliage, rocks=rocks, snow=snow, drifts=drifts, blood=blood,
                bodies_soviet=bodies_soviet, bodies_german=bodies_german, cases_45=cases_45, cases_76=cases_76,
                cases_88=cases_88, furniture=furniture, barricade=barricade)


def rect(along, lateral, mirror=True):
    """Band in the source frame: along ∈ [hl·k0+c0, hl·k1+c1], lateral ∈ [hw·k0+c0, hw·k1+c1] on one
    side of the source (both sides when mirrored). Each bound is [k, c]."""
    return dict(shape='rect', along=along, lateral=lateral, mirror=mirror)


def annulus(r0, r1, sector=None):
    """Band r ∈ [R·k0+c0, R·k1+c1] around the source; `sector` = [centre_deg relative to the
    source heading, half_angle_deg] limits it to a wedge."""
    b = dict(shape='annulus', r0=r0, r1=r1)
    if sector:
        b['sector'] = sector
    return b


def rule(source, group, density, band, orient='random', jitter=180.0, scale=(0.9, 1.1), slope_max=0.55, side=None,
         classes=None, companions=None, lane_clear=False, min_intensity=0.0, name=None):
    r = dict(source=source, group=group, density=density, band=band, orient=orient, jitter_deg=jitter, scale=list(scale),
             slope_max=slope_max)
    if side:
        r['side'] = side
    if classes:
        r['classes'] = classes
    if companions:
        r['companions'] = companions
    if lane_clear:
        r['lane_clear'] = True
    if min_intensity:
        r['min_intensity'] = min_intensity
    r['name'] = name or f'{source}.{group}'
    return r


BODY_COMPANIONS = [dict(group='blood', count=[1, 2], radius=0.9), dict(group='kit', count=[0, 2], radius=1.6),
                   dict(group='weapons', count=[0, 1], radius=1.2), dict(group='cases_small', count=[0, 6], radius=1.4)]
GERMAN_GUN_COMPANIONS = None


def rules():
    R = []
    # ── Roads: the retreat's litter on the shoulders, cases where columns were strafed, kit thrown
    # from vehicles; drifts and snow lumps on the verges; the dead who sat down beside the road.
    R += [rule('road', 'cases_small', 1.2, rect([[1, 0], [1, 0]], [[0, 0], [0, 6]]), name='road.cases'),
          rule('road', 'kit', 0.35, rect([[1, 0], [1, 0]], [[0, 0], [0, 9]]), name='road.kit'),
          rule('road', 'kit', 0.04, rect([[1, 0], [1, 0]], [[-1, 0], [0, 0]]), name='road.kit_on_road'),
          rule('road', 'cases_small', 0.3, rect([[1, 0], [1, 0]], [[-1, 0], [0, 0]]), name='road.cases_on_road'),
          rule('road', 'crates', 0.05, rect([[1, 0], [1, 0]], [[0, 0.5], [0, 9]]), lane_clear=True, name='road.crates'),
          rule('road', 'snow', 0.12, rect([[1, 0], [1, 0]], [[0, 1], [0, 10]]), scale=(0.5, 1.1), classes=['snow'], name='road.snow'),
          rule('road', 'rubble', 0.3, rect([[1, 0], [1, 0]], [[0, 0], [0, 7]]), name='road.rubble'),
          rule('road', 'fragments', 0.05, rect([[1, 0], [1, 0]], [[0, 0], [0, 8]]), name='road.fragments'),
          rule('road', 'weapons', 0.06, rect([[1, 0], [1, 0]], [[0, 0], [0, 9]]), name='road.weapons'),
          rule('road', 'bodies_german', 0.009, rect([[1, 0], [1, 0]], [[0, 1.5], [0, 9]]), orient='along', jitter=35,
               companions=BODY_COMPANIONS, lane_clear=True, name='road.bodies_german'),
          rule('road', 'bodies_soviet', 0.004, rect([[1, 0], [1, 0]], [[0, 1.5], [0, 9]]), orient='along', jitter=50,
               companions=BODY_COMPANIONS, lane_clear=True, name='road.bodies_soviet')]
    # ── Craters: frozen earth and splinters fan outward, snow thrown clear; the occasional dead.
    R += [rule('crater', 'clods', 0.9, annulus([1, 0], [4.5, 0]), orient='radial', jitter=25, scale=(0.45, 1.0), name='crater.clods'),
          rule('crater', 'shards', 0.45, annulus([0.5, 0], [6, 0]), orient='radial', jitter=30, name='crater.shards'),
          rule('crater', 'snow', 0.25, annulus([1, 0], [5, 0]), orient='radial', jitter=40, scale=(0.5, 1.1), classes=['snow'], name='crater.snow'),
          rule('crater', 'kit', 0.15, annulus([0.5, 0], [4, 0]), orient='radial', jitter=60, name='crater.kit'),
          rule('crater', 'bodies_german', 0.005, annulus([0.8, 0], [3.5, 0]), orient='radial', jitter=40, companions=BODY_COMPANIONS,
               lane_clear=True, name='crater.bodies_german'),
          rule('crater', 'bodies_soviet', 0.003, annulus([0.8, 0], [3.5, 0]), orient='radial', jitter=40, companions=BODY_COMPANIONS,
               lane_clear=True, name='crater.bodies_soviet')]
    # ── Scorch: burnt boards and splinters on the ash.
    R += [rule('scorch', 'rubble', 0.5, annulus([0, 0], [2, 0]), name='scorch.rubble'),
          rule('scorch', 'shards', 0.3, annulus([0, 0], [2.5, 0]), name='scorch.shards')]
    # ── Wrecks: what came off and out of them, the crew that bailed, drifts in their lee (obstacle).
    R += [rule('wreck', 'fragments', 0.5, annulus([1, 0.3], [1, 7]), orient='radial', jitter=60, lane_clear=True, name='wreck.fragments'),
          rule('wreck', 'kit', 0.6, annulus([1, 0.2], [1, 5]), lane_clear=True, name='wreck.kit'),
          rule('wreck', 'crates', 0.06, annulus([1, 0.4], [1, 5]), lane_clear=True, name='wreck.crates'),
          rule('wreck', 'bodies_german', 0.006, annulus([1, 0], [1, 6]), orient='radial', jitter=45, side='german',
               companions=BODY_COMPANIONS, lane_clear=True, name='wreck.bodies_german'),
          rule('wreck', 'bodies_soviet', 0.006, annulus([1, 0], [1, 6]), orient='radial', jitter=45, side='soviet',
               companions=BODY_COMPANIONS, lane_clear=True, name='wreck.bodies_soviet'),
          rule('wreck', 'snow', 0.15, annulus([1, 0.2], [1, 4]), scale=(0.5, 1.1), classes=['snow'], name='wreck.snow')]
    # ── Guns: the fired cases where the piece stood, boxes beside it.
    R += [rule('gun', 'cases_45', 3.0, annulus([0, 1.0], [0, 6.0], sector=[150, 55]), side='45mm', name='gun.cases_45'),
          rule('gun', 'cases_76', 2.0, annulus([1, 0.3], [1, 3.5], sector=[90, 60]), side='76mm', name='gun.cases_76'),
          rule('gun', 'cases_88', 1.5, annulus([1, 0.3], [1, 3.5], sector=[-90, 60]), side='88mm', name='gun.cases_88'),
          rule('gun', 'crates', 0.25, annulus([0, 1.5], [0, 7.0], sector=[180, 70]), lane_clear=True, name='gun.boxes'),
          rule('gun', 'cases_small', 1.5, annulus([0, 0.5], [0, 8.0]), name='gun.cases_small')]
    # ── Trees: boughs, dry grass through the snow, stones and lumps at the trunk.
    R += [rule('tree', 'foliage', 1.2, annulus([0, 0.3], [1.2, 0]), scale=(0.7, 1.3), name='tree.foliage'),
          rule('tree', 'rocks', 0.35, annulus([0, 0], [1.5, 0]), scale=(0.6, 1.4), name='tree.rocks'),
          rule('tree', 'snow', 0.12, annulus([0, 0], [1.5, 0]), scale=(0.5, 1.1), classes=['snow'], name='tree.snow')]
    # ── Obstacles: drifts and lumps form in the lee of anything that breaks the wind.
    R += [rule('obstacle', 'drifts', 0.12, rect([[1, 0], [1, 0]], [[1, 0.2], [1, 3.5]], mirror=False), orient='wind', jitter=12,
               classes=['snow'], name='obstacle.drifts'),
          rule('obstacle', 'snow', 0.4, rect([[1, 0.5], [1, 0.5]], [[1, 0.1], [1, 3.0]], mirror=False), orient='wind', jitter=40, scale=(0.5, 1.1),
               classes=['snow'], name='obstacle.snow')]
    # ── Fighting: bodies oriented by the direction of flight, cases, dropped weapons and helmets,
    # the clods and splinters of shellfire.
    R += [rule('fighting', 'bodies_german', 0.012, annulus([0, 0], [1, 0]), orient='flight', jitter=35, side='german',
               companions=BODY_COMPANIONS, lane_clear=True, name='fighting.bodies_german'),
          rule('fighting', 'bodies_soviet', 0.012, annulus([0, 0], [1, 0]), orient='flight', jitter=35, side='soviet',
               companions=BODY_COMPANIONS, lane_clear=True, name='fighting.bodies_soviet'),
          rule('fighting', 'cases_small', 0.4, annulus([0, 0], [1.2, 0]), name='fighting.cases'),
          rule('fighting', 'weapons', 0.08, annulus([0, 0], [1, 0]), name='fighting.weapons'),
          rule('fighting', 'kit', 0.08, annulus([0, 0], [1, 0]), name='fighting.kit'),
          rule('fighting', 'clods', 0.15, annulus([0, 0], [1, 0]), name='fighting.clods'),
          rule('fighting', 'shards', 0.1, annulus([0, 0], [1, 0]), name='fighting.shards'),
          rule('fighting', 'barricade', 0.004, annulus([0, 0], [1, 0]), orient='flight', jitter=20, lane_clear=True, min_intensity=0.6,
               name='fighting.barricade')]
    # ── Authored bodies (the generator's and the wreckage layer's): blood and kit around each.
    R += [rule('casualty', 'blood', 1.5, annulus([0, 0.1], [0, 0.9]), name='casualty.blood'),
          rule('casualty', 'kit', 0.5, annulus([0, 0.3], [0, 1.6]), name='casualty.kit'),
          rule('casualty', 'weapons', 0.25, annulus([0, 0.3], [0, 1.4]), name='casualty.weapons'),
          rule('casualty', 'cases_small', 1.2, annulus([0, 0.2], [0, 1.5]), name='casualty.cases')]
    # ── Buildings: the rubble apron of every ruin, household objects thrown out of it.
    R += [rule('building', 'rubble', 1.0, annulus([0.9, 0], [1, 6]), orient='radial', jitter=70, name='building.rubble'),
          rule('building', 'clods', 0.3, annulus([0.9, 0], [1, 4]), name='building.clods'),
          rule('building', 'furniture', 0.05, annulus([1, 0.3], [1, 5]), lane_clear=True, name='building.furniture'),
          rule('building', 'kit', 0.2, annulus([0.9, 0], [1, 6]), name='building.kit'),
          rule('building', 'snow', 0.1, annulus([1, 0], [1, 5]), scale=(0.5, 1.1), classes=['snow'], name='building.snow')]
    return R


def band_area(band, src):
    hl, hw, R = src.get('half_length', 1.0), src.get('half_width', 1.0), src.get('radius', 1.0)
    if band['shape'] == 'rect':
        a0 = hl * band['along'][0][0] + band['along'][0][1]
        a1 = hl * band['along'][1][0] + band['along'][1][1]
        l0 = hw * band['lateral'][0][0] + band['lateral'][0][1]
        l1 = hw * band['lateral'][1][0] + band['lateral'][1][1]
        return max(a1 + a0, 0.0) * max(l1 - l0, 0.0) * (2 if band.get('mirror', True) else 1)
    r0 = R * band['r0'][0] + band['r0'][1]
    r1 = R * band['r1'][0] + band['r1'][1]
    frac = (band['sector'][1] * 2 / 360.0) if band.get('sector') else 1.0
    return max(math.pi * (r1 * r1 - r0 * r0), 0.0) * frac


# ---------------------------------------------------------------------------------------------------
# sources
# ---------------------------------------------------------------------------------------------------
def long_axis_heading(stem, deg):
    """World heading (degrees, 0 = +Z, 90 = +X) of an asset's long axis for a placement yaw."""
    key = next((k for k, v in WRECK_ASSETS.items() if Path(v[0]).stem == stem), None)
    axis = WRECK_ASSETS[key][1] if key else 'z'
    a = math.radians(deg)
    fx, fz = (math.cos(a), -math.sin(a)) if axis == 'x' else (math.sin(a), math.cos(a))
    return math.degrees(math.atan2(fx, fz))


WRECK_CLASS = {'kv1_wreck_burnt': ('tank', 3.6, 1.7, 'soviet', True), 'kv1_wreck_detail': ('tank', 3.6, 1.7, 'soviet', False),
               'pz3_wreck_burnt': ('tank', 4.0, 2.0, 'german', True), 'pz3_wreck_detail': ('tank', 4.0, 2.0, 'german', False),
               'lorry_wreck_burnt': ('lorry', 3.3, 1.3, 'german', True), 'lorry_wreck_detail': ('lorry', 3.3, 1.2, 'german', False),
               'wreck_cart': ('cart', 2.2, 1.1, 'german', False), 'panje_sledge_wreck_detail': ('sledge', 2.2, 0.7, 'german', False),
               'panje_sledge_overturned_wreck_detail': ('sledge', 2.2, 0.7, 'german', False),
               'draught_horse_a_wreck_detail': ('horse', 1.5, 1.0, 'german', False), 'draught_horse_b_wreck_detail': ('horse', 1.5, 1.0, 'german', False),
               'wreck_gun45': ('gun', 1.7, 0.8, 'soviet', False), 'gun_pit_parapet': ('parapet', 2.3, 3.7, 'soviet', False),
               'outpost': ('outpost', 1.9, 3.2, 'soviet', False)}
TREES = {'tree_8': 2.8, 'tree_9': 2.8, 'pine_tree_n_1': 2.2, 'pine_tree_n_2': 2.4, 'pine_tree_n_3': 2.6, 'tree_stump_1': 0.8,
         'resting_tree_log': 1.2}
OBSTACLES = {'wall_hr_1': (1.3, 0.25), 'fence_b_1': (1.3, 0.15)}
SCENE_INTENSITY = {'armour_column': (0.6, 16), 'retreat_line': (0.8, 18), 'dissolved_unit': (1.0, 22), 'gun_position': (0.9, 14),
                   'strike_field': (0.5, 20), 'lone_vehicle': (0.3, 10), 'frozen_horses': (0.4, 14)}
CLUSTER_INTENSITY = {'ruined_position': (0.7, 12), 'destroyed_transport': (0.5, 12), 'abandoned_field_position': (0.5, 12)}


def r3(v):
    return round(float(v), 3)


def derive_sources(ws):
    S = []
    counts = collections.Counter()
    route = ws.route
    # roads: one segment per route span
    pts = route.p
    for i in range(len(pts) - 1):
        a, b = pts[i], pts[i + 1]
        d = b - a
        length = float(np.linalg.norm(d))
        if length < 0.5:
            continue
        m = (a + b) * 0.5
        S.append(dict(kind='road', x=r3(m[0]), z=r3(m[1]), heading_deg=r3(math.degrees(math.atan2(d[0], d[1]))),
                      half_length=r3(length * 0.5), half_width=ROAD_HALF_M))
        counts['road'] += 1
    # every live placement: base + overlay adds, minus deleted base indices
    assets = ws.manifest['assets']
    deleted = {op['i'] for op in ws.edits.get('ops', []) if op.get('op') == 'delete'}
    live = []
    for i, p in enumerate(ws.manifest['placements']):
        if i in deleted:
            continue
        s = p[5] if not isinstance(p[5], list) else max(p[5])
        live.append((Path(assets[p[0]]).stem, float(p[1]), float(p[3]), float(p[4]), float(s)))
    for op in ws.edits.get('ops', []):
        if op.get('op') == 'add':
            s = op.get('s', 1.0)
            s = s if not isinstance(s, list) else max(s)
            live.append((Path(op['asset']).stem, float(op['x']), float(op['z']), float(op.get('deg', 0.0)), float(s)))
    for stem, x, z, deg, s in live:
        if stem in WRECK_CLASS:
            cls, hl, hw, side, burnt = WRECK_CLASS[stem]
            heading = long_axis_heading(stem, deg)
            S.append(dict(kind='wreck', cls=cls, x=r3(x), z=r3(z), heading_deg=r3(heading), half_length=r3(hl * s), half_width=r3(hw * s),
                          radius=r3(max(hl, hw) * s), side=side, burnt=burnt))
            counts['wreck'] += 1
            S.append(dict(kind='obstacle', x=r3(x), z=r3(z), heading_deg=r3(heading), half_length=r3(hl * s), half_width=r3(hw * s)))
            counts['obstacle'] += 1
            if cls == 'gun':
                S.append(dict(kind='gun', x=r3(x), z=r3(z), heading_deg=r3(heading), radius=r3(max(hl, hw) * s), side='45mm'))
                counts['gun'] += 1
            elif cls == 'tank':
                S.append(dict(kind='gun', x=r3(x), z=r3(z), heading_deg=r3(heading), radius=r3(max(hl, hw) * s),
                              side='76mm' if side == 'soviet' else '88mm'))
                counts['gun'] += 1
        elif stem == 'crater_scar':
            S.append(dict(kind='crater', x=r3(x), z=r3(z), radius=r3(1.35 * s)))
            counts['crater'] += 1
        elif stem == 'scorched_earth':
            S.append(dict(kind='scorch', x=r3(x), z=r3(z), radius=r3(1.1 * s)))
            counts['scorch'] += 1
        elif stem in TREES:
            S.append(dict(kind='tree', x=r3(x), z=r3(z), radius=r3(TREES[stem] * s)))
            counts['tree'] += 1
        elif stem in OBSTACLES:
            hl, hw = OBSTACLES[stem]
            S.append(dict(kind='obstacle', x=r3(x), z=r3(z), heading_deg=r3(long_axis_heading(stem, deg)), half_length=r3(hl * s), half_width=r3(hw * s)))
            counts['obstacle'] += 1
        elif stem.startswith('casualty_'):
            S.append(dict(kind='casualty', x=r3(x), z=r3(z), heading_deg=r3(deg), radius=1.0))
            counts['casualty'] += 1
    # fighting: wreckage scenes by route fraction and side, population clusters, garrison posts
    for scene in ws.edits.get('canyon_v2_wreckage', {}).get('scenes', []):
        inten, radius = SCENE_INTENSITY.get(scene['kind'], (0.4, 12))
        f = float(scene['fraction'])
        x, z, tx, tz = route.at(f)
        lateral = float(scene.get('side', 1)) * (LANE_HALF_M + radius * 0.6)
        wx, wz = route.world(f, lateral)
        nationality = scene.get('nationality', 'german')
        flight = math.degrees(math.atan2(tx, tz)) + (0.0 if nationality == 'german' else 180.0)
        S.append(dict(kind='fighting', x=r3(wx), z=r3(wz), radius=r3(radius), intensity=inten, flight_deg=r3(flight), side=nationality,
                      origin=scene['id']))
        counts['fighting'] += 1
    for c in ws.edits.get('canyon_v2_population', {}).get('clusters', []):
        inten, radius = CLUSTER_INTENSITY.get(c['kind'], (0.4, 12))
        inten = min(1.0, inten + 0.05 * float(c.get('bodies', 0)))
        _, f = route.nearest(c['x'], c['z'])
        x, z, tx, tz = route.at(f)
        side = 'soviet' if c['kind'] == 'ruined_position' else 'german'
        flight = math.degrees(math.atan2(tx, tz)) + (0.0 if side == 'german' else 180.0)
        S.append(dict(kind='fighting', x=r3(c['x']), z=r3(c['z']), radius=r3(radius), intensity=inten, flight_deg=r3(flight), side=side,
                      origin=c['id']))
        counts['fighting'] += 1
    for post in ws.edits['canyon_course'].get('garrison', []):
        _, f = route.nearest(post['x'], post['z'])
        x, z, tx, tz = route.at(f)
        # the posts are held: what lies around them is the attackers' earlier dead, facing the post
        flight = math.degrees(math.atan2(post['x'] - x, post['z'] - z))
        S.append(dict(kind='fighting', x=r3(post['x']), z=r3(post['z']), radius=9.0, intensity=0.35, flight_deg=r3(flight), side='german',
                      origin=post.get('host', 'garrison')))
        counts['fighting'] += 1
    # buildings
    for b in ws.edits.get('building_instances', []):
        hw, hl = float(b['width_m']) * 0.5, float(b['depth_m']) * 0.5
        S.append(dict(kind='building', x=r3(b['x']), z=r3(b['z']), heading_deg=r3(b.get('yaw_deg', 0.0)), half_length=r3(hl), half_width=r3(hw),
                      radius=r3(max(hl, hw)), condition=b.get('condition', 'ruin')))
        counts['building'] += 1
        S.append(dict(kind='obstacle', x=r3(b['x']), z=r3(b['z']), heading_deg=r3(b.get('yaw_deg', 0.0)), half_length=r3(hl), half_width=r3(hw)))
        counts['obstacle'] += 1
    return S, counts


# ---------------------------------------------------------------------------------------------------
# ground raster
# ---------------------------------------------------------------------------------------------------
def ground_raster(ws, sources, root):
    """R: class (0 snow, 1 road, 2 rubble, 3 scorch, 5 bare rock); G: snow cover 0..255."""
    n = GROUND_RES
    world = (ws.H.shape[0] - 1) * ws.spacing
    texel = world / n
    cls = np.zeros((n, n), np.uint8)
    xs = (np.arange(n) + 0.5) * texel - world * 0.5
    # snow cover from the terrain's snow map
    snow_path = root / ws.manifest['terrain'].get('snow_map', '')
    if snow_path.is_file():
        snow = np.asarray(Image.open(snow_path).resize((n, n), Image.BILINEAR), dtype=np.float32)
        if snow.ndim == 3:
            snow = snow[..., 0]
        snow = np.clip(snow / (255.0 if snow.max() > 1.5 else 1.0), 0, 1)
    else:
        snow = np.ones((n, n), np.float32)
    cls[snow < 0.35] = 5
    # road: within the road half-width of the route (plus a shoulder metre)
    route = ws.route
    pts = route.p
    for i in range(len(pts) - 1):
        a, b = pts[i], pts[i + 1]
        lo = np.minimum(a, b) - ROAD_HALF_M - 1.5
        hi = np.maximum(a, b) + ROAD_HALF_M + 1.5
        i0, i1 = int((lo[0] + world * 0.5) / texel), int((hi[0] + world * 0.5) / texel) + 1
        j0, j1 = int((lo[1] + world * 0.5) / texel), int((hi[1] + world * 0.5) / texel) + 1
        i0, j0 = max(i0, 0), max(j0, 0)
        i1, j1 = min(i1, n), min(j1, n)
        if i1 <= i0 or j1 <= j0:
            continue
        gx, gz = np.meshgrid(xs[i0:i1], xs[j0:j1])
        d = b - a
        l2 = float(d @ d) or 1e-9
        t = np.clip(((gx - a[0]) * d[0] + (gz - a[1]) * d[1]) / l2, 0, 1)
        dist = np.hypot(gx - (a[0] + d[0] * t), gz - (a[1] + d[1] * t))
        cls[j0:j1, i0:i1][dist < ROAD_HALF_M + 1.0] = 1
    for s in sources:
        if s['kind'] == 'building':
            r = s['radius'] + 5.0
        elif s['kind'] == 'scorch':
            r = s['radius'] * 1.3
        elif s['kind'] == 'crater':
            r = s['radius'] * 1.2
        else:
            continue
        i0, i1 = int((s['x'] - r + world * 0.5) / texel), int((s['x'] + r + world * 0.5) / texel) + 1
        j0, j1 = int((s['z'] - r + world * 0.5) / texel), int((s['z'] + r + world * 0.5) / texel) + 1
        i0, j0 = max(i0, 0), max(j0, 0)
        i1, j1 = min(i1, n), min(j1, n)
        if i1 <= i0 or j1 <= j0:
            continue
        gx, gz = np.meshgrid(xs[i0:i1], xs[j0:j1])
        inside = np.hypot(gx - s['x'], gz - s['z']) < r
        cls[j0:j1, i0:i1][inside] = 2 if s['kind'] == 'building' else 3
    out = np.zeros((n, n, 3), np.uint8)
    out[..., 0] = cls
    out[..., 1] = (snow * 255).astype(np.uint8)
    return Image.fromarray(out, 'RGB'), texel


# ---------------------------------------------------------------------------------------------------
def estimate(sources, rules_):
    by_kind = collections.defaultdict(list)
    for s in sources:
        by_kind[s['kind']].append(s)
    rows = []
    total = 0.0
    for r in rules_:
        n = 0.0
        for s in by_kind.get(r['source'], []):
            if r.get('side') and s.get('side') and r['side'] != s['side']:
                continue
            if s.get('intensity', 1.0) < r.get('min_intensity', 0.0):
                continue
            n += r['density'] * s.get('intensity', 1.0) * band_area(r['band'], s)
        rows.append((r['name'], n))
        total += n
    return rows, total


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--dest', default=DEST)
    parser.add_argument('--seed', type=lambda v: int(v, 0), default=DEFAULT_SEED)
    parser.add_argument('--density', type=float, default=1.0)
    parser.add_argument('--estimate', action='store_true')
    args = parser.parse_args()
    root = args.root
    ws = Workshop(root, root / args.dest)
    sources, counts = derive_sources(ws)
    rules_ = rules()
    for r in rules_:
        r['density'] = r['density'] * args.density
    grp = groups()
    missing = [e['asset'] for entries in grp.values() for e in entries if not (root / e['asset']).is_file()]
    if missing:
        raise SystemExit('missing group assets:\n  ' + '\n  '.join(missing))
    rows, total = estimate(sources, rules_)
    print('sources:', dict(counts))
    for name, n in sorted(rows, key=lambda t: -t[1]):
        print(f'  {name:32s} {n:10.0f}')
    print(f'expected placements ~ {total:,.0f} (companions excluded)')
    if args.estimate:
        return
    raster, texel = ground_raster(ws, sources, root)
    dest = root / args.dest
    raster.save(dest / 'scatter_ground.v1.png')
    doc = dict(schema='kessel.scatter', version=VERSION, tool=TOOL, authored='2026-10-27', seed=args.seed, density=args.density,
               wind=list(WIND), lane_half_m=LANE_HALF_M, road_half_m=ROAD_HALF_M,
               keep_clear=[dict(x=r3(ws.spawn[0]), z=r3(ws.spawn[1]), r=60.0, why='spawn'),
                           dict(x=r3(ws.goal[0]), z=r3(ws.goal[1]), r=40.0, why='goal')],
               ground=dict(raster='scatter_ground.v1.png', texel_m=texel, res=GROUND_RES,
                           classes=['snow', 'road', 'rubble', 'scorch', 'mud', 'rock']),
               provenance=dict(
                   cls='attested_condition',
                   scope='Sources are the workshop\'s own authored events (route, craters, wrecks, guns, trees, obstacles, the '
                         'wreckage layer\'s scenes, the population clusters, the garrison, the building instances); the litter '
                         'around them is the recurring, well-attested condition of the pocket\'s roads and positions in '
                         'December 1942 to January 1943: fired cases, dropped kit, splinters and thrown earth, masonry, the '
                         'unburied dead and the drifts that formed against everything that stood. No named engagement, unit '
                         'or person; nothing readable.',
                   references=['docs/CANYON_V2_WRECKAGE_20261027.md (provenance block)', 'docs/ENVSTORY.md',
                               'docs/research/PHOTO_REFERENCE_ANALYSIS_20260928.md', 'docs/research/URBAN_INTRO_TABLEAUX_20260928.md']),
               expected=dict(total=round(total), rules={name: round(n) for name, n in rows}),
               source_counts=dict(counts), groups=grp, rules=rules_, sources=sources)
    (dest / 'scatter.v1.json').write_text(json.dumps(doc, separators=(',', ':')) + '\n')
    print(f'wrote {dest / "scatter.v1.json"} ({len(sources)} sources, {len(rules_)} rules, {sum(len(v) for v in grp.values())} group entries) '
          f'and scatter_ground.v1.png ({GROUND_RES}^2 at {texel:.3f} m)')


if __name__ == '__main__':
    main()
