#!/usr/bin/env python3
"""Authored winter surface wear over the unchanged Gaea rock and snow inputs.

The lane and yards record prior traffic / destruction as substrate and snow coverage.
They are material authoring, not newly simulated ruts, heat, or runtime track imprints.
Coordinates use the same 5 km XZ extent and image orientation as terrain.rs.
"""
import math

import numpy as np


def author(rock, snow, route, buildings, extent=5000.):
    rock = np.asarray(rock, dtype=np.float32).copy()
    snow = np.asarray(snow, dtype=np.float32).copy()
    size = snow.shape[0]
    spacing = extent / (size - 1)
    axis = np.linspace(-extent / 2, extent / 2, size, dtype=np.float32)
    traffic = np.zeros_like(snow)
    yards = np.zeros_like(snow)
    route = np.asarray(route, dtype=float)

    def window(x0, x1, z0, z1):
        ix0 = max(0, int(math.floor((x0 + extent / 2) / spacing)))
        ix1 = min(size, int(math.ceil((x1 + extent / 2) / spacing)) + 1)
        iz0 = max(0, int(math.floor((z0 + extent / 2) / spacing)))
        iz1 = min(size, int(math.ceil((z1 + extent / 2) / spacing)) + 1)
        return (slice(iz0, iz1), slice(ix0, ix1)), np.meshgrid(axis[ix0:ix1], axis[iz0:iz1])

    def stroke(a, b, width, strength):
        radius = width / 2
        margin = radius + spacing
        sl, (x, z) = window(min(a[0], b[0])-margin, max(a[0], b[0])+margin,
                            min(a[1], b[1])-margin, max(a[1], b[1])+margin)
        dx, dz = b-a
        t = np.clip(((x-a[0])*dx+(z-a[1])*dz) / max(dx*dx+dz*dz, 1e-6), 0, 1)
        dist = np.hypot(x-a[0]-t*dx, z-a[1]-t*dz)
        # Integrate the narrow marks over an approximate texel footprint; no stair-step binary mask.
        mask = np.clip((radius + spacing*.65-dist) / (spacing*1.3), 0, 1)
        traffic[sl] = np.maximum(traffic[sl], mask*strength)

    tangents = np.gradient(route, axis=0)
    tangents /= np.linalg.norm(tangents, axis=1)[:, None]
    normals = np.column_stack([-tangents[:, 1], tangents[:, 0]])
    for i, (a, b) in enumerate(zip(route[:-1], route[1:])):
        # A connected used lane with an irregular snow shoulder, plus two darker traffic bands.
        width = 6.8 + .8*math.sin(i*.071) + .45*math.sin(i*.233)
        stroke(a, b, width, .63)
        for side in (-1, 1):
            aa = a + normals[i]*side*1.45
            bb = b + normals[i+1]*side*1.45
            stroke(aa, bb, 1.25, .91)

    for b in buildings:
        center = np.array([b['x'], b['z']])
        nearest = int(np.argmin(np.sum((route-center)**2, axis=1)))
        # The yard shares its actual building orientation, instead of a random circular stain.
        rx, rz = b['width_m']*.5+4., b['depth_m']*.5+5.
        a = math.radians(b['yaw_deg']); co, si = math.cos(a), math.sin(a)
        radius = math.hypot(rx, rz)+6
        sl, (x, z) = window(center[0]-radius, center[0]+radius, center[1]-radius, center[1]+radius)
        xx, zz = x-center[0], z-center[1]
        lx, lz = xx*co-zz*si, xx*si+zz*co
        border = np.maximum(np.abs(lx)/rx, np.abs(lz)/rz)
        # Broad, fixed variations represent remaining drifts among exposed soil / burned debris.
        broken = .13*np.sin(x*.51+np.sin(z*.19)) + .10*np.cos(z*.43-x*.21)
        mask = np.clip((1.18+broken-border)/.38, 0, 1)
        patch = .48+.26*(.5+.5*np.sin(x*.31+np.cos(z*.28))*np.cos(z*.37))
        yards[sl] = np.maximum(yards[sl], mask*patch)
        delta = center-route[nearest]
        distance = np.linalg.norm(delta)
        if distance > 1:
            # An access lane terminates at the lot, not through its surviving house walls.
            end = center-delta/distance*min(rx, rz)
            stroke(route[nearest], end, 2.9, .73)

    wear = np.maximum(traffic, yards)
    # Source rock is preserved outside occupied / travelled plots. Inside, exposed local earth
    # and ash replace substrate albedo; coverage independently reveals that material.
    grain = np.sin(axis[None, :]*.27+np.cos(axis[:, None]*.33))*.04
    soil = np.array([93., 85., 73.], dtype=np.float32)
    ash = np.array([76., 72., 66.], dtype=np.float32)
    for c in range(3):
        target = soil[c]*(1-yards)+ash[c]*yards
        target *= 1+grain
        rock[:, :, c] = rock[:, :, c]*(1-wear)+target*wear
    snow *= 1-wear
    assert np.isfinite(snow).all() and snow.min() >= 0 and snow.max() <= 1
    vegetation = np.uint8(np.rint(255*np.clip((.30-wear)/.30, 0, 1)))
    return np.uint8(np.rint(np.clip(rock, 0, 255))), snow, vegetation, dict(
        authoring='Prior traffic and ruined-yard material coverage; not dynamic rut simulation.',
        road_width_m=[5.55, 8.05], traffic_band_width_m=1.25, traffic_band_centers_m=[-1.45, 1.45],
        texel_spacing_m=spacing, modified_texels=int(np.count_nonzero(wear > .01)),
        surface_geometry_changed=False, building_yards=len(buildings))
