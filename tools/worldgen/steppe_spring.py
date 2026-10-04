#!/usr/bin/env python3
"""Map 2, first pass: a spring steppe of long rolling hills, shallow ravines (balkas), strip
fields, villages along a dirt road and patches of dense wood. Kursk-like in character; it is a
generated mechanics course, not a survey of any real ground.

The terrain is generated here (no Gaea source). The route, villages, cover and defence come from
the same course grammar as the canyon run, so the map plays in the existing run mode.

Run from the checkout: python tools/worldgen/steppe_spring.py [--dest DIR] [--plan PNG]
Assets are read from and written under KESSEL_ASSET_ROOT (the checkout that holds `assets/`).
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
import canyon_course as cc
import gaea_canyon_run as gaea

ASSET_ROOT = Path(os.environ.get('KESSEL_ASSET_ROOT', HERE.parents[1])).resolve()
for module in (cc, gaea, cc.uid):
    if hasattr(module, 'ROOT'):
        module.ROOT = ASSET_ROOT
DEST = ASSET_ROOT/'assets/worldgen/workshops/steppe-spring-v1'
SOURCE = ASSET_ROOT/'assets/terrain/steppe_spring/source'
TERRAIN = ASSET_ROOT/'assets/terrain/steppe_spring'
VERSION = 1
SEED = 0x5747_1943
EXTENT, RES, TEX = gaea.EXTENT, gaea.RES, 4096
# Route anchors on the 4096 grid: south-west to north-east, across two ravines.
ANCHORS = [(620, 3480), (1380, 2640), (2180, 2820), (2740, 1820), (3480, 720)]
GROUPS = [
    ('hamlet', (.05, .17), (18., 105.), {'wooden_house': .9, 'merchant_brick_house': .1}, 22),
    ('village', (.26, .44), (18., 150.), {'wooden_house': .78, 'merchant_brick_house': .16, 'workshop': .06}, 60),
    ('farm', (.47, .55), (22., 105.), {'wooden_house': .7, 'workshop': .3}, 14),
    ('station', (.58, .73), (18., 150.), {'workshop': .45, 'wooden_house': .35, 'merchant_brick_house': .20}, 36),
    ('strongpoint', (.82, .93), (22., 125.), {'wooden_house': .5, 'merchant_brick_house': .3, 'workshop': .2}, 28),
]
NAT = cc.NAT
WOOD_TREES = ['tree_1', 'tree_5', 'tree_6', 'tree_7', 'tree_5', 'tree_7', 'pine_tree_n_1']
WOOD_SPACING_M = 5.6
WOOD_LANE_M = 34.0
MAX_WOOD_TREES = int(os.environ.get("STEPPE_TREES", "9000"))
WOOD_REACH_M = 430.0      # woods stand where the road passes them, not across the whole square


def band_noise(rng, n, lo_m, hi_m, extent=EXTENT):
    """Unit-variance noise whose wavelengths lie between lo_m and hi_m (smooth band edges)."""
    f = np.fft.fftfreq(n, d=extent/n)
    fx, fz = np.meshgrid(f, f)
    k = np.hypot(fx, fz)
    k[0, 0] = 1e-9
    wavelength = 1.0/k
    weight = np.exp(-0.5*(np.log(wavelength/math.sqrt(lo_m*hi_m))/(0.5*math.log(hi_m/lo_m)))**2)
    weight[0, 0] = 0.0
    spectrum = np.fft.fft2(rng.standard_normal((n, n)))*weight
    out = np.fft.ifft2(spectrum).real
    return out/out.std()


def heights(rng):
    """Rolling hills with ravines cut where a slow field crosses zero, deeper in low ground."""
    n = RES
    hills = 10.5*band_noise(rng, n, 900, 2600)+3.6*band_noise(rng, n, 320, 900)
    low = 1.0/(1.0+np.exp((hills-np.percentile(hills, 45))/3.2))    # 1 in low ground
    vein = np.abs(band_noise(rng, n, 700, 1700))
    # Only in low ground: a ravine is a branch of the drainage, never a ring round a hill.
    ravine = np.exp(-(vein/0.085)**2)*low**1.6
    # A ravine has a flat wet floor and convex shoulders, not a V notch.
    H = hills-7.5*ravine**0.8+0.55*band_noise(rng, n, 70, 260)+0.12*band_noise(rng, n, 18, 60)
    H -= H.min()-6.0
    return H, ravine, low


def write_source(H):
    """The course grammar reads a 4096-square 16-bit height export; give it one."""
    SOURCE.mkdir(parents=True, exist_ok=True)
    big = np.asarray(Image.fromarray(H.astype(np.float32)).resize((TEX, TEX), Image.BICUBIC), dtype=np.float64)
    raw = np.clip(big/gaea.ELEVATION*65535.0, 0, 65535).astype(np.uint16)
    Image.fromarray(raw).save(SOURCE/'Export.png')
    return SOURCE


def upsample(field, order=Image.BILINEAR):
    return np.asarray(Image.fromarray(field.astype(np.float32)).resize((TEX, TEX), order), dtype=np.float32)


def route_distance_field(route):
    """Distance in metres from every texel to the route polyline (chunked, bounded memory)."""
    axis = np.linspace(-EXTENT/2, EXTENT/2, TEX//4)
    X, Z = np.meshgrid(axis, axis)
    pts = np.column_stack([X.ravel(), Z.ravel()]).astype(np.float32)
    r = route[::2].astype(np.float32)
    best = np.full(len(pts), np.inf, dtype=np.float32)
    for start in range(0, len(pts), 16384):
        p = pts[start:start+16384]
        best[start:start+len(p)] = np.sqrt(((p[:, None, :]-r[None, :, :])**2).sum(axis=2).min(axis=1))
    return upsample(best.reshape(X.shape))


def ground(rng, H, ravine, route, buildings, woods):
    """The ground's colour and where grass grows. Colours are linear-ish sRGB values 0..1."""
    slope = upsample(np.hypot(*np.gradient(H, EXTENT/(RES-1))))
    rav = upsample(ravine)
    wood = upsample(woods.astype(np.float32))
    fine = band_noise(rng, TEX, 4, 30).astype(np.float32)
    mid = band_noise(rng, TEX, 40, 260).astype(np.float32)
    axis = np.linspace(-EXTENT/2, EXTENT/2, TEX, dtype=np.float32)
    X, Z = np.meshgrid(axis, axis)
    # Strip fields: blocks of a few hundred metres, each ploughed in its own direction.
    block = 520.0
    bx, bz = np.floor((X+mid*60)/block).astype(np.int64), np.floor((Z-mid*60)/block).astype(np.int64)
    key = (bx*73856093) ^ (bz*19349663) ^ SEED
    angle = ((key % 1000)/1000.0*math.pi).astype(np.float32)
    width = (38.0+(key//1000 % 60)).astype(np.float32)
    across = X*np.cos(angle)+Z*np.sin(angle)
    strip = np.floor(across/width).astype(np.int64)
    crop = ((strip*2654435761) ^ key) % 100
    furrow = 0.5+0.5*np.sin(across*math.tau/1.9)
    meadow = np.stack([0.27+0.05*mid, 0.37+0.06*mid, 0.14+0.03*mid], axis=-1)
    colour = meadow.copy()
    wheat = np.stack([0.36+0.03*fine, 0.45+0.03*fine, 0.17+0.0*fine], axis=-1)
    plough = np.stack([0.15+0.03*furrow, 0.12+0.025*furrow, 0.09+0.02*furrow], axis=-1)
    rye = np.stack([0.50+0.03*fine, 0.50+0.03*fine, 0.24+0.0*fine], axis=-1)
    fieldable = (slope < 0.10) & (rav < 0.12) & (wood < 0.3)
    for lo, hi, tint in [(0, 42, wheat), (42, 62, plough), (62, 76, rye)]:
        mask = fieldable & (crop >= lo) & (crop < hi)
        colour[mask] = tint[mask]
    # Ravine floors are wet and dark; their steep shoulders show clay.
    wet = np.clip((rav-0.45)/0.4, 0, 1)[..., None]
    colour = colour*(1-wet)+np.array([0.16, 0.25, 0.11])*wet
    clay = np.clip((slope-0.21)/0.08, 0, 1)[..., None]*0.8
    colour = colour*(1-clay)+np.array([0.46, 0.36, 0.24])*clay
    floor = np.clip(wood, 0, 1)[..., None]
    colour = colour*(1-floor)+np.array([0.13, 0.19, 0.09])*floor
    colour *= (1.0+0.06*fine)[..., None]
    # The road: a dirt track with two worn ruts, and trodden yards around the houses.
    d = route_distance_field(route)
    track = np.clip(1.0-(d-2.6)/1.6, 0, 1)*(0.75+0.25*np.clip(mid+0.5, 0, 1))
    rut = np.exp(-((d-1.45)/0.45)**2)
    dirt = np.stack([0.37-0.10*rut, 0.30-0.09*rut, 0.21-0.07*rut], axis=-1)
    colour = colour*(1-track[..., None])+dirt*track[..., None]
    yard = np.zeros((TEX, TEX), dtype=np.float32)
    to_px = (TEX-1)/EXTENT
    for b in buildings:
        r = max(b['width_m'], b['depth_m'])*0.5+7.0
        cx, cz = (b['x']+EXTENT/2)*to_px, (b['z']+EXTENT/2)*to_px
        x0, x1 = int(max(cx-r*to_px-2, 0)), int(min(cx+r*to_px+2, TEX))
        z0, z1 = int(max(cz-r*to_px-2, 0)), int(min(cz+r*to_px+2, TEX))
        yy, xx = np.mgrid[z0:z1, x0:x1]
        dist = np.hypot(xx-cx, yy-cz)/to_px
        yard[z0:z1, x0:x1] = np.maximum(yard[z0:z1, x0:x1], np.clip(1.0-(dist-r+5.0)/5.0, 0, 1)*0.8)
    colour = colour*(1-yard[..., None])+np.array([0.33, 0.28, 0.20])*yard[..., None]
    # Grass stands in drifts and clumps, thick in the meadows and thin on the crops.
    clump = np.clip(0.5+0.9*mid+0.5*fine, 0, 1)
    grass = np.clip(1.0-track*1.5-yard-clay[..., 0]*0.8, 0, 1)*clump
    grass[fieldable & (crop < 42)] *= 0.35
    grass[fieldable & (crop >= 62) & (crop < 76)] *= 0.35
    grass[fieldable & (crop >= 42) & (crop < 62)] = 0.0       # ploughed strips carry none
    grass *= 1.0-0.75*np.clip(wood, 0, 1)                     # little grows under the canopy
    return np.clip(colour, 0, 1), grass


def wood_mask(rng, H, ravine, route, buildings):
    """Patches of dense wood on the 1025 grid: clear of the road, the houses and ravine floors."""
    patch = band_noise(rng, RES, 300, 800)
    slope = np.hypot(*np.gradient(H, EXTENT/(RES-1)))
    mask = (slope < 0.24) & (ravine < 0.5)
    axis = np.linspace(-EXTENT/2, EXTENT/2, RES)
    X, Z = np.meshgrid(axis, axis)
    near = np.full(X.shape, np.inf)
    for start in range(0, len(route), 4):
        near = np.minimum(near, np.hypot(X-route[start, 0], Z-route[start, 1]))
    mask &= (near > WOOD_LANE_M) & (near < WOOD_REACH_M)
    for b in buildings:
        mask &= np.hypot(X-b['x'], Z-b['z']) > max(b['width_m'], b['depth_m'])*0.5+14.0
    # Keep the densest cores only, as many cells as the tree budget fills at full density, so
    # every patch that exists is a real thicket and none is a thin scatter.
    cell_m2 = (EXTENT/(RES-1))**2
    cells = int(MAX_WOOD_TREES*WOOD_SPACING_M**2/cell_m2)
    eligible = np.sort(patch[mask])
    threshold = eligible[-cells] if len(eligible) > cells else -np.inf
    return mask & (patch >= threshold)


def plant_woods(manifest, H, woods):
    """Dense stands: a jittered grid of trees inside each wood patch, tighter than a tank is wide."""
    grid = gaea.SurfaceGrid(RES, EXTENT/(RES-1))
    assets = manifest['assets']
    index = {}
    for name in WOOD_TREES:
        path = NAT+name+'.glb'
        if path not in assets:
            assets.append(path)
        index[name] = assets.index(path)
    cells = int(EXTENT/WOOD_SPACING_M)
    placed = []
    for iz in range(cells):
        for ix in range(cells):
            x = -EXTENT/2+(ix+0.5+0.8*(cc.rnd(f'wood.{ix}.{iz}', 1)-0.5))*WOOD_SPACING_M
            z = -EXTENT/2+(iz+0.5+0.8*(cc.rnd(f'wood.{ix}.{iz}', 2)-0.5))*WOOD_SPACING_M
            gx, gz = int(round(grid.col(x))), int(round(grid.col(z)))
            if not (2 <= gx < RES-2 and 2 <= gz < RES-2) or not woods[gz, gx]:
                continue
            kind = WOOD_TREES[int(cc.rnd(f'wood.{ix}.{iz}', 3)*len(WOOD_TREES))]
            scale = 0.85+0.75*cc.rnd(f'wood.{ix}.{iz}', 4)
            placed.append([index[kind], cc.r3(x), cc.r3(grid.sample(H, x, z)-0.05), cc.r3(z),
                           round(cc.rnd(f'wood.{ix}.{iz}', 5)*360.0, 2), cc.r3(scale)])
    manifest['placements'].extend(placed)
    return len(placed)


def build(dest, plan=None):
    rng = np.random.default_rng(SEED)
    H0, ravine, _ = heights(rng)
    source = write_source(H0)
    gaea.ANCHORS = ANCHORS
    cc.GROUPS = GROUPS
    cc.CRATERS = 30
    cc.RAZED_PLOTS = 6
    # Leaf trees in the yards and along the road; the bare winter trees stay with the canyon.
    cc.ASSETS['tree_tall'] = NAT+'tree_5.glb'
    cc.ASSETS['tree_small'] = NAT+'tree_7.glb'
    manifest, edits, H, receipt = gaea.build(source)
    # No snow in May: drop every drift the winter grammar laid along the road.
    drifts = {i for i, path in enumerate(manifest['assets']) if 'snow_drift' in path}
    before = len(manifest['placements'])
    manifest['placements'] = [p for p in manifest['placements'] if p[0] not in drifts]
    route = np.array(edits['canyon_course']['route'], dtype=np.float64)
    buildings = edits['building_instances']
    datum = receipt['runtime_datum_m']
    woods = wood_mask(rng, H, ravine, route, buildings)
    trees = plant_woods(manifest, H, woods)
    colour, grass = ground(rng, H, ravine, route, buildings, woods)
    TERRAIN.mkdir(parents=True, exist_ok=True)
    # The engine's packed ground format: colour in RGB, snow cover in alpha (32 is none).
    rgba = np.dstack([np.uint8(np.rint(colour*255)), np.full((TEX, TEX), 32, dtype=np.uint8)])
    Image.fromarray(rgba, 'RGBA').save(TERRAIN/'ground_material.png')
    cls = np.uint8(np.rint(grass*255*float(os.environ.get('STEPPE_GRASS', '1'))))
    Image.fromarray(np.dstack([cls, np.zeros_like(cls), np.zeros_like(cls)]), 'RGB').save(TERRAIN/'ground_class.png')
    manifest['terrain'].update(texture='assets/terrain/steppe_spring/ground_material.png',
        material_encoding='gaea-rock-srgb-snow-linear-alpha32-223-v1',
        ground_class='assets/terrain/steppe_spring/ground_class.png', grass_season='spring',
        color=[0.9, 0.9, 0.9, 1.0])
    manifest['authoring'].update(tool='tools/worldgen/steppe_spring.py', tool_version=VERSION, variant='steppe-spring')
    edits['base_placements'] = len(manifest['placements'])
    edits['canyon_course'].update(variant='steppe-spring', tool='tools/worldgen/steppe_spring.py')
    edits['canyon_course']['provenance'] = dict(
        terrain='Generated rolling steppe with ravines, strip fields and wood patches. Kursk-like in character only; not a survey of real ground.',
        buildings=cc.PROVENANCE['buildings'], defence=cc.PROVENANCE['defence'],
        scope='Generative mechanics course, not a recorded engagement.')
    # A clear late afternoon in early summer: a low sun, long shadows, broken cloud.
    edits['environment_lighting'] = dict(enabled=True, fog_density=0.0007, overcast=0.32, storm=0.0,
        schema='kessel.environment-authoring', utc='1943-07-16T14:40:00Z', version=2)
    dest = Path(dest)
    dest.mkdir(parents=True, exist_ok=True)
    (dest/'manifest.v1.json').write_text(cc.dumps(manifest, indent=None))
    (dest/'manifest_edits.json').write_text(cc.dumps(edits))
    np.save(dest/'heights.npy', H.astype(np.float32))
    receipt.update(schema='panzer.steppe-spring', version=VERSION, seed=SEED, wood_trees=trees,
        wood_cover_fraction=float(woods.mean()), snow_drifts_removed=before-len(manifest['placements'])+trees,
        height_range_m=[float(H.min()), float(H.max())], runtime_datum_m=datum,
        outputs={n: cc.sha256_file(dest/n) for n in cc.OUTPUT_FILES})
    (dest/'canyon-course.receipt.json').write_text(cc.dumps(receipt))
    if plan:
        preview = np.uint8(np.rint(colour[::4, ::4]*255)).copy()
        to_px = (TEX//4-1)/EXTENT
        for x, z in route:
            px, pz = int((x+EXTENT/2)*to_px), int((z+EXTENT/2)*to_px)
            preview[max(pz-1, 0):pz+2, max(px-1, 0):px+2] = [255, 255, 255]
        for b in buildings:
            px, pz = int((b['x']+EXTENT/2)*to_px), int((b['z']+EXTENT/2)*to_px)
            preview[max(pz-1, 0):pz+2, max(px-1, 0):px+2] = [230, 120, 40]
        Image.fromarray(preview).save(plan)
    print(json.dumps({k: receipt[k] for k in ['route_length_m', 'standing_buildings', 'wood_trees',
        'wood_cover_fraction', 'height_range_m', 'gun_candidates', 'travel_surface']}, indent=1))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--dest', type=Path, default=DEST)
    parser.add_argument('--plan', type=Path)
    args = parser.parse_args()
    build(args.dest, args.plan)
