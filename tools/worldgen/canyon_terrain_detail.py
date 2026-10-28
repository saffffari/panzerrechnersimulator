#!/usr/bin/env python3
"""Canyon v2 terrain detail: the Gaea canyon at its native resolution, wind-redistributed snow.

The Canyon v2 runtime grid (`heights.npy`, 1025 nodes, 4.883 m) is a bilinear 4x downsample of the
Gaea export (`Export.png`, 4096 px, 16-bit, 5000 m a side, 1.221 m). This cook keeps that grid as
the fallback and writes, next to it under `detail/`:

  heights.4097.npy    float32 C-order, 4097 nodes at 1.2207 m: the native export, dequantised
                      within its own 16-bit step, with the course's authored edits (building pads,
                      receipt craters, travel-strip blends) carried over from heights.npy, plus the
                      surface change of wind-redistributed snow (drifts in the lee, scour on
                      exposed ground).
  surface.4097.png    RGBA8 per node: R ground class, G snow depth (cm), B signed offset from the
                      course centreline (0.1 m steps, 127 = on it, 255 = beyond 12.7 m),
                      A Winstral upwind shelter Sx (deg * 4 + 128).
  terrain-detail.receipt.json

and adds `terrain.detail` to manifest.v1.json. The runtime reads both with src/terrain_relief.rs.

Snow model (named in docs/TERRAIN_DETAIL.md and the physical-truth ledger): the Gaea height is
the Snowfield node's output, i.e. already a snow surface; settled steppe snow of 0.40 m is the
declared depth wherever Gaea's snow mask is set. The local depth follows the Winstral maximum
upwind slope Sx over 100 m along the act's prevailing wind (world::weather_wind_profile(1)),
computed on the surface plus building and wreck envelopes, so drifts form behind obstacles and
ridges are scoured. Depth = 0.40 m * (1 + max(Sx - 2.5, 0) / 5 + min(Sx, 0) / 15) (deg), the lee term spread 2 m
across the wind, clamped to 0..2 m: an empirical linear redistribution, not a snow-transport
solve and not fitted to a 1942 survey.

Run from the checkout: python tools/worldgen/canyon_terrain_detail.py [--source DIR] [--root DIR]
"""
import argparse
import hashlib
import json
import math
import os
import time
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage

ROOT = Path(__file__).resolve().parents[2]
TOOL = 'tools/worldgen/canyon_terrain_detail.py'
TOOL_VERSION = 2
WORKSHOP = 'assets/worldgen/workshops/gaea-canyon-run-v2'
SOURCES = [Path('E:/gaea_maps/panzer/canyon'), Path('/citadel/gaea_maps/panzer/canyon')]
EXTENT = 5000.0
ELEVATION = 2500.0
SRC = 4096
COARSE = 1025
RES = 4097
SP = EXTENT / (RES - 1)
STEP_M = ELEVATION / 65535.0
# world::weather_wind_profile(1): the air moves toward +x/+z; snow arrives from the opposite side.
WIND = np.array([1.0, 0.28]) / math.hypot(1.0, 0.28)
SX_REACH_M = 100.0
SETTLED_DEPTH_M = 0.40
SX_LEE_ONSET_DEG = 2.5
SX_LEE_DEG_PER_DEPTH = 5.0
SX_SCOUR_DEG_PER_DEPTH = 15.0
LEE_SPREAD_M = 2.0
DRIFT_MAX_M = 2.0
ROAD_DEPTH_M = 0.08
ROAD_HALF_M = 3.6
ROUTE_BAND_M = 12.7

SNOW, CRUST, ROCK, FROZEN_MUD, RUBBLE, TRAMPLED = range(6)
CLASS_NAMES = ['snow', 'wind_crust', 'rock', 'frozen_mud', 'rubble', 'trampled']

# Wind-shelter envelopes of wrecks and props (height m, length m, width m). Box approximations of
# the source meshes for the upwind-slope scan only; they do not enter collision or rendering.
OBSTACLES = {
    'pz3_wreck': (2.4, 5.6, 2.9), 'kv1_wreck': (2.7, 6.7, 3.3), 'lorry_wreck': (2.5, 6.0, 2.3),
    'wreck_cart': (1.2, 3.0, 1.5), 'draught_horse': (0.8, 2.5, 0.9), 'panje_sledge': (0.8, 2.5, 1.2),
    'wreck_gun45': (1.2, 4.0, 1.7), 'gun_pit_parapet': (0.8, 5.0, 5.0), 'tiger_hull': (2.0, 6.3, 3.6),
}
CONDITION_HEIGHT = {'burned': 0.35, 'partly_collapsed': 0.6, 'shelled': 0.8}
STOREY_M = 3.0


def sha256_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def dumps(obj, compact=False):
    if compact:
        return json.dumps(obj, ensure_ascii=False, sort_keys=True, separators=(',', ':')) + '\n'
    return json.dumps(obj, indent=1, ensure_ascii=False, sort_keys=True) + '\n'


def tri_upsample(H):
    """Evaluate the runtime 1025 surface (src/terrain.rs grade_at: cells split along the
    (i+1,j)-(i,j+1) diagonal) at every 4097 node; 1025 node i is 4097 node 4i exactly."""
    n = H.shape[0]
    f = (RES - 1) // (n - 1)
    k = np.arange(RES)
    i0 = np.minimum(k // f, n - 2)
    t = (k - i0 * f) / f
    tz, tx = np.meshgrid(t, t, indexing='ij')
    jz, ix = np.meshgrid(i0, i0, indexing='ij')
    h00, h10, h01, h11 = H[jz, ix], H[jz, ix + 1], H[jz + 1, ix], H[jz + 1, ix + 1]
    lower = tx + tz <= 1.0
    return np.where(lower, h00 + (h10 - h00) * tx + (h01 - h00) * tz,
                    h11 + (h01 - h11) * (1 - tx) + (h10 - h11) * (1 - tz))


def coarse_raw(raw):
    """gaea_canyon_run.build()'s exact 1025 resample of the export, in metres before the datum."""
    axis = np.linspace(0, SRC - 1, COARSE)
    lo = axis.astype(int)
    hi = np.minimum(lo + 1, SRC - 1)
    w = axis - lo
    rows = raw[lo] * (1 - w[:, None]) + raw[hi] * w[:, None]
    return (rows[:, lo] * (1 - w[None, :]) + rows[:, hi] * w[None, :]) * ELEVATION / 65535.


def native_nodes(raw):
    """The export at 4097 nodes. Pixel p of 4096 sits at world p * 5000/4095 (as the 1025 cook
    maps it), node k at k * 5000/4096: a bilinear shift of at most half a pixel. Before that, the
    16-bit steps (3.8 cm) are removed by a 3x3 binomial average held inside each sample's own
    quantisation interval, so every height stays consistent with the source value it came from."""
    smooth = ndimage.correlate(raw, np.outer([1, 2, 1], [1, 2, 1]) / 16.0, mode='nearest')
    deq = np.clip(smooth, raw - 0.5, raw + 0.5)
    pos = np.arange(RES) * (SRC - 1) / (RES - 1)
    lo = np.minimum(pos.astype(int), SRC - 2)
    w = pos - lo
    rows = deq[lo] * (1 - w[:, None]) + deq[lo + 1] * w[:, None]
    out = rows[:, lo] * (1 - w[None, :]) + rows[:, lo + 1] * w[None, :]
    return out * STEP_M, float(np.abs(deq - raw).max() * STEP_M)


def world_to_node(x, z):
    return (np.asarray(x) + EXTENT * 0.5) / SP, (np.asarray(z) + EXTENT * 0.5) / SP


def rasterize_boxes(boxes):
    """Max-height raster of oriented boxes (cx, cz, length, width, yaw_deg, height)."""
    out = np.zeros((RES, RES), np.float32)
    for cx, cz, length, width, yaw, height in boxes:
        a = math.radians(yaw)
        ca, sa = math.cos(a), math.sin(a)
        r = 0.5 * math.hypot(length, width)
        gi, gj = world_to_node(cx, cz)
        i0, i1 = int(max(gi - r / SP - 1, 0)), int(min(gi + r / SP + 2, RES))
        j0, j1 = int(max(gj - r / SP - 1, 0)), int(min(gj + r / SP + 2, RES))
        if i0 >= i1 or j0 >= j1:
            continue
        xs = np.arange(i0, i1) * SP - EXTENT * 0.5
        zs = np.arange(j0, j1) * SP - EXTENT * 0.5
        Z, X = np.meshgrid(zs, xs, indexing='ij')
        dx, dz = X - cx, Z - cz
        # glTF +z forward, yaw about +y (the engine's placement rotation).
        u = dx * sa + dz * ca
        v = dx * ca - dz * sa
        inside = (np.abs(u) <= length * 0.5) & (np.abs(v) <= width * 0.5)
        block = out[j0:j1, i0:i1]
        block[inside] = np.maximum(block[inside], height)
    return out


def obstacles(edits):
    buildings, wrecks = [], []
    for b in edits.get('building_instances', []):
        h = b['storeys'] * STOREY_M * CONDITION_HEIGHT.get(b['condition'], 1.0) + 0.6
        buildings.append((b['x'], b['z'], b['depth_m'], b['width_m'], b['yaw_deg'], h))
    for op in edits.get('ops', []):
        if op.get('op') != 'add':
            continue
        stem = op.get('asset', '').split('/')[-1]
        for key, (h, length, width) in OBSTACLES.items():
            if stem.startswith(key):
                s = op.get('s', 1.0)
                s = s[0] if isinstance(s, list) else s
                wrecks.append((op['x'], op['z'], length * s, width * s, op.get('deg', 0.0), h * s))
                break
    return buildings, wrecks


def winstral_sx(S):
    """Maximum upwind slope (deg) over SX_REACH_M against the prevailing wind, per node."""
    up = -WIND
    steps = int(math.ceil(SX_REACH_M / SP))
    pad = steps + 2
    P = np.pad(S, pad, mode='edge')
    best = np.full(S.shape, -90.0, np.float32)
    seen = set()
    for k in range(1, steps + 1):
        di, dj = int(round(up[0] * k)), int(round(up[1] * k))
        if (di, dj) in seen:
            continue
        seen.add((di, dj))
        dist = math.hypot(di, dj) * SP
        shifted = P[pad + dj:pad + dj + RES, pad + di:pad + di + RES]
        np.maximum(best, np.degrees(np.arctan((shifted - S) / dist)).astype(np.float32), out=best)
    return best


def route_offsets(route):
    """Signed distance (m) from the course centreline per node, left of travel negative."""
    pts = np.asarray(route, float)
    dense, tangents = [], []
    for a, b in zip(pts[:-1], pts[1:]):
        seg = b - a
        length = float(np.hypot(*seg))
        n = max(int(length / (SP * 0.5)), 1)
        t = (np.arange(n) / n)[:, None]
        dense.append(a + seg * t)
        tangents.append(np.repeat((seg / max(length, 1e-6))[None], n, 0))
    dense = np.vstack(dense)
    tangents = np.vstack(tangents)
    gi, gj = world_to_node(dense[:, 0], dense[:, 1])
    gi = np.clip(np.rint(gi).astype(int), 0, RES - 1)
    gj = np.clip(np.rint(gj).astype(int), 0, RES - 1)
    seed = np.ones((RES, RES), bool)
    seed[gj, gi] = False
    owner = np.full((RES, RES), -1, np.int64)
    owner[gj, gi] = np.arange(len(dense))
    dist, (nj, ni) = ndimage.distance_transform_edt(seed, return_indices=True)
    nearest = owner[nj, ni]
    zs = np.arange(RES) * SP - EXTENT * 0.5
    Z, X = np.meshgrid(zs, zs, indexing='ij')
    rel_x = X - dense[nearest, 0]
    rel_z = Z - dense[nearest, 1]
    t = tangents[nearest]
    across = t[..., 0] * rel_z - t[..., 1] * rel_x
    along = t[..., 0] * rel_x + t[..., 1] * rel_z
    # Perpendicular offset to the local segment, beyond its end the plain distance.
    signed = np.where(np.abs(along) < SP, across, np.sign(across) * dist * SP)
    return signed.astype(np.float32)


def blur(a, sigma):
    return ndimage.gaussian_filter(a, sigma, mode='nearest')


def build(source, root):
    started = time.time()
    dest = root / WORKSHOP
    receipt_in = json.loads((dest / 'canyon-course.receipt.json').read_text())
    export = source / 'Export.png'
    source_sha = sha256_file(export)
    if source_sha != receipt_in['source_height_sha256']:
        raise ValueError(f'{export} is not the export the course was cooked from')
    img = Image.open(export)
    if img.mode not in ('I;16', 'I;16L'):
        raise ValueError('Gaea export must remain native 16-bit')
    raw = np.array(img, dtype=np.float64)
    if raw.shape != (SRC, SRC):
        raise ValueError(f'expected the {SRC}-square canyon export, got {raw.shape}')
    datum = float(receipt_in['runtime_datum_m'])
    H0 = coarse_raw(raw)
    if abs(float(H0[COARSE // 2, COARSE // 2]) - datum) > 1e-6:
        raise ValueError('runtime datum does not reproduce from the export')
    H1025 = np.load(dest / 'heights.npy').astype(np.float64)
    delta = H1025 - (H0 - datum)
    edited = np.abs(delta) > 1e-3
    # The course's edits stay at their authored 4.88 m definition: inside every coarse cell that
    # touches an edited node the fine surface is the coarse one, with a two-node blend outside.
    cell = edited[:-1, :-1] | edited[1:, :-1] | edited[:-1, 1:] | edited[1:, 1:]
    cell = np.pad(cell, ((0, 1), (0, 1)))
    fine_cell = np.repeat(np.repeat(cell, 4, 0), 4, 1)[:RES, :RES]
    fine_cell[-1, :] |= fine_cell[-2, :]
    fine_cell[:, -1] |= fine_cell[:, -2]
    mask = np.clip(blur(fine_cell.astype(np.float32), 1.0) * 1.5, 0.0, 1.0)
    mask = np.maximum(mask, fine_cell)
    native, dequant_max = native_nodes(raw)
    native -= datum
    coarse_runtime = tri_upsample(H1025)
    coarse_source = tri_upsample(H0 - datum)
    residual = native - coarse_source
    ground = coarse_runtime + (1.0 - mask) * residual

    edits = json.loads((dest / 'manifest_edits.json').read_text())
    buildings, wrecks = obstacles(edits)
    b_raster = rasterize_boxes(buildings)
    w_raster = rasterize_boxes(wrecks)
    obstacle = np.maximum(b_raster, w_raster)
    footprint = obstacle > 0
    sx = winstral_sx((ground + obstacle).astype(np.float32))
    sx = np.where(footprint, 0.0, sx)
    snow_src = np.array(Image.open(source / 'snow.png'), dtype=np.float32)
    if snow_src.max() > 1.0:
        snow_src /= 65535.0 if snow_src.max() > 255 else 255.0
    gx = (np.arange(RES) * (SRC - 1) / (RES - 1))
    gaea_snow = ndimage.map_coordinates(snow_src, np.meshgrid(gx, gx, indexing='ij'), order=1, mode='nearest')
    gaea_snow = np.clip(gaea_snow, 0.0, 1.0)
    dz, dx = np.gradient(ground, SP)
    normal_y = 1.0 / np.sqrt(1.0 + dx * dx + dz * dz)
    # Dry snow does not lie on faces much steeper than its repose (terrain_material.wgsl's rule).
    repose = np.clip((normal_y - 0.40) / 0.30, 0.0, 1.0)
    repose = repose * repose * (3 - 2 * repose)
    # Lee: deposition starts above a 2.5 deg horizon, so a drift behind an obstacle of height h
    # ends about 20 h downwind (snow-fence drifts reach 12-15 h at equilibrium); spread across the
    # wind by 2 m because the scan is one-dimensional. Windward: scour grows gently with exposure.
    lee = blur(np.maximum(sx - SX_LEE_ONSET_DEG, 0.0) / SX_LEE_DEG_PER_DEPTH, (LEE_SPREAD_M / SP, LEE_SPREAD_M / SP))
    scour = np.minimum(blur(sx, 1.0), 0.0) / SX_SCOUR_DEG_PER_DEPTH
    depth = SETTLED_DEPTH_M * (1.0 + lee + scour)
    depth = np.clip(depth, 0.0, DRIFT_MAX_M) * gaea_snow * repose

    route = edits['canyon_course']['route']
    offset = route_offsets(route)
    road = np.clip((ROAD_HALF_M + 1.5 - np.abs(offset)) / 1.5, 0.0, 1.0)
    depth = depth * (1.0 - road) + np.minimum(depth, ROAD_DEPTH_M) * road
    depth = np.where(footprint, 0.0, depth)
    # The redistributed snow changes the surface by its departure from the settled cover the Gaea
    # surface already carries; building and wreck footprints keep their ground.
    # A drift does not stand as a wall at the obstacle: the change ramps up over about 3 m.
    change = blur((depth - SETTLED_DEPTH_M * gaea_snow * repose) * (1.0 - footprint), 1.5)
    change = change * (1.0 - np.clip(blur(footprint.astype(np.float32), 2.5) * 2.0, 0.0, 1.0)) * (1.0 - mask)
    heights = (ground + change).astype(np.float32)

    cls = np.full((RES, RES), SNOW, np.uint8)
    cls[depth < 0.15] = CRUST
    cls[(depth < 0.03) | (normal_y < 0.55)] = ROCK
    near_building = ndimage.distance_transform_edt(b_raster <= 0) * SP
    cls[(near_building > 0) & (near_building < 8.0) & (cls == SNOW)] = TRAMPLED
    ruined = rasterize_boxes([(x, z, d + 6.0, w + 6.0, yaw, 1.0) for x, z, d, w, yaw, _ in buildings]) > 0
    cls[ruined] = RUBBLE
    cls[np.abs(offset) <= ROAD_HALF_M] = FROZEN_MUD

    surface = np.zeros((RES, RES, 4), np.uint8)
    # Bit 7 marks nodes held to the course's authored surface (building pads, receipt craters):
    # the runtime adds no procedural relief there, so foundations stand on what was cooked.
    surface[..., 0] = cls | np.where(mask > 0.5, 0x80, 0).astype(np.uint8)
    surface[..., 1] = np.clip(np.rint(depth * 100.0), 0, 255)
    surface[..., 2] = np.where(np.abs(offset) <= ROUTE_BAND_M,
                               np.clip(np.rint(offset * 10.0) + 127, 0, 254), 255)
    surface[..., 3] = np.clip(np.rint(sx * 4.0) + 128, 0, 255)

    out = dest / 'detail'
    out.mkdir(exist_ok=True)
    np.save(out / 'heights.4097.npy', np.ascontiguousarray(heights))
    Image.fromarray(surface, 'RGBA').save(out / 'surface.4097.png', optimize=False, compress_level=6)

    # Travel check on the course centreline with the Tiger's half-gauge either side.
    pts = np.asarray(route, float)
    def sample(x, z):
        gi, gj = world_to_node(x, z)
        return ndimage.map_coordinates(heights, [[gj], [gi]], order=1, mode='nearest')[0]
    grades = []
    for a, b in zip(pts[:-1], pts[1:]):
        grades.append(abs(sample(*b) - sample(*a)) / max(np.hypot(*(b - a)), 1e-6))
    classes = {CLASS_NAMES[c]: round(float((cls == c).mean()), 5) for c in range(len(CLASS_NAMES))}
    outputs = {name: sha256_file(out / name) for name in ['heights.4097.npy', 'surface.4097.png']}
    receipt = dict(
        schema='kessel.terrain-detail.receipt', version=1, tool=TOOL, tool_version=TOOL_VERSION,
        source_directory=str(source), source_height_sha256=source_sha,
        source_snow_sha256=sha256_file(source / 'snow.png'),
        coarse_heights_sha256=sha256_file(dest / 'heights.npy'),
        source_resolution=dict(pixels=SRC, spacing_m=EXTENT / (SRC - 1), extent_m=EXTENT,
                               elevation_range_m=[0, ELEVATION], quantisation_m=STEP_M,
                               distinct_values=int(len(np.unique(raw))),
                               dequantisation_max_change_m=round(dequant_max, 4)),
        grid=dict(res=RES, spacing_m=SP, coarse_res=COARSE, runtime_datum_m=datum),
        native_residual_vs_coarse_m=dict(std=round(float(residual.std()), 4),
                                         p99=round(float(np.percentile(np.abs(residual), 99)), 4),
                                         max=round(float(np.abs(residual).max()), 3)),
        authored_edits=dict(coarse_nodes=int(edited.sum()), fine_nodes_held=int((mask > 0.5).sum())),
        snow=dict(model='Winstral maximum upwind slope Sx, linear depth response (empirical, declared)',
                  wind_toward=WIND.round(4).tolist(), wind_source='src/world.rs weather_wind_profile(1)',
                  sx_reach_m=SX_REACH_M, settled_depth_m=SETTLED_DEPTH_M,
                  lee_onset_deg=SX_LEE_ONSET_DEG, lee_deg_per_settled_depth=SX_LEE_DEG_PER_DEPTH,
                  scour_deg_per_settled_depth=SX_SCOUR_DEG_PER_DEPTH, lee_spread_m=LEE_SPREAD_M, drift_max_m=DRIFT_MAX_M,
                  road_depth_m=ROAD_DEPTH_M, road_half_width_m=ROAD_HALF_M,
                  obstacles=dict(buildings=len(buildings), wrecks=len(wrecks),
                                 envelopes='box envelopes, wind shelter only',
                                 storey_m=STOREY_M, condition_height_share=CONDITION_HEIGHT),
                  depth_m=dict(mean=round(float(depth.mean()), 3),
                               p99=round(float(np.percentile(depth, 99)), 3),
                               max=round(float(depth.max()), 3)),
                  surface_change_m=dict(min=round(float(change.min()), 3),
                                        max=round(float(change.max()), 3)),
                  sx_deg=dict(p1=round(float(np.percentile(sx, 1)), 2),
                              p99=round(float(np.percentile(sx, 99)), 2))),
        classes=classes, class_names=CLASS_NAMES,
        surface_encoding='R class (bit 7: held authored surface, no relief), G snow depth cm, B route offset dm+127 (255 beyond 12.7 m), A Sx deg*4+128',
        route_max_grade=round(float(max(grades)), 3),
        outputs=outputs, seconds=round(time.time() - started, 1))
    (out / 'terrain-detail.receipt.json').write_text(dumps(receipt))

    manifest_path = dest / 'manifest.v1.json'
    text = manifest_path.read_text(encoding='utf-8')
    manifest = json.loads(text)
    if dumps(manifest, compact=True) != text:
        raise ValueError('manifest.v1.json does not round-trip; refusing to rewrite it')
    manifest['terrain']['detail'] = dict(
        schema='kessel.terrain-detail', version=1, res=RES, spacing=SP,
        heights='detail/heights.4097.npy', surface='detail/surface.4097.png',
        receipt='detail/terrain-detail.receipt.json', classes=CLASS_NAMES,
        heights_sha256=outputs['heights.4097.npy'], surface_sha256=outputs['surface.4097.png'])
    tmp = manifest_path.with_suffix('.json.tmp')
    tmp.write_text(dumps(manifest, compact=True), encoding='utf-8')
    os.replace(tmp, manifest_path)
    return receipt


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--source', type=Path, default=next((p for p in SOURCES if p.exists()), SOURCES[0]))
    ap.add_argument('--root', type=Path, default=Path(os.environ.get('KESSEL_ASSET_ROOT', ROOT)))
    args = ap.parse_args()
    receipt = build(args.source, args.root)
    print(json.dumps({k: receipt[k] for k in ('source_resolution', 'native_residual_vs_coarse_m',
                                             'authored_edits', 'snow', 'classes', 'route_max_grade',
                                             'seconds')}, indent=1))


if __name__ == '__main__':
    main()
