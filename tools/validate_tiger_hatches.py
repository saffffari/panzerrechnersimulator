#!/usr/bin/env python3
"""CPU-only conservation and aperture checks for source-root Tiger roof-lid partitions."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sys

import numpy as np

from fit_crew_steel_helmet import accessor, read_glb

ROOT = Path(__file__).resolve().parents[1]


def matrix(node: dict) -> np.ndarray:
    if "matrix" in node:
        return np.array(node["matrix"], dtype=float).reshape(4, 4).T
    x, y, z, w = node.get("rotation", (0, 0, 0, 1))
    r = np.array([
        [1-2*(y*y+z*z), 2*(x*y-z*w), 2*(x*z+y*w)],
        [2*(x*y+z*w), 1-2*(x*x+z*z), 2*(y*z-x*w)],
        [2*(x*z-y*w), 2*(y*z+x*w), 1-2*(x*x+y*y)],
    ])
    out = np.eye(4)
    out[:3, :3] = r @ np.diag(node.get("scale", (1, 1, 1)))
    out[:3, 3] = node.get("translation", (0, 0, 0))
    return out


def vertical_hits(vertices: np.ndarray, indices: np.ndarray, ray: tuple) -> list[float]:
    x, z, bottom, top = ray
    if not (vertices[:, 0].min() <= x <= vertices[:, 0].max()
            and vertices[:, 2].min() <= z <= vertices[:, 2].max()):
        return []
    tri = vertices[indices.reshape(-1, 3)]
    a, b, c = tri[:, 0], tri[:, 1] - tri[:, 0], tri[:, 2] - tri[:, 0]
    det = b[:, 0] * c[:, 2] - b[:, 2] * c[:, 0]
    valid = np.abs(det) > 1e-12
    a, b, c, det = a[valid], b[valid], c[valid], det[valid]
    dx, dz = x-a[:, 0], z-a[:, 2]
    u = (dx*c[:, 2] - dz*c[:, 0]) / det
    v = (b[:, 0]*dz - b[:, 2]*dx) / det
    y = a[:, 1] + u*b[:, 1] + v*c[:, 1]
    mask = (u >= -1e-7) & (v >= -1e-7) & (u+v <= 1+1e-7) & (y >= bottom) & (y <= top)
    return sorted(set(float(round(t, 6)) for t in y[mask]))


def inspect(part: str, root_name: str, hinge: tuple, axis: tuple, ray: tuple) -> dict:
    path = f"assets/tanks/tiger/baked/tiger_{part}.glb"
    doc, binary, raw = read_glb(path)
    roots = doc["scenes"][doc.get("scene", 0)]["nodes"]
    matches = [i for i in roots if doc["nodes"][i].get("name") == root_name]
    assert len(matches) == 1, (part, root_name, matches)
    moving_root = matches[0]
    angle = np.radians(95)
    ax = np.array(axis, dtype=float)
    cross = np.array([[0, -ax[2], ax[1]], [ax[2], 0, -ax[0]], [-ax[1], ax[0], 0]])
    rotation = np.eye(3)*np.cos(angle) + (1-np.cos(angle))*np.outer(ax, ax) + np.sin(angle)*cross
    pivot = np.array(hinge)
    counts = [0, 0]
    hits = {"static": [], "closed_lid": [], "open_lid": []}
    moving_nodes = []
    bounds_min, bounds_max = np.full(3, np.inf), np.full(3, -np.inf)
    material_ids = set()

    def walk(index: int, parent: np.ndarray, moving: bool) -> None:
        nonlocal bounds_min, bounds_max
        node = doc["nodes"][index]
        world = parent @ matrix(node)
        if moving:
            moving_nodes.append(index)
        if "mesh" in node:
            for prim in doc["meshes"][node["mesh"]]["primitives"]:
                local = accessor(doc, binary, prim["attributes"]["POSITION"])
                verts = local @ world[:3, :3].T + world[:3, 3]
                indices = accessor(doc, binary, prim["indices"])
                counts[int(moving)] += indices.size // 3
                hits["closed_lid" if moving else "static"].extend(vertical_hits(verts, indices, ray))
                if moving:
                    material_ids.add(prim.get("material"))
                    bounds_min = np.minimum(bounds_min, verts.min(0))
                    bounds_max = np.maximum(bounds_max, verts.max(0))
                    opened = (verts - pivot) @ rotation.T + pivot
                    hits["open_lid"].extend(vertical_hits(opened, indices, ray))
        for child in node.get("children", []):
            walk(child, world, moving)

    for root in roots:
        walk(root, np.eye(4), root == moving_root)
    hits = {key: sorted(set(values)) for key, values in hits.items()}
    assert hits["closed_lid"], f"{part}: selected source lid does not cover the test aperture"
    assert not hits["open_lid"], f"{part}: opened lid still covers aperture: {hits}"
    assert not hits["static"], f"{part}: static remainder still seals aperture: {hits}"
    assert counts[1] > 0 and counts[0] > counts[1]
    return {"source": path, "source_sha256": hashlib.sha256(raw).hexdigest(),
            "root": root_name, "moving_nodes": moving_nodes,
            "triangles_static": counts[0], "triangles_moving": counts[1],
            "source_triangle_sum": sum(counts), "material_indices_preserved": sorted(material_ids),
            "closed_bounds_min_m": bounds_min.tolist(), "closed_bounds_max_m": bounds_max.tolist(),
            "hinge_m": list(hinge), "open_axis": list(axis), "open_degrees": 95,
            "aperture_test_ray_x_z_ymin_ymax": list(ray), "intersections_y_m": hits}


def main() -> None:
    rows = [
        inspect("hull", "Drivers Hatch Pivot", (1.382032, 1.9149045, 1.8893634),
                (0, 0, -1), (0.98, 1.8894, 1.7, 2.3)),
        inspect("turret", "gunner_hatch", (-0.7036, 2.4564, 0.1235),
                (1, 0, 0), (-0.70, -0.10, 2.05, 2.8)),
    ]
    print(json.dumps({"schema": "kessel.tiger-roof-hatch-partition-check", "version": 1,
                      "checks": rows}, indent=2))


if __name__ == "__main__":
    main()
