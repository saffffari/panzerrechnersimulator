#!/usr/bin/env python3
"""Authored winter surface wear over the unchanged Gaea rock and snow inputs.

The lane and yards record prior traffic / destruction as substrate and snow coverage.
They are material authoring, not newly simulated ruts, heat, or runtime track imprints.
Coordinates use the same 5 km XZ extent and image orientation as terrain.rs.
"""
import math

import numpy as np

from canyon_ground_splatter import seed_at


def noise(x, z, scale, seed):
    """World-fixed smooth value noise; material authoring, not a simulated process."""
    x, z = x/scale, z/scale
    ix, iz = np.floor(x).astype(np.int64), np.floor(z).astype(np.int64)
    tx, tz = x-ix, z-iz
    tx, tz = tx*tx*(3-2*tx), tz*tz*(3-2*tz)
    def value(a, b):
        n = (a*374761393 + b*668265263 + seed*1442695041) & 0xffffffff
        n = ((n ^ (n >> 13))*1274126177) & 0xffffffff
        return ((n ^ (n >> 16)) & 0xffffff) / 16777215.
    a, b, c, d = value(ix, iz), value(ix+1, iz), value(ix, iz+1), value(ix+1, iz+1)
    return np.asarray((a*(1-tx)+b*tx)*(1-tz)+(c*(1-tx)+d*tx)*tz, dtype=np.float32)


def restrict_stalks(manifest, vegetation, extent=5000.):
    """Keep separate dry-stalk props within the same sparse surviving grass pockets."""
    placements = []
    before, after = 0, 0
    for p in manifest['placements']:
        if manifest['assets'][p[0]].endswith('/wheat_1.glb'):
            before += 1
            x, z = p[1], p[3]
            ix, iz = np.clip(np.rint((np.array([x,z])+extent/2)/extent*(len(vegetation)-1)),0,len(vegetation)-1).astype(int)
            if vegetation[iz,ix] < 128 or seed_at(x,z) % 5:
                continue
            after += 1
        placements.append(p)
    manifest['placements'] = placements
    return dict(stalks_before=before, stalks_after=after)


def author(rock, snow, route, buildings, extent=5000., heights=None):
    rock = np.asarray(rock, dtype=np.float32).copy()
    snow = np.asarray(snow, dtype=np.float32).copy()
    size = snow.shape[0]
    spacing = extent / (size - 1)
    axis = np.linspace(-extent / 2, extent / 2, size, dtype=np.float32)
    traffic = np.zeros_like(snow)
    yards = np.zeros_like(snow)
    vegetation = np.zeros_like(snow)
    route = np.asarray(route, dtype=float)

    def window(x0, x1, z0, z1):
        ix0 = max(0, int(math.floor((x0 + extent / 2) / spacing)))
        ix1 = min(size, int(math.ceil((x1 + extent / 2) / spacing)) + 1)
        iz0 = max(0, int(math.floor((z0 + extent / 2) / spacing)))
        iz1 = min(size, int(math.ceil((z1 + extent / 2) / spacing)) + 1)
        return (slice(iz0, iz1), slice(ix0, ix1)), np.meshgrid(axis[ix0:ix1], axis[iz0:iz1])

    def stroke(a, b, width, strength):
        radius = width / 2
        margin = radius + spacing*2
        sl, (x, z) = window(min(a[0], b[0])-margin, max(a[0], b[0])+margin,
                            min(a[1], b[1])-margin, max(a[1], b[1])+margin)
        dx, dz = b-a
        t = np.clip(((x-a[0])*dx+(z-a[1])*dz) / max(dx*dx+dz*dz, 1e-6), 0, 1)
        dist = np.hypot(x-a[0]-t*dx, z-a[1]-t*dz)
        # Integrate the narrow marks over an approximate texel footprint; no stair-step binary mask.
        ragged = (noise(x, z, 3.8, 23)-.5)*2.1 + (noise(x, z, 1.6, 37)-.5)*.8
        mask = np.clip((radius + ragged + spacing*.55-dist) / (spacing*1.1), 0, 1)
        # Churned snow and exposed earth intermingle instead of an airbrushed grey strip.
        broken = .74 + .26*noise(x, z, 2.3, 59)
        traffic[sl] = np.maximum(traffic[sl], mask*strength*broken)

    tangents = np.gradient(route, axis=0)
    tangents /= np.linalg.norm(tangents, axis=1)[:, None]
    normals = np.column_stack([-tangents[:, 1], tangents[:, 0]])
    for i, (a, b) in enumerate(zip(route[:-1], route[1:])):
        # A connected used lane with an irregular snow shoulder, plus two darker traffic bands.
        width = 7.2 + .8*math.sin(i*.071) + .45*math.sin(i*.233)
        stroke(a, b, width, .76)
        for side in (-1, 1):
            aa = a + normals[i]*side*1.45
            bb = b + normals[i+1]*side*1.45
            stroke(aa, bb, 1.4, .98)

    for b in buildings:
        center = np.array([b['x'], b['z']])
        nearest = int(np.argmin(np.sum((route-center)**2, axis=1)))
        # The yard shares its actual building orientation, instead of a random circular stain.
        rx, rz = b['width_m']*.5+6., b['depth_m']*.5+7.
        a = math.radians(b['yaw_deg']); co, si = math.cos(a), math.sin(a)
        radius = math.hypot(rx, rz)+6
        sl, (x, z) = window(center[0]-radius, center[0]+radius, center[1]-radius, center[1]+radius)
        xx, zz = x-center[0], z-center[1]
        lx, lz = xx*co-zz*si, xx*si+zz*co
        border = np.maximum(np.abs(lx)/rx, np.abs(lz)/rz)
        # Broad, fixed variations represent remaining drifts among exposed soil / burned debris.
        broken = (noise(x, z, 4.6, 71)-.5)*.75 + (noise(x, z, 1.7, 79)-.5)*.25
        mask = np.clip((1.16+broken-border)/.30, 0, 1)
        patch = .47 + .49*noise(x, z, 3.2, 83)
        yards[sl] = np.maximum(yards[sl], mask*patch)
        delta = center-route[nearest]
        distance = np.linalg.norm(delta)
        if distance > 1:
            # An access lane terminates at the lot, not through its surviving house walls.
            end = center-delta/distance*min(rx, rz)
            stroke(route[nearest], end, 3.3, .86)

    # A few surviving stubble pockets on the shoulders / behind the occupied plots.
    # Blank means no grass, rather than filling every unclassified snow texel.
    cumulative = np.r_[0., np.cumsum(np.linalg.norm(np.diff(route, axis=0), axis=1))]
    rng = np.random.default_rng(19430120)
    pockets = 0
    for distance in np.arange(45., cumulative[-1], 62.):
        j = min(int(np.searchsorted(cumulative, distance)), len(route)-1)
        for side in (-1, 1):
            if rng.random() > .57:
                continue
            p = route[j]+normals[j]*side*rng.uniform(27, 125)
            rx, rz = rng.uniform(12, 22), rng.uniform(8, 15)
            sl, (x, z) = window(p[0]-rx*1.35, p[0]+rx*1.35, p[1]-rz*1.35, p[1]+rz*1.35)
            edge = np.sqrt(((x-p[0])/rx)**2 + ((z-p[1])/rz)**2)
            broken = (noise(x, z, 5.7, 101)-.5)*.45
            pocket = np.clip((1+broken-edge)/.24, 0, 1)
            vegetation[sl] = np.maximum(vegetation[sl], pocket)
            pockets += 1

    wear = np.maximum(traffic, yards)
    # Ragged wind-scoured exposure on banks. It follows actual slope and broad
    # material patches; valley snow remains continuous between used plots.
    bank = np.zeros_like(snow)
    if heights is not None:
        from PIL import Image
        hs = extent/(heights.shape[0]-1)
        dz, dx = np.gradient(heights, hs)
        slope = np.hypot(dx, dz).astype(np.float32)
        slope = np.asarray(Image.fromarray(slope).resize((size, size), Image.Resampling.BILINEAR))
        x, z = axis[None, :], axis[:, None]
        exposure = np.clip((noise(x, z, 24., 113)*.72 + noise(x, z, 6., 127)*.28-.47)/.35, 0, 1)
        bank = np.clip((slope-.10)/.30, 0, 1)*exposure*.72
    vegetation *= np.clip((.20-wear)/.20, 0, 1)
    # Little exposed litter beneath the dense pockets, with a soft broken margin.
    litter = vegetation*.23
    snow *= 1-np.maximum(wear, np.maximum(bank, litter))
    # Source rock is preserved outside occupied / travelled plots. Inside, exposed local earth
    # and ash replace substrate albedo; coverage independently reveals that material.
    grain = (noise(axis[None, :], axis[:, None], 2.7, 139)-.5)*.32
    soil = np.array([89., 81., 70.], dtype=np.float32)
    ash = np.array([60., 58., 55.], dtype=np.float32)
    for c in range(3):
        target = soil[c]*(1-yards)+ash[c]*yards
        target *= 1+grain
        rock[:, :, c] = rock[:, :, c]*(1-wear)+target*wear
    assert np.isfinite(snow).all() and snow.min() >= 0 and snow.max() <= 1
    vegetation = np.uint8(np.rint(255*vegetation))
    return np.uint8(np.rint(np.clip(rock, 0, 255))), snow, vegetation, dict(
        authoring='Broken prior traffic, ash yards and scoured-bank coverage; not dynamic ruts or simulated snow transport.',
        road_width_m=[5.95, 8.45], traffic_band_width_m=1.4, traffic_band_centers_m=[-1.45, 1.45],
        texel_spacing_m=spacing, modified_texels=int(np.count_nonzero(wear > .01)),
        vegetation_pockets=pockets, vegetation_area_fraction=float(np.mean(vegetation > 64)),
        bank_exposed_texels=int(np.count_nonzero(bank > .1)),
        surface_geometry_changed=False, building_yards=len(buildings))
