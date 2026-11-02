#!/usr/bin/env python3
"""Voxelise the measured Tiger CAD components into compact occupancy grids for the damage chain.

Plain Python 3 + numpy, no FreeCAD, no GPU. Run from the repository root after
tools/tiger_cad_register.py has written the per-component meshes:

    python3 -I tools/tiger_cad_voxelize.py [--cell 0.02] [--meshes assets/fea/tiger_cad/components]
                                           [--catalog catalog/tiger_cad_components.v1.json]
                                           [--out catalog/tiger_cad_component_voxels.v1.json]

Input: assets/fea/tiger_cad/components/<id>.obj (gitignored; engine metres; hull components in the
hull frame, turret components turret-local at 0 deg gun elevation), one `o` object per kit solid.
Each kit solid is a closed tessellation of a B-rep solid, so a vertical ray through a column of cells
crosses it an even number of times; the cells whose centres lie between an entry and the following
exit are inside that solid (ray parity, per solid; a component is the union of its solids).

Output: catalog/tiger_cad_component_voxels.v1.json (small, committed). Per component: frame, grid
origin (the centre of cell 0,0,0 is origin + cell/2), cell size, dimensions (nx, ny, nz), and the
occupancy run-length encoded over the flattened order index = (ix * nz + iz) * ny + iy (columns along
+Y), alternating empty/full run lengths starting with an empty run, each length as an unsigned LEB128
varint, base64. Also the voxel volume against the exact solid volume from the catalog, and the
geometric bound: any voxel boundary lies within half a cell diagonal of the true surface (the column
parity test is exact at cell centres up to the tessellation chord the catalog records).

Deterministic: the column centres are offset by a fixed irrational fraction of a cell so that no ray
passes exactly through a tessellation vertex or edge; no clocks or random state reach the output.
"""
import argparse
import base64
import json
import math
import os
import sys

import numpy as np

# Column-centre offset (fraction of a cell) keeping rays off the cell-aligned tessellation features.
JITTER = (0.5 + 1.0e-4 * math.sqrt(2.0), 0.5 + 1.0e-4 * math.sqrt(3.0))


def read_obj(path):
    """Return a list of (name, vertices[n,3], faces[m,3]) per `o` object; face indices are local."""
    objects = []
    verts = []
    name = None
    faces = []
    offset = 0  # OBJ indices are global and 1-based
    local_start = 0

    def flush():
        if name is not None and faces:
            f = np.asarray(faces, dtype=np.int64) - 1
            used = np.unique(f)
            remap = np.full(len(verts) + 1, -1, dtype=np.int64)
            remap[used] = np.arange(len(used))
            v = np.asarray(verts, dtype=np.float64)[used]
            objects.append((name, v, remap[f]))

    with open(path, "r", encoding="utf-8") as fh:
        for line in fh:
            if line.startswith("v "):
                parts = line.split()
                verts.append((float(parts[1]), float(parts[2]), float(parts[3])))
            elif line.startswith("f "):
                idx = [int(p.split("/")[0]) for p in line.split()[1:]]
                for k in range(1, len(idx) - 1):  # fan-triangulate polygons
                    faces.append((idx[0], idx[k], idx[k + 1]))
            elif line.startswith("o "):
                flush()
                name = line[2:].strip()
                faces = []
    flush()
    return objects


def column_hits(tris, origin, cell, nx, nz):
    """For every triangle and every column centre inside its XZ projection, the hit height y.

    Returns (column_index, y) arrays. Columns are indexed ix * nz + iz.
    """
    a, b, c = tris[:, 0], tris[:, 1], tris[:, 2]
    xmin = np.minimum(np.minimum(a[:, 0], b[:, 0]), c[:, 0])
    xmax = np.maximum(np.maximum(a[:, 0], b[:, 0]), c[:, 0])
    zmin = np.minimum(np.minimum(a[:, 2], b[:, 2]), c[:, 2])
    zmax = np.maximum(np.maximum(a[:, 2], b[:, 2]), c[:, 2])
    # Column centre x = origin.x + (ix + JITTER0) * cell
    ix0 = np.ceil((xmin - origin[0]) / cell - JITTER[0]).astype(np.int64)
    ix1 = np.floor((xmax - origin[0]) / cell - JITTER[0]).astype(np.int64)
    iz0 = np.ceil((zmin - origin[2]) / cell - JITTER[1]).astype(np.int64)
    iz1 = np.floor((zmax - origin[2]) / cell - JITTER[1]).astype(np.int64)
    ix0 = np.clip(ix0, 0, nx - 1)
    ix1 = np.clip(ix1, -1, nx - 1)
    iz0 = np.clip(iz0, 0, nz - 1)
    iz1 = np.clip(iz1, -1, nz - 1)
    nxs = np.maximum(ix1 - ix0 + 1, 0)
    nzs = np.maximum(iz1 - iz0 + 1, 0)
    counts = nxs * nzs
    keep = counts > 0
    if not keep.any():
        return np.zeros(0, dtype=np.int64), np.zeros(0)
    tri_index = np.repeat(np.nonzero(keep)[0], counts[keep])
    # Enumerate the candidate columns of each triangle.
    starts = np.cumsum(counts[keep]) - counts[keep]
    local = np.arange(len(tri_index)) - np.repeat(starts, counts[keep])
    nz_rep = np.repeat(nzs[keep], counts[keep])
    cx = np.repeat(ix0[keep], counts[keep]) + local // nz_rep
    cz = np.repeat(iz0[keep], counts[keep]) + local % nz_rep
    px = origin[0] + (cx + JITTER[0]) * cell
    pz = origin[2] + (cz + JITTER[1]) * cell
    ta, tb, tc = a[tri_index], b[tri_index], c[tri_index]
    # Barycentric test in XZ.
    v0x, v0z = tb[:, 0] - ta[:, 0], tb[:, 2] - ta[:, 2]
    v1x, v1z = tc[:, 0] - ta[:, 0], tc[:, 2] - ta[:, 2]
    v2x, v2z = px - ta[:, 0], pz - ta[:, 2]
    den = v0x * v1z - v1x * v0z
    ok = np.abs(den) > 1e-18
    den = np.where(ok, den, 1.0)
    u = (v2x * v1z - v1x * v2z) / den
    v = (v0x * v2z - v2x * v0z) / den
    inside = ok & (u >= 0.0) & (v >= 0.0) & (u + v <= 1.0)
    y = ta[:, 1] + u * (tb[:, 1] - ta[:, 1]) + v * (tc[:, 1] - ta[:, 1])
    col = cx * nz + cz
    return col[inside], y[inside]


def voxelise_component(objects, cell):
    allv = np.concatenate([v for _, v, _ in objects])
    lo = allv.min(axis=0)
    hi = allv.max(axis=0)
    origin = np.floor(lo / cell) * cell - cell
    dims = np.ceil((hi - origin) / cell).astype(np.int64) + 1
    nx, ny, nz = (int(d) for d in dims)
    occ = np.zeros((nx, nz, ny), dtype=bool)
    odd_columns = 0
    for name, v, f in objects:
        tris = v[f]
        col, y = column_hits(tris, origin, cell, nx, nz)
        if len(col) == 0:
            continue
        order = np.lexsort((y, col))
        col, y = col[order], y[order]
        # Group by column.
        boundaries = np.nonzero(np.diff(col))[0] + 1
        groups_start = np.concatenate(([0], boundaries))
        groups_end = np.concatenate((boundaries, [len(col)]))
        for s, e in zip(groups_start, groups_end):
            ys = y[s:e]
            if len(ys) % 2 == 1:
                # A ray grazing a tessellation seam; drop the column's last crossing (counted).
                odd_columns += 1
                ys = ys[:-1]
            c = int(col[s])
            ix, iz = divmod(c, nz)
            for k in range(0, len(ys), 2):
                y0 = (ys[k] - origin[1]) / cell - 0.5
                y1 = (ys[k + 1] - origin[1]) / cell - 0.5
                j0 = max(int(math.ceil(y0)), 0)
                j1 = min(int(math.floor(y1)), ny - 1)
                if j1 >= j0:
                    occ[ix, iz, j0:j1 + 1] = True
    return origin, (nx, ny, nz), occ, odd_columns


def rle_encode(flat):
    runs = []
    current = False
    length = 0
    for value in flat:
        if bool(value) == current:
            length += 1
        else:
            runs.append(length)
            current = not current
            length = 1
    runs.append(length)
    out = bytearray()
    for r in runs:
        while True:
            byte = r & 0x7F
            r >>= 7
            if r:
                out.append(byte | 0x80)
            else:
                out.append(byte)
                break
    return base64.b64encode(bytes(out)).decode("ascii"), len(runs)


def rle_encode_fast(flat):
    flat = np.asarray(flat, dtype=bool)
    if len(flat) == 0:
        return rle_encode([])
    change = np.nonzero(np.diff(flat.astype(np.int8)))[0] + 1
    edges = np.concatenate(([0], change, [len(flat)]))
    lengths = np.diff(edges).tolist()
    if flat[0]:
        lengths = [0] + lengths
    out = bytearray()
    for r in lengths:
        while True:
            byte = r & 0x7F
            r >>= 7
            if r:
                out.append(byte | 0x80)
            else:
                out.append(byte)
                break
    return base64.b64encode(bytes(out)).decode("ascii"), len(lengths)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cell", type=float, default=0.02)
    ap.add_argument("--meshes", default="assets/fea/tiger_cad/components")
    ap.add_argument("--catalog", default="catalog/tiger_cad_components.v1.json")
    ap.add_argument("--out", default="catalog/tiger_cad_component_voxels.v1.json")
    args = ap.parse_args()
    with open(args.catalog, "r", encoding="utf-8") as fh:
        catalog = json.load(fh)
    cell = args.cell
    result = {
        "schema": "panzer.tiger-cad-component-voxels.v1",
        "generated_by": "tools/tiger_cad_voxelize.py",
        "source_catalog": args.catalog,
        "cell_m": cell,
        "encoding": ("occupancy over index (ix*nz+iz)*ny+iy, alternating empty/full run lengths "
                     "starting empty, unsigned LEB128 varints, base64"),
        "cell_centre": "origin_m + (i + 0.5) * cell_m",
        "surface_bound_m": cell * math.sqrt(3.0) / 2.0,
        "frames": catalog["frames"],
        "components": [],
    }
    for comp in catalog["components"]:
        path = os.path.join(args.meshes, comp["id"] + ".obj")
        objects = read_obj(path)
        origin, dims, occ, odd = voxelise_component(objects, cell)
        nx, ny, nz = dims
        flat = occ.reshape(-1)  # (ix, iz, iy) order == (ix*nz+iz)*ny+iy
        data, runs = rle_encode_fast(flat)
        voxel_volume = float(flat.sum()) * cell ** 3
        exact = comp["volume_m3"]["solids"]
        entry = {
            "id": comp["id"],
            "frame": comp["frame"],
            "origin_m": [round(float(o), 6) for o in origin],
            "dims": [nx, ny, nz],
            "occupied_cells": int(flat.sum()),
            "voxel_volume_m3": round(voxel_volume, 6),
            "solid_volume_m3": exact,
            "volume_ratio": round(voxel_volume / exact, 4) if exact > 0 else None,
            "odd_parity_columns": odd,
            "runs": runs,
            "rle_b64": data,
        }
        if comp.get("elevates_with_gun"):
            entry["elevates_with_gun"] = comp["elevates_with_gun"]
        result["components"].append(entry)
        print(f"{comp['id']:28s} dims {nx:3d}x{ny:3d}x{nz:3d} cells {int(flat.sum()):7d} "
              f"vol {voxel_volume:.4f}/{exact:.4f} ratio {voxel_volume/exact:.3f} odd {odd} "
              f"b64 {len(data)}")
    with open(args.out, "w", encoding="utf-8", newline="\n") as fh:
        json.dump(result, fh, indent=1)
        fh.write("\n")
    print("wrote", args.out, os.path.getsize(args.out), "bytes")


if __name__ == "__main__":
    sys.exit(main())
