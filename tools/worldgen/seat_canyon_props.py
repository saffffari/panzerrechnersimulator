#!/usr/bin/env python3
"""Seat explicit Canyon wreck/wall families on the actual triangle heightfield.

Writes a separate review revision; never rewrites a running map or its edits. Placement field 6
is optional [pitch_deg, roll_deg], using Ry(yaw) Rx(-pitch) Rz(roll). Source mesh geometry is
unchanged. Wrecks use a rigid least-squares ground plane plus actual lower-surface clearance;
upright walls are buried to their lowest terrain support. This is static authoring, not physics.
"""
import argparse
import copy
import hashlib
import json
import math
from pathlib import Path
import shutil
import struct

import numpy as np

WRECKS = {f"assets/authoring/battlefield/{tank}_wreck_{finish}.glb"
          for tank in ("kv1", "pz3") for finish in ("burnt", "detail")}
WRECKS.update(f"assets/authoring/canyon-v2/{name}.glb" for name in ("wreck_cart", "wreck_gun45"))
WALL_SUFFIX = "/Modular Structures/wall_hr_1.glb"
BURIAL_M = 0.035


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def rotation(yaw, pitch=0., roll=0.):
    y, p, r = np.radians([yaw, pitch, roll])
    cy, sy, cp, sp, cr, sr = math.cos(y), math.sin(y), math.cos(p), math.sin(p), math.cos(r), math.sin(r)
    return (np.array([[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]])
            @ np.array([[1, 0, 0], [0, cp, sp], [0, -sp, cp]])
            @ np.array([[cr, -sr, 0], [sr, cr, 0], [0, 0, 1]]))


class Ground:
    def __init__(self, heights, spacing):
        self.h = np.asarray(heights, dtype=float)
        assert self.h.ndim == 2 and self.h.shape[0] == self.h.shape[1] and np.isfinite(self.h).all()
        assert spacing > 0
        self.spacing = spacing

    def __call__(self, x, z):
        # Match Heightfield::grade_at, including the triangle diagonal (not bilinear filtering).
        n = len(self.h); gx = np.asarray(x) / self.spacing + (n - 1) * .5
        gz = np.asarray(z) / self.spacing + (n - 1) * .5
        ix = np.clip(np.floor(gx), 0, n - 2).astype(int); iz = np.clip(np.floor(gz), 0, n - 2).astype(int)
        tx = np.clip(gx - ix, 0, 1); tz = np.clip(gz - iz, 0, 1)
        a, b, c, d = self.h[iz, ix], self.h[iz, ix + 1], self.h[iz + 1, ix], self.h[iz + 1, ix + 1]
        return np.where(tx + tz <= 1, a + (b-a)*tx + (c-a)*tz,
                        d + (c-d)*(1-tx) + (b-d)*(1-tz))


def glb_points(path):
    """Scene-node transforms and POSITION data only. Materials/assets are never rewritten."""
    raw = Path(path).read_bytes()
    assert raw[:4] == b"glTF"
    n = struct.unpack_from("<I", raw, 12)[0]; doc = json.loads(raw[20:20+n])
    length, kind = struct.unpack_from("<II", raw, 20+n); assert kind == 0x004E4942
    binary = raw[28+n:28+n+length]
    result = []

    def positions(index):
        a = doc["accessors"][index]; v = doc["bufferViews"][a["bufferView"]]
        assert a["componentType"] == 5126 and a["type"] == "VEC3" and "sparse" not in a
        assert v.get("buffer", 0) == 0
        return np.ndarray((a["count"], 3), dtype="<f4", buffer=binary,
                          offset=v.get("byteOffset", 0)+a.get("byteOffset", 0),
                          strides=(v.get("byteStride", 12), 4)).astype(float)

    def walk(index, parent):
        node = doc["nodes"][index]
        if "matrix" in node:
            matrix = np.asarray(node["matrix"]).reshape(4, 4).T
        else:
            x, y, z, w = node.get("rotation", [0, 0, 0, 1])
            q = np.array([[1-2*(y*y+z*z), 2*(x*y-z*w), 2*(x*z+y*w)],
                          [2*(x*y+z*w), 1-2*(x*x+z*z), 2*(y*z-x*w)],
                          [2*(x*z-y*w), 2*(y*z+x*w), 1-2*(x*x+y*y)]])
            matrix = np.eye(4); matrix[:3, :3] = q @ np.diag(node.get("scale", [1, 1, 1]))
            matrix[:3, 3] = node.get("translation", [0, 0, 0])
        model = parent @ matrix
        if "mesh" in node:
            for primitive in doc["meshes"][node["mesh"]]["primitives"]:
                points = positions(primitive["attributes"]["POSITION"])
                result.append(points @ model[:3, :3].T + model[:3, 3])
        for child in node.get("children", []): walk(child, model)
    for index in doc["scenes"][doc.get("scene", 0)]["nodes"]: walk(index, np.eye(4))
    result = np.concatenate(result)
    assert np.isfinite(result).all()
    return np.unique(result, axis=0)


def supports(points):
    # Lowest actual vertex per 20 cm X/Z cell: repeated triangulation and dense rivet meshes
    # must not outweigh a sparsely triangulated track. Keep the lower 20 cm of the source.
    low = points[points[:, 1] <= points[:, 1].min() + .20]
    cells = np.floor(low[:, [0, 2]] / .20).astype(np.int64)
    _, group = np.unique(cells, axis=0, return_inverse=True)
    ordered = np.lexsort((low[:, 1], group))
    ordered = ordered[np.r_[True, np.diff(group[ordered]) != 0]]
    result = low[ordered]
    assert len(result) >= 3, "insufficient source ground contacts"
    return result


def seat(points, placement, ground, upright=False):
    p = copy.deepcopy(placement)
    scaled = points * np.broadcast_to(np.asarray(p[5], dtype=float), (3,))
    if upright:
        lo, hi = scaled.min(0), scaled.max(0)
        # Sample the entire rectangular base every <=20cm, including edges; endpoints alone
        # can miss the terrain triangle changes under a 4m wall segment.
        xs = np.linspace(lo[0], hi[0], max(2, math.ceil((hi[0]-lo[0])/.2)+1))
        zs = np.linspace(lo[2], hi[2], max(2, math.ceil((hi[2]-lo[2])/.2)+1))
        x, z = np.meshgrid(xs, zs)
        contacts = np.column_stack((x.ravel(), np.full(x.size, lo[1]), z.ravel()))
        tilt = [0., 0.]
    else:
        contacts = supports(scaled)
        horizontal = contacts @ rotation(p[4]).T
        heights = ground(p[1]+horizontal[:, 0], p[3]+horizontal[:, 2])
        centered = horizontal[:, [0, 2]] - horizontal[:, [0, 2]].mean(0)
        gradient = np.linalg.lstsq(centered, heights-heights.mean(), rcond=None)[0]
        local_gradient = rotation(p[4]).T @ np.array([gradient[0], 0, gradient[1]])
        pitch = math.atan(local_gradient[2]); roll = math.atan(local_gradient[0]/math.sqrt(1+local_gradient[2]**2))
        tilt = list(np.degrees([pitch, roll]))
    model = rotation(p[4], *tilt)
    posed = contacts @ model.T
    required = ground(p[1]+posed[:, 0], p[3]+posed[:, 2])-posed[:, 1]
    # A wall must meet ground along its entire base. A rigid wreck clears all measured supports.
    height = float(required.min() if upright else required.max()) - BURIAL_M
    p[2] = round(height, 6)
    if len(p) < 7: p.append([0., 0.])
    p[6] = [round(float(v), 6) for v in tilt]
    clearance = posed[:, 1]+p[2]-ground(p[1]+posed[:, 0], p[3]+posed[:, 2])
    old_tilt = placement[6] if len(placement) >= 7 else [0., 0.]
    old = contacts @ rotation(placement[4], *old_tilt).T
    old_clearance = old[:, 1]+placement[2]-ground(placement[1]+old[:, 0], placement[3]+old[:, 2])
    detail = dict(height_delta_m=p[2]-placement[2], tilt_deg=p[6], contacts=len(contacts),
                  old_clearance_range_m=[float(old_clearance.min()), float(old_clearance.max())],
                  clearance_range_m=[float(clearance.min()), float(clearance.max())])
    if upright:
        detail["remaining_wall_height_m"] = float(scaled[:, 1].max()-scaled[:, 1].min()+clearance.min())
    return p, detail


def apply(manifest, heights, root, edited_indices=()):
    result = copy.deepcopy(manifest); ground = Ground(heights, manifest["terrain"]["spacing"])
    cache = {}; changes = []; skipped = []
    for index, p in enumerate(manifest["placements"]):
        asset = manifest["assets"][p[0]]; wall = asset.endswith(WALL_SUFFIX)
        if asset not in WRECKS and not wall: continue
        if index in edited_indices:
            skipped.append(dict(index=index, asset=asset, reason="preserved authored transform/delete overlay")); continue
        if asset not in cache: cache[asset] = glb_points(Path(root)/asset)
        seated, detail = seat(cache[asset], p, ground, upright=wall)
        result["placements"][index] = seated
        changes.append(dict(index=index, asset=asset, kind="wall" if wall else "wreck", **detail))
    return result, dict(schema="panzer.static-grounding.v1", burial_m=BURIAL_M, changes=changes, skipped=skipped,
                        source_assets={a: digest(Path(root)/a) for a in cache},
                        limits="Rigid source-derived support fitting, not suspension. Upright walls bury uphill bases; terrain and meshes unchanged.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--dest", type=Path, required=True)
    args = parser.parse_args(); source = args.manifest.resolve(); dest = args.dest.resolve()
    assert dest != source.parent, "write a separate review revision"
    assert not dest.exists(), "refuse to replace an existing revision"
    original = json.loads(source.read_text()); terrain_ref = Path(original["terrain"]["heights"])
    height_path = source.parent/terrain_ref
    if not height_path.exists(): height_path = args.root/terrain_ref
    edits_path = source.with_name("manifest_edits.json")
    edits = json.loads(edits_path.read_text()) if edits_path.exists() else {}
    # This tool cannot silently ignore geometry-changing overlays.
    assert not edits.get("scene_blocks") and not edits.get("terrain_edits"), "bake terrain overlays first"
    modified = {op["i"] for op in edits.get("ops", []) if op.get("op") in ("set", "delete")}
    result, receipt = apply(original, np.load(height_path), args.root, modified)
    dest.mkdir(parents=True)
    result["terrain"]["heights"] = "heights.npy"
    result.setdefault("authoring", {})["grounding_review"] = "tools/worldgen/seat_canyon_props.py"
    (dest/"manifest.v1.json").write_text(json.dumps(result, separators=(",", ":"), sort_keys=True)+"\n")
    shutil.copy2(height_path, dest/"heights.npy")
    if edits_path.exists(): shutil.copy2(edits_path, dest/"manifest_edits.json")
    receipt.update(source_manifest=str(source), source_manifest_sha256=digest(source),
                   terrain_sha256=digest(height_path), edits_sha256=digest(edits_path) if edits_path.exists() else None,
                   output_manifest_sha256=digest(dest/"manifest.v1.json"))
    (dest/"grounding.receipt.json").write_text(json.dumps(receipt, indent=2)+"\n")
    from collections import Counter
    print(json.dumps(dict(destination=str(dest), counts=Counter(c["kind"] for c in receipt["changes"]),
                          skipped=len(receipt["skipped"]), max_tilt_deg=max(max(map(abs,c["tilt_deg"])) for c in receipt["changes"])), indent=2))


if __name__ == "__main__": main()
