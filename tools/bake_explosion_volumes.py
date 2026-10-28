#!/usr/bin/env python3
"""Bake OpenVDB explosion and fire volumes into the raymarch backend's explosion atlas.

Run with Blender (its bundled ``openvdb`` module reads the VDBs) from the repository root::

    blender --background --factory-startup --python-exit-code 1 \
      --python tools/bake_explosion_volumes.py -- \
      "E:/3D/library/volumes/Cloudscapes V2 PRO/volumes" assets/vfx/explosion_volumes \
      burst:Ground_explosion_21 burst:Ground_explosion_22 ... cloud:Ground_explosion_1 ...

Each ROLE:NAME selects ``NAME.vdb`` for a role: ``burst``, a fresh ground burst (fire, ejecta
streaks, the first second of a detonation), or ``cloud``, a developed cloud of soot and lifted earth
(the seconds after); the runtime cross-fades one of each per detonation. The selection made from
side projections of all thirty (2026-10-28) is burst 21, 22, 23, 25, 26, 29 and cloud 1, 3, 4, 6,
8, 9 of the
Cloudscapes V2 PRO ``Ground_explosion_N`` volumes (the tall columns and mushrooms are bomb-scale,
the ``Fire ball`` volumes are streaking trails). The density and flames grids (temperature when a file has no flames) are cropped to
their joint active bounds, box-resampled into one 64^3 slot with Y up (the source is Blender's Z
up), and each channel is normalised by the 99.8th percentile of its non-zero voxels. The slots stack
along the atlas's depth: ``atlas.bin`` holds 64 x 64 x (64 n) voxels of two bytes (density, flame),
x fastest, then y (up), then z; ``atlas.json`` records for each slot its source file, the source's
voxel size and active bounds, the shape's extent in source units and its aspect (extent over the
largest extent), and the normalisation. The runtime (``src/explosion_volumes.rs``) sizes and times
the shapes from the simulation's blast; the source's own scale is recorded, not used.

The source pack is licensed to the owner; the atlas is a local runtime asset (``/assets/*`` is not
tracked) and is rebuilt from the library with this script.
"""

from __future__ import annotations

import json
from pathlib import Path
import sys

import numpy as np
import openvdb

SLOT = 64


def box_matrix(n_in: int, n_out: int) -> np.ndarray:
    """An n_out x n_in matrix averaging the input cells each output cell covers (area weights)."""
    m = np.zeros((n_out, n_in), dtype=np.float32)
    scale = n_in / n_out
    for o in range(n_out):
        lo, hi = o * scale, (o + 1) * scale
        for i in range(int(np.floor(lo)), min(int(np.ceil(hi)), n_in)):
            w = min(hi, i + 1) - max(lo, i)
            if w > 0:
                m[o, i] = w
        m[o] /= max(m[o].sum(), 1e-9)
    return m


def resample(dense: np.ndarray) -> np.ndarray:
    """(nx, ny, nz) source index order, Z up -> (SLOT z, SLOT y_up, SLOT x) atlas order."""
    nx, ny, nz = dense.shape
    a = np.tensordot(box_matrix(nx, SLOT), dense, axes=(1, 0))          # (64, ny, nz)
    a = np.tensordot(a, box_matrix(ny, SLOT), axes=(1, 1))              # (64, nz, 64)
    a = np.tensordot(a, box_matrix(nz, SLOT), axes=(1, 1))              # (64, 64, 64)
    # a is indexed (x, source y, source z) after the three products. The atlas wants z (depth) =
    # source y (horizontal), y (rows, up) = source z, x = source x: order (z, y, x).
    return np.ascontiguousarray(np.transpose(a, (1, 2, 0)))


def active_bounds(grid):
    lo, hi = grid.evalActiveVoxelBoundingBox()
    if lo[0] > hi[0]:
        return None
    return np.array(lo), np.array(hi)


def dense_of(grid, lo, hi) -> np.ndarray:
    shape = tuple(int(v) for v in (hi - lo + 1))
    arr = np.zeros(shape, dtype=np.float32)
    grid.copyToArray(arr, ijk=tuple(int(v) for v in lo))
    return np.maximum(arr, 0.0)


def normalise(channel: np.ndarray) -> tuple[np.ndarray, float]:
    nz = channel[channel > 1e-6]
    if nz.size == 0:
        return np.zeros_like(channel), 0.0
    top = float(np.percentile(nz, 99.8))
    return np.clip(channel / max(top, 1e-9), 0.0, 1.0), top


def bake(source_dir: Path, out_dir: Path, picks: list[tuple[str, str]]) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    voxels = []
    slots = []
    for role, name in picks:
        path = source_dir / f"{name}.vdb"
        grids, _ = openvdb.readAll(str(path))
        by = {g.name: g for g in grids}
        density = by.get("density")
        flame = by.get("flames") or by.get("flame") or by.get("temperature")
        bounds = [b for b in (active_bounds(g) for g in (density, flame) if g is not None) if b]
        if not bounds:
            raise SystemExit(f"{name}: no active voxels")
        lo = np.min([b[0] for b in bounds], axis=0)
        hi = np.max([b[1] for b in bounds], axis=0)
        channels = []
        norms = []
        for g in (density, flame):
            if g is None or active_bounds(g) is None:
                channels.append(np.zeros((SLOT, SLOT, SLOT), dtype=np.float32))
                norms.append(0.0)
                continue
            c, top = normalise(resample(dense_of(g, lo, hi)))
            channels.append(c)
            norms.append(top)
        voxel = float(density.transform.voxelSize()[0] if density is not None else flame.transform.voxelSize()[0])
        size = (hi - lo + 1).astype(float) * voxel
        # Atlas axes: x = source x, y (up) = source z, z = source y.
        extent = [size[0], size[2], size[1]]
        largest = max(extent)
        voxels.append(np.stack(channels, axis=-1))
        slots.append({
            "role": role,
            "source": path.name,
            "voxel_size": voxel,
            "active_index_bounds": [lo.tolist(), hi.tolist()],
            "extent_source_units": extent,
            "aspect": [e / largest for e in extent],
            "density_norm": norms[0],
            "flame_norm": norms[1],
            "flame_grid": flame.name if flame is not None else None,
        })
        print(f"[bake] {role:8s} {name}: extent {extent} norms {norms}", flush=True)
    atlas = np.concatenate(voxels, axis=0)  # (64 n, 64, 64, 2)
    (out_dir / "atlas.bin").write_bytes((atlas * 255.0 + 0.5).astype(np.uint8).tobytes())
    (out_dir / "atlas.json").write_text(json.dumps({
        "schema": "kessel.explosion-volumes.v1",
        "slot": SLOT,
        "count": len(slots),
        "layout": "u8 density, u8 flame; x fastest, then y (up), then z; slots stacked in z",
        "source_dir": str(source_dir),
        "slots": slots,
    }, indent=2))
    print(f"[bake] wrote {len(slots)} slots to {out_dir}", flush=True)


def main(argv: list[str]) -> None:
    args = argv[argv.index("--") + 1:] if "--" in argv else argv[1:]
    if len(args) < 3:
        raise SystemExit(__doc__)
    picks = []
    for spec in args[2:]:
        role, _, name = spec.partition(":")
        if role not in ("burst", "cloud") or not name:
            raise SystemExit(f"bad pick {spec!r}: ROLE:NAME with ROLE burst or cloud")
        picks.append((role, name))
    bake(Path(args[0]), Path(args[1]), picks)


if __name__ == "__main__":
    main(sys.argv)
