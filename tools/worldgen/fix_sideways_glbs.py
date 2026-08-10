#!/usr/bin/env python3
"""fix_sideways_glbs.py — stand fallen tree GLBs upright, in place.

The "PSX Nature (Tree branches separated)" pack ships every tree/pine with the trunk authored
along ±Z (lying on its side) plus a stray showcase-scene root translation — so all three acts
planted forests of fallen, offset trees. This tool rewrites a GLB so ANY loader sees it right:

  1. bake every node's world transform into its mesh vertex data (positions/normals/tangents),
  2. rotate the whole model about X so the long axis stands on +Y,
  3. re-origin: ground-contact slice centroid → (0, 0, 0),
  4. strip all node TRS (identity graph), recompute POSITION accessor min/max,
  5. keep the untouched original beside it as <name>_orig.glb (assetlib skips *_orig*).

Only files that LOOK fallen are touched (long axis on Z, > 2.5 m, > 1.5× the Y extent) — logs,
stumps, grass, and stones pass through untouched. Idempotent: an upright file is left alone.

Usage: python3 tools/worldgen/fix_sideways_glbs.py <file.glb|dir> [...]
"""
import json
import math
import os
import shutil
import struct
import sys

F32 = 5126


def mat_mul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(4)) for j in range(4)] for i in range(4)]


def mat_apply(m, v, w):
    return [sum(m[i][k] * (v[k] if k < 3 else w) for k in range(4)) for i in range(3)]


def trs_matrix(node):
    t = node.get("translation", [0, 0, 0])
    q = node.get("rotation", [0, 0, 0, 1])
    s = node.get("scale", [1, 1, 1])
    x, y, z, w = q
    r = [
        [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
        [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
        [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
    ]
    m = [[r[i][j] * s[j] for j in range(3)] + [t[i]] for i in range(3)]
    return m + [[0, 0, 0, 1]]


def load_glb(path):
    b = open(path, "rb").read()
    assert b[:4] == b"glTF", f"not a GLB: {path}"
    jlen = struct.unpack("<I", b[12:16])[0]
    assert b[16:20] == b"JSON"
    g = json.loads(b[20:20 + jlen])
    off = 20 + jlen
    blen = struct.unpack("<I", b[off:off + 4])[0]
    assert b[off + 4:off + 8] == b"BIN\x00"
    bin_ = bytearray(b[off + 8:off + 8 + blen])
    return g, bin_


def save_glb(path, g, bin_):
    j = json.dumps(g, separators=(",", ":")).encode()
    j += b" " * (-len(j) % 4)
    while len(bin_) % 4:
        bin_.append(0)
    out = b"glTF" + struct.pack("<II", 2, 12 + 8 + len(j) + 8 + len(bin_))
    out += struct.pack("<I", len(j)) + b"JSON" + j
    out += struct.pack("<I", len(bin_)) + b"BIN\x00" + bytes(bin_)
    open(path, "wb").write(out)


def accessor_floats(g, bin_, idx):
    """Yield (byte_offset, n_components) for each element of a float accessor."""
    acc = g["accessors"][idx]
    assert acc["componentType"] == F32, f"accessor {idx} not float32"
    ncomp = {"VEC3": 3, "VEC4": 4}[acc["type"]]
    bv = g["bufferViews"][acc["bufferView"]]
    stride = bv.get("byteStride", ncomp * 4)
    base = bv.get("byteOffset", 0) + acc.get("byteOffset", 0)
    for i in range(acc["count"]):
        yield base + i * stride, ncomp
    del bin_  # (signature symmetry; data access happens at the caller)


def node_world_mats(g):
    """mesh-node index → world matrix, walking the default scene."""
    out = {}

    def walk(idx, parent):
        node = g["nodes"][idx]
        m = mat_mul(parent, trs_matrix(node))
        if "mesh" in node:
            assert node["mesh"] not in [g["nodes"][i].get("mesh") for i in out if i != idx], \
                "mesh instanced by two nodes — flattening would fork it"
            out[idx] = m
        for c in node.get("children", []):
            walk(c, m)

    ident = [[float(i == j) for j in range(4)] for i in range(4)]
    scene = g.get("scenes", [{}])[g.get("scene", 0)]
    for r in scene.get("nodes", range(len(g.get("nodes", [])))):
        walk(r, ident)
    return out


def fix_file(path):
    g, bin_ = load_glb(path)
    worlds = node_world_mats(g)

    # World AABB (positions baked) — decide whether this model is fallen at all.
    mn = [1e18] * 3
    mx = [-1e18] * 3
    for ni, m in worlds.items():
        mesh = g["meshes"][g["nodes"][ni]["mesh"]]
        for prim in mesh["primitives"]:
            for off, nc in accessor_floats(g, bin_, prim["attributes"]["POSITION"]):
                v = struct.unpack_from("<3f", bin_, off)
                w = mat_apply(m, list(v), 1.0)
                for i in range(3):
                    mn[i] = min(mn[i], w[i])
                    mx[i] = max(mx[i], w[i])
    ey, ez = mx[1] - mn[1], mx[2] - mn[2]
    fallen = ez > 2.5 and ez > 1.5 * ey
    # Even upright models can carry the pack's stray showcase-scene offset: base centre far from
    # the origin means every placement renders metres away from where it was authored.
    off = math.hypot((mn[0] + mx[0]) * 0.5, (mn[2] + mx[2]) * 0.5)
    if not fallen and off < 1.0 and abs(mn[1]) < 0.5:
        return False  # upright and on-origin — leave it alone

    if fallen:
        # Corrective rotation about X: trunk on -Z → +90° ((x,y,z)→(x,-z,y)); on +Z → -90°.
        to_neg_z = abs(mn[2]) > abs(mx[2])
        rot = ([[1, 0, 0], [0, 0, -1], [0, 1, 0]] if to_neg_z
               else [[1, 0, 0], [0, 0, 1], [0, -1, 0]])
    else:
        rot = [[1, 0, 0], [0, 1, 0], [0, 0, 1]]  # re-origin only

    orig = path[:-4] + "_orig.glb"
    if not os.path.exists(orig):
        shutil.copy2(path, orig)

    # Pass 1: bake world + rotation into every position/normal/tangent, gather the new AABB.
    seen = set()
    nmn = [1e18] * 3
    nmx = [-1e18] * 3
    baked_pos = []  # (accessor idx, [floats]) to re-write with the re-origin shift in pass 2
    for ni, m in worlds.items():
        rot_only = [[m[i][j] for j in range(3)] for i in range(3)]
        mesh = g["meshes"][g["nodes"][ni]["mesh"]]
        for prim in mesh["primitives"]:
            attrs = prim["attributes"]
            for key, full in (("POSITION", True), ("NORMAL", False), ("TANGENT", False)):
                if key not in attrs or attrs[key] in seen:
                    continue
                seen.add(attrs[key])
                for off, nc in accessor_floats(g, bin_, attrs[key]):
                    v = list(struct.unpack_from(f"<{nc}f", bin_, off))
                    if full:
                        w = mat_apply(m, v[:3], 1.0)
                    else:
                        w = [sum(rot_only[i][k] * v[k] for k in range(3)) for i in range(3)]
                    r = [sum(rot[i][k] * w[k] for k in range(3)) for i in range(3)]
                    if full:
                        for i in range(3):
                            nmn[i] = min(nmn[i], r[i])
                            nmx[i] = max(nmx[i], r[i])
                    struct.pack_into(f"<{min(nc,3)}f", bin_, off, *r)
                if key == "POSITION":
                    baked_pos.append(attrs[key])

    # Ground-contact slice centroid → the new origin (trunk base at 0,0,0).
    cx = cz = cn = 0.0
    for ai in baked_pos:
        for off, nc in accessor_floats(g, bin_, ai):
            x, y, z = struct.unpack_from("<3f", bin_, off)
            if y < nmn[1] + 0.4:
                cx += x
                cz += z
                cn += 1
    cx, cz = (cx / cn, cz / cn) if cn else (0.0, 0.0)
    shift = [-cx, -nmn[1], -cz]
    for ai in baked_pos:
        amn = [1e18] * 3
        amx = [-1e18] * 3
        for off, nc in accessor_floats(g, bin_, ai):
            v = list(struct.unpack_from("<3f", bin_, off))
            v = [v[i] + shift[i] for i in range(3)]
            struct.pack_into("<3f", bin_, off, *v)
            for i in range(3):
                amn[i] = min(amn[i], v[i])
                amx[i] = max(amx[i], v[i])
        g["accessors"][ai]["min"] = amn
        g["accessors"][ai]["max"] = amx

    # The node graph is baked into the data now — every node becomes identity.
    for n in g.get("nodes", []):
        for k in ("translation", "rotation", "scale"):
            n.pop(k, None)

    save_glb(path, g, bin_)
    return True


def main():
    targets = []
    for arg in sys.argv[1:]:
        if os.path.isdir(arg):
            targets += [os.path.join(arg, f) for f in sorted(os.listdir(arg))
                        if f.endswith(".glb") and "_orig" not in f]
        else:
            targets.append(arg)
    fixed = 0
    for p in targets:
        try:
            if fix_file(p):
                fixed += 1
                print(f"[fix] UPRIGHTED {p}")
        except Exception as e:
            print(f"[fix] SKIP {p}: {e}")
    print(f"[fix] {fixed} fixed of {len(targets)} scanned")


if __name__ == "__main__":
    main()
