#!/usr/bin/env python3
"""Stage 2 of the Tiger CAD component bake: kit metadata, registration, component envelopes.

Plain Python 3 + numpy (no FreeCAD, no GPU). Run from the repository root AFTER stage 1
(tools/bake_tiger_cad_components.py) has written assets/fea/tiger_cad/solids.json and
meshes_full.bin:

    python3 -I tools/tiger_cad_register.py                 # every stage
    python3 -I tools/tiger_cad_register.py --stage sw      # sw | register | components | union | catalog | all

Optional arguments (defaults in brackets):
    --cad-dir   folder with the STEP files and Tiger-I(SW).zip [owner's reference archive]
    --baked     folder with the game's tiger_*.glb [/windows/shop/panzerrechnersystem/assets/tanks/tiger/baked]
    --out       stage-1/2 output folder [assets/fea/tiger_cad]
    --catalog   compact committed catalog [catalog/tiger_cad_components.v1.json]
    --freecad   FreeCAD console binary for the union stage [FreeCADCmd]

Stages and outputs (all under --out unless noted):
  sw          sw_parts.json - every SolidWorks part/assembly in Tiger-I(SW).zip, read in memory
              from the zip (never extracted): kit part number, kit sub-assembly, the designer's
              part name (English + Traditional Chinese), material and the mass SolidWorks cached in
              the file's custom properties (grams, 2 decimals). The archive is untrusted input: the
              OLE2 compound-file and property-set readers below only slice bytes and never execute
              or write anything from it.
  register    registration.json - kit millimetres -> game hull metres. The game's baked Tiger GLBs
              are tessellations of this same kit CAD, so the fit is measured on hundreds of
              solid<->mesh correspondences (volume ~ s^3, area ~ s^2, volume centroid), plus a
              cross-check from overall extents (hull length, width over tracks, track width, ground
              line, turret ring height). Residuals are reported per axis in millimetres.
  components  components.json, components/<id>.obj and assemblies/<name>.obj (named per-solid
              objects, engine metres), solid_index.json (per-solid labels by mass). Each component
              lists its kit solids, the identification evidence and doubts, an oriented box in engine
              metres (hull or turret frame), the volume of the solids and of the box, and the
              measured envelope error.
  union       runs `TIGER_CAD_STAGE=union FreeCADCmd tools/bake_tiger_cad_components.py` (fused
              volumes, cavity shells, exact B-rep excursion past each box) and merges the result.
  catalog     the compact committed catalog (--catalog).

Every run is deterministic: no clocks or random state reach the outputs (the RANSAC seed is fixed).
"""
import argparse
import json
import math
import os
import struct
import sys
import time
import zipfile

import numpy as np

DEFAULT_CAD_DIR = ("/windows/shop/ts_assets/_archive/_extracted/_Reference_CAD_Tigers/"
                   "panzer-vi-tiger-1-35-1.snapshot.2/panzer-vi-tiger-1-35-1.snapshot.2")
DEFAULT_BAKED = "/windows/shop/panzerrechnersystem/assets/tanks/tiger/baked"
SW_ZIP = "Tiger-I(SW).zip"
ABS_DENSITY_G_PER_MM3 = 1.02e-3  # SolidWorks library ABS (1020 kg/m^3); re-measured in stage sw


def rnd(x, nd=6):
    if isinstance(x, (list, tuple, np.ndarray)):
        return [rnd(v, nd) for v in x]
    v = round(float(x), nd)
    return 0.0 if v == 0 else v


def _compact(obj, ind=0):
    """JSON with objects indented one space per level and scalar arrays (or arrays of scalar
    arrays, e.g. box axes) kept on one line."""
    pad = " " * (ind + 1)
    scalar = lambda v: not isinstance(v, (list, dict))
    if isinstance(obj, dict):
        if not obj:
            return "{}"
        items = [pad + json.dumps(k, ensure_ascii=False) + ": " + _compact(v, ind + 1) for k, v in obj.items()]
        return "{\n" + ",\n".join(items) + "\n" + " " * ind + "}"
    if isinstance(obj, list):
        if all(scalar(v) for v in obj) or all(isinstance(v, list) and all(scalar(w) for w in v) for v in obj):
            return json.dumps(obj, ensure_ascii=False, separators=(", ", ": "))
        return "[\n" + ",\n".join(pad + _compact(v, ind + 1) for v in obj) + "\n" + " " * ind + "]"
    return json.dumps(obj, ensure_ascii=False)


def dump(path, obj):
    tmp = path + ".tmp"
    with open(tmp, "w", newline="\n") as f:
        f.write(_compact(obj))
        f.write("\n")
    os.replace(tmp, path)


# ----------------------------------------------------------------------------------------------
# SolidWorks metadata (OLE2 compound file -> "Config-0-Properties" property set)
# ----------------------------------------------------------------------------------------------
END_OF_CHAIN = 0xFFFFFFFA


class OleFile:
    """Minimal read-only OLE2 compound-file reader over an in-memory byte string."""

    def __init__(self, data):
        self.d = data
        h = data[:512]
        if h[:8] != bytes.fromhex("d0cf11e0a1b11ae1"):
            raise ValueError("not an OLE2 compound file")
        self.ssz = 1 << struct.unpack_from("<H", h, 30)[0]
        self.mssz = 1 << struct.unpack_from("<H", h, 32)[0]
        nfat = struct.unpack_from("<I", h, 44)[0]
        dir0 = struct.unpack_from("<I", h, 48)[0]
        self.cutoff = struct.unpack_from("<I", h, 56)[0]
        minifat0 = struct.unpack_from("<I", h, 60)[0]
        difat0, ndifat = struct.unpack_from("<II", h, 68)
        difat = list(struct.unpack_from("<109I", h, 76))
        s = difat0
        for _ in range(ndifat):
            if s >= END_OF_CHAIN:
                break
            vals = struct.unpack_from("<%dI" % (self.ssz // 4), self._sector(s))
            difat += list(vals[:-1])
            s = vals[-1]
        fat = []
        for fs in [x for x in difat if x < END_OF_CHAIN][:nfat]:
            fat += struct.unpack_from("<%dI" % (self.ssz // 4), self._sector(fs))
        self.fat = fat
        dirs = self._chain(dir0)
        self.entries = []
        for i in range(len(dirs) // 128):
            e = dirs[i * 128:(i + 1) * 128]
            nl = struct.unpack_from("<H", e, 64)[0]
            size = struct.unpack_from("<Q", e, 120)[0]
            if self.ssz == 512:
                size &= 0xFFFFFFFF
            self.entries.append({
                "name": e[:max(0, nl - 2)].decode("utf-16le", "replace"), "type": e[66],
                "left": struct.unpack_from("<i", e, 68)[0], "right": struct.unpack_from("<i", e, 72)[0],
                "child": struct.unpack_from("<i", e, 76)[0],
                "start": struct.unpack_from("<I", e, 116)[0], "size": size})
        root = self.entries[0]
        self.mini = self._chain(root["start"])[:root["size"]] if root["start"] < END_OF_CHAIN else b""
        self.minifat = []
        if minifat0 < END_OF_CHAIN:
            mf = self._chain(minifat0)
            self.minifat = list(struct.unpack_from("<%dI" % (len(mf) // 4), mf))

    def _sector(self, s):
        o = (s + 1) * self.ssz
        return self.d[o:o + self.ssz]

    def _chain(self, s):
        out, n = [], 0
        while s < END_OF_CHAIN and s < len(self.fat) and n < 1_000_000:
            out.append(self._sector(s))
            s = self.fat[s]
            n += 1
        return b"".join(out)

    def read(self, e):
        if e["type"] == 2 and e["size"] < self.cutoff:
            out, s, n = [], e["start"], 0
            while s < END_OF_CHAIN and s < len(self.minifat) and n < 1_000_000:
                o = s * self.mssz
                out.append(self.mini[o:o + self.mssz])
                s = self.minifat[s]
                n += 1
            return b"".join(out)[:e["size"]]
        return self._chain(e["start"])[:e["size"]]

    def streams(self):
        """(path, entry) for every stream, walking the red-black directory tree iteratively."""
        out, stack, seen = [], [(self.entries[0]["child"], "")], set()
        while stack:
            idx, prefix = stack.pop()
            if idx < 0 or idx >= len(self.entries) or idx in seen:
                continue
            seen.add(idx)
            e = self.entries[idx]
            stack.append((e["left"], prefix))
            stack.append((e["right"], prefix))
            path = prefix + e["name"]
            if e["type"] == 1:
                stack.append((e["child"], path + "/"))
            elif e["type"] == 2:
                out.append((path, e))
        return sorted(out, key=lambda q: q[0])


def parse_property_set(b):
    """MS-OLEPS property set -> {name: value}; names from the dictionary property."""
    nsets = struct.unpack_from("<I", b, 24)[0]
    props = {}
    for si in range(min(nsets, 4)):
        off = struct.unpack_from("<I", b, 44 + si * 20)[0]
        nprops = struct.unpack_from("<I", b, off + 4)[0]
        raw = {}
        for k in range(min(nprops, 4096)):
            pid, poff = struct.unpack_from("<II", b, off + 8 + 8 * k)
            o = off + poff
            if pid == 0:
                n = struct.unpack_from("<I", b, o)[0]
                o += 4
                d = {}
                for _ in range(min(n, 4096)):
                    did, ln = struct.unpack_from("<II", b, o)
                    o += 8
                    d[did] = b[o:o + ln]
                    o += ln
                raw[0] = d
                continue
            typ = struct.unpack_from("<I", b, o)[0] & 0xFFFF
            o += 4
            if typ == 2:
                v = struct.unpack_from("<h", b, o)[0]
            elif typ == 3:
                v = struct.unpack_from("<i", b, o)[0]
            elif typ == 0x1E:
                ln = struct.unpack_from("<I", b, o)[0]
                v = b[o + 4:o + 4 + ln]
            elif typ == 0x1F:
                ln = struct.unpack_from("<I", b, o)[0]
                v = b[o + 4:o + 4 + 2 * ln].decode("utf-16le", "replace").rstrip("\x00")
            elif typ == 5:
                v = struct.unpack_from("<d", b, o)[0]
            else:
                v = None
            raw[pid] = v

        def dec(x):
            if isinstance(x, bytes):
                x = x.rstrip(b"\x00")
                for enc in ("utf-8", "cp950", "latin-1"):
                    try:
                        return x.decode(enc)
                    except UnicodeDecodeError:
                        pass
            return x
        names = {k: dec(v) for k, v in raw.get(0, {}).items()}
        for pid, v in raw.items():
            if pid in (0, 1):
                continue
            props[str(names.get(pid, pid))] = dec(v)
    return props


SW_FIELDS = {  # SolidWorks custom-property names used by the kit designer (Traditional Chinese)
    "零件名稱": "part_name",          # part name, "ENGLISH(中文)"
    "零件圖號": "part_drawing_no",    # part drawing number, e.g. RM5010-28U05
    "零件編號": "part_no",            # sprue part number, e.g. U05
    "機構圖號": "assembly_drawing_no",  # assembly (mechanism) drawing number, e.g. RM5010-0B29
    "零件材質": "material",
    "零件重量": "mass_g",             # cached SW-Mass evaluation, grams, 2 decimals
    "規格說明": "spec",               # scale note and the kit pieces merged into this CAD part
    "設備機種": "model",
}


def stage_sw(args):
    zpath = os.path.join(args.cad_dir, SW_ZIP)
    z = zipfile.ZipFile(zpath)
    parts = []
    for info in sorted(z.infolist(), key=lambda i: i.filename):
        name = info.filename
        if info.is_dir() or not name.upper().endswith((".SLDPRT", ".SLDASM")):
            continue
        base = name.rsplit("/", 1)[-1]
        if base.startswith("~$"):
            continue  # SolidWorks lock files, not documents
        if not info.flag_bits & 0x800:  # legacy zip name: cp437 bytes of a Big5 (cp950) name
            try:
                name = name.encode("cp437").decode("cp950")
            except (UnicodeEncodeError, UnicodeDecodeError):
                pass
        rec = {"zip_path": name, "file": name.rsplit("/", 1)[-1], "kind": base[-6:].upper(),
               "zip_bytes": info.file_size}
        try:
            ole = OleFile(z.read(info))
            props = {}
            for path, e in ole.streams():
                if path == "Config-0-Properties":
                    props = parse_property_set(ole.read(e))
            for k, v in SW_FIELDS.items():
                if k in props and props[k] not in ("", "---"):
                    rec[v] = props[k]
            if "mass_g" in rec:
                try:
                    rec["mass_g"] = float(rec["mass_g"])
                except ValueError:
                    rec["mass_g_text"] = rec.pop("mass_g")
        except Exception as ex:  # report, never hide
            rec["error"] = repr(ex)
        stem = rec["file"].rsplit(".", 1)[0].replace("TG_RM5010-", "")
        rec["kit_number"] = stem.split("_", 1)[0]
        rec["label"] = stem.split("_", 1)[1] if "_" in stem else stem
        folders = [p for p in rec["zip_path"].split("/")[1:-1]]
        rec["folder"] = "/".join(folders)
        parts.append(rec)
    dump(os.path.join(args.out, "sw_parts.json"), {
        "schema": "panzer.tiger-cad-sw-parts.v1",
        "source": SW_ZIP,
        "note": ("Read in memory from the zip, never extracted. part_name is the designer's "
                 "'ENGLISH(中文)' label; mass_g is the SolidWorks-cached mass (2 decimals) with the "
                 "file's material (ABS throughout this kit)."),
        "parts": parts})
    print("[sw] %d SolidWorks documents -> sw_parts.json" % len(parts))
    return parts


# ----------------------------------------------------------------------------------------------
# Game GLB reading (glTF 2.0 binary, JSON + BIN chunk; numpy only)
# ----------------------------------------------------------------------------------------------
def _quat_mat(q):
    x, y, z, w = q
    return np.array([[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
                     [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
                     [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]])


def _node_local(n):
    if "matrix" in n:
        return np.array(n["matrix"], float).reshape(4, 4).T
    M = np.eye(4)
    M[:3, :3] = _quat_mat(n.get("rotation", [0, 0, 0, 1])) @ np.diag(n.get("scale", [1, 1, 1]))
    M[:3, 3] = n.get("translation", [0, 0, 0])
    return M


def read_glb(path):
    with open(path, "rb") as f:
        data = f.read()
    clen = struct.unpack_from("<I", data, 12)[0]
    js = json.loads(data[20:20 + clen])
    off = 20 + clen
    blen = struct.unpack_from("<I", data, off)[0]
    return js, data[off + 8:off + 8 + blen]


def _accessor(js, bin_, ai):
    a = js["accessors"][ai]
    bv = js["bufferViews"][a["bufferView"]]
    comp = {5126: np.float32, 5125: np.uint32, 5123: np.uint16, 5121: np.uint8}[a["componentType"]]
    ncomp = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[a["type"]]
    start = bv.get("byteOffset", 0) + a.get("byteOffset", 0)
    item = np.dtype(comp).itemsize * ncomp
    stride = bv.get("byteStride", 0) or item
    raw = np.frombuffer(bin_, np.uint8, count=stride * (a["count"] - 1) + item, offset=start)
    rows = np.lib.stride_tricks.as_strided(raw, (a["count"], item), (stride, 1)).copy()
    return rows.view(comp).reshape(a["count"], ncomp)


def glb_node_meshes(path):
    """Every mesh node: world-space vertices, triangles, name (world = the GLB's own frame)."""
    js, bin_ = read_glb(path)
    nodes = js["nodes"]
    parent = {c: i for i, n in enumerate(nodes) for c in n.get("children", [])}
    cache = {}

    def world(i):
        if i not in cache:
            M = _node_local(nodes[i])
            cache[i] = world(parent[i]) @ M if i in parent else M
        return cache[i]
    out = []
    for i, n in enumerate(nodes):
        if "mesh" not in n:
            continue
        M = world(i)
        Ps, Ts, base = [], [], 0
        for prim in js["meshes"][n["mesh"]]["primitives"]:
            P = _accessor(js, bin_, prim["attributes"]["POSITION"]).astype(float) @ M[:3, :3].T + M[:3, 3]
            T = (_accessor(js, bin_, prim["indices"]).astype(np.int64).reshape(-1, 3)
                 if "indices" in prim else np.arange(len(P)).reshape(-1, 3))
            Ps.append(P)
            Ts.append(T + base)
            base += len(P)
        P, T = np.concatenate(Ps), np.concatenate(Ts)
        V, A, C = mesh_mass_props(P, T)
        out.append({"node": i, "name": n.get("name", "node%d" % i), "P": P, "T": T,
                    "V": V, "A": A, "C": C, "lo": P.min(0), "hi": P.max(0)})
    return out


def mesh_mass_props(P, T):
    """Signed volume, area and volume centroid of a triangle mesh (divergence theorem)."""
    a, b, c = P[T[:, 0]], P[T[:, 1]], P[T[:, 2]]
    area = 0.5 * float(np.linalg.norm(np.cross(b - a, c - a), axis=1).sum())
    v6 = np.einsum("ij,ij->i", a, np.cross(b, c))
    V = float(v6.sum() / 6.0)
    C = ((a + b + c) * v6[:, None]).sum(0) / 24.0 / V if abs(V) > 1e-18 else P.mean(0)
    return V, area, C


# ----------------------------------------------------------------------------------------------
# Kit solids and meshes (stage-1 outputs)
# ----------------------------------------------------------------------------------------------
def load_solids(out_dir):
    d = json.load(open(os.path.join(out_dir, "solids.json")))
    return d, d["files"]["full"]["solids"]


def load_kit_meshes(out_dir, solids, ids=None):
    """{id: (P, T)} from meshes_full.bin (kit mm, STEP frame)."""
    want = set(ids) if ids is not None else None
    out = {}
    with open(os.path.join(out_dir, "meshes_full.bin"), "rb") as f:
        head = f.read(16)
        if head[:8] != b"TGRCADM1":
            raise ValueError("meshes_full.bin: bad magic")
        for s in solids:
            if want is not None and s["id"] not in want:
                continue
            f.seek(s["mesh"]["offset"])
            sid, nv, nt = struct.unpack("<III", f.read(12))
            assert sid == s["id"]
            P = np.frombuffer(f.read(12 * nv), "<f4").reshape(nv, 3).astype(float)
            T = np.frombuffer(f.read(12 * nt), "<u4").reshape(nt, 3).astype(np.int64)
            out[sid] = (P, T)
    return out


# ----------------------------------------------------------------------------------------------
# Registration kit mm -> game metres
# ----------------------------------------------------------------------------------------------
def umeyama(A, B, with_scale=True):
    """Similarity s, R (proper), t minimising |s R a + t - b|^2."""
    ca, cb = A.mean(0), B.mean(0)
    A0, B0 = A - ca, B - cb
    H = B0.T @ A0 / len(A)
    U, S, Vt = np.linalg.svd(H)
    D = np.eye(3)
    if np.linalg.det(U) * np.linalg.det(Vt) < 0:
        D[2, 2] = -1
    R = U @ D @ Vt
    s = float(np.trace(np.diag(S) @ D) / ((A0 ** 2).sum() / len(A))) if with_scale else 1.0
    return s, R, cb - s * R @ ca


def nearest_signed_permutation(R):
    P = np.zeros((3, 3))
    used = set()
    for i in np.argsort(-np.abs(R).max(1)):
        j = max((j for j in range(3) if j not in used), key=lambda j: abs(R[i, j]))
        used.add(j)
        P[i, j] = np.sign(R[i, j])
    ang = math.degrees(math.acos(max(-1.0, min(1.0, (np.trace(P.T @ R) - 1) / 2))))
    return P, ang


def correspond(kit, nodes, s, tol_v=0.01, tol_a=0.01):
    """Invariant-compatible (kit index, node index) pairs for scale s (kit mm -> game m)."""
    KV = np.array([k["volume_mm3"] for k in kit]) * s ** 3
    KA = np.array([k["area_mm2"] for k in kit]) * s ** 2
    NV = np.array([abs(n["V"]) for n in nodes])
    NA = np.array([n["A"] for n in nodes])
    order = np.argsort(KV)
    sv = KV[order]
    pairs = []
    for j in range(len(nodes)):
        if NV[j] <= 0:
            continue
        lo = np.searchsorted(sv, NV[j] / (1 + tol_v))
        hi = np.searchsorted(sv, NV[j] / (1 - tol_v))
        for i in order[lo:hi]:
            if abs(KA[i] - NA[j]) <= tol_a * NA[j]:
                pairs.append((int(i), j))
    return pairs


def fit_frame(kit, nodes, label, min_volume_mm3=20.0, inlier_m=0.004):
    """Measure s, R, t mapping kit centres of mass onto GLB node volume centroids."""
    big_k = [i for i, k in enumerate(kit) if k["volume_mm3"] >= min_volume_mm3]
    big_n = sorted(range(len(nodes)), key=lambda j: -abs(nodes[j]["V"]))[:200]
    # 1) scale from invariant ratios: s_V = (V_n/V_k)^(1/3) must agree with s_A = (A_n/A_k)^(1/2)
    sv = []
    for j in big_n:
        n = nodes[j]
        if n["V"] <= 0:
            continue
        for i in big_k:
            k = kit[i]
            s_v = (n["V"] / k["volume_mm3"]) ** (1 / 3)
            s_a = (n["A"] / k["area_mm2"]) ** 0.5
            if abs(s_v / s_a - 1) < 0.002:
                sv.append(s_v)
    sv = np.sort(np.array(sv))
    # densest window of +-0.2 % in log scale
    ls = np.log(sv)
    best = max(range(len(ls)), key=lambda q: np.searchsorted(ls, ls[q] + 0.004) - q)
    s0 = float(np.median(sv[best:np.searchsorted(ls, ls[best] + 0.004)]))
    # 2) seeds: pairs whose invariants are unique on both sides, then RANSAC (fixed seed)
    allpairs = [p for p in correspond(kit, nodes, s0) if kit[p[0]]["volume_mm3"] >= min_volume_mm3]
    nk, nn = {}, {}
    for i, j in allpairs:
        nk[i] = nk.get(i, 0) + 1
        nn[j] = nn.get(j, 0) + 1
    seeds = [(i, j) for i, j in allpairs if nk[i] == 1 and nn[j] == 1]
    SK = np.array([kit[i]["com_mm"] for i, _ in seeds])
    SN = np.array([nodes[j]["C"] for _, j in seeds])
    rng = np.random.default_rng(20261102)
    best_inl = None
    for _ in range(3000):
        q = rng.choice(len(seeds), 3, replace=False)
        X = SK[q] - SK[q].mean(0)
        if np.linalg.svd(X, compute_uv=False)[1] < 1.0:  # near-collinear sample
            continue
        s, R, t = umeyama(SK[q], SN[q])
        if not 0.98 * s0 < s < 1.02 * s0:
            continue
        inl = np.linalg.norm(s * SK @ R.T + t - SN, axis=1) < inlier_m
        if best_inl is None or inl.sum() > best_inl.sum():
            best_inl = inl
    s, R, t = umeyama(SK[best_inl], SN[best_inl])
    # 3) every invariant-compatible pair, one-to-one by residual, iterative refit
    pairs = allpairs
    KC = np.array([kit[i]["com_mm"] for i, _ in pairs])
    NC = np.array([nodes[j]["C"] for _, j in pairs])
    for _ in range(5):
        res = np.linalg.norm(s * KC @ R.T + t - NC, axis=1)
        taken_k, taken_n, keep = set(), set(), []
        for q in np.argsort(res, kind="stable"):
            i, j = pairs[q]
            if res[q] < inlier_m and i not in taken_k and j not in taken_n:
                taken_k.add(i)
                taken_n.add(j)
                keep.append(int(q))
        keep = np.array(sorted(keep), dtype=np.int64)
        s, R, t = umeyama(KC[keep], NC[keep])
    pred = s * KC[keep] @ R.T + t
    d = pred - NC[keep]
    P, ang = nearest_signed_permutation(R)
    res = {
        "label": label,
        "scale_m_per_kit_mm": rnd(s, 9),
        "scale_from_invariants_m_per_kit_mm": rnd(s0, 9),
        "R_kit_to_engine": rnd(R.tolist(), 9),
        "nearest_signed_permutation": P.astype(int).tolist(),
        "rotation_off_permutation_deg": rnd(ang, 6),
        "t_m": rnd(t, 6),
        "pairs_used": int(len(keep)),
        "unique_seed_pairs": int(len(seeds)), "seed_inliers": int(best_inl.sum()),
        "pair_selection": ("kit solid >= %.0f mm^3; volume and area agree within 1 %% after scaling; "
                           "volume centroid within %.1f mm after the fit; one-to-one" % (min_volume_mm3, inlier_m * 1000)),
        "residual_mm": {
            "per_axis_rms": rnd(np.sqrt((d ** 2).mean(0)) * 1000, 4),
            "per_axis_max_abs": rnd(np.abs(d).max(0) * 1000, 4),
            "per_axis_mean": rnd(d.mean(0) * 1000, 4),
            "norm_rms": rnd(float(np.sqrt((np.linalg.norm(d, axis=1) ** 2).mean())) * 1000, 4),
            "norm_max": rnd(float(np.linalg.norm(d, axis=1).max()) * 1000, 4),
        },
        "pairs": [{"kit_id": kit[pairs[q][0]]["id"], "node": nodes[pairs[q][1]]["name"],
                   "rel_dV": rnd(abs(nodes[pairs[q][1]]["V"]) / (kit[pairs[q][0]]["volume_mm3"] * s ** 3) - 1, 6),
                   "residual_mm": rnd((s * np.array(kit[pairs[q][0]]["com_mm"]) @ R.T + t - nodes[pairs[q][1]]["C"]) * 1000, 4)}
                  for q in keep],
    }
    return res, (s, R, np.array(t))


GAME_TURRET_PIVOT = np.array([0.0, 0.524, 0.019])    # src/tank.rs TIGER_PARTS.turret_pivot (hull m)
GAME_GUN_TRUNNION = np.array([0.004, 1.845, 1.040])  # src/tank.rs TIGER_PARTS.gun_trunnion (turret m)
NOMINAL_KIT_SCALE = 0.035                            # 1:35, metres of real vehicle per kit millimetre


def fit_scale_translation(K, E):
    """Least squares s, t for E ~ s K + t with the rotation fixed to the identity."""
    kc, ec = K.mean(0), E.mean(0)
    s = float(((K - kc) * (E - ec)).sum() / ((K - kc) ** 2).sum())
    return s, ec - s * kc


def residual_block(d_m):
    d = np.asarray(d_m) * 1000.0
    n = np.linalg.norm(d, axis=1)
    return {"per_axis_rms_mm": rnd(np.sqrt((d ** 2).mean(0)), 4),
            "per_axis_max_abs_mm": rnd(np.abs(d).max(0), 4),
            "per_axis_mean_mm": rnd(d.mean(0), 4),
            "norm_rms_mm": rnd(float(np.sqrt((n ** 2).mean())), 4),
            "norm_p95_mm": rnd(float(np.percentile(n, 95)), 4),
            "norm_max_mm": rnd(float(n.max()), 4)}


def span(P):
    return P.min(0), P.max(0)


def stage_register(args):
    t0 = time.time()
    d, kit = load_solids(args.out)
    nodes = {lab: glb_node_meshes(os.path.join(args.baked, "tiger_%s.glb" % lab))
             for lab in ("hull", "turret", "gun")}
    fits, raw = {}, {}
    for lab in ("hull", "turret", "gun"):
        res, fit = fit_frame(kit, nodes[lab], lab)
        res["glb"] = "tiger_%s.glb" % lab
        res["glb_frame"] = {"hull": "hull metres", "turret": "turret-local metres (yaw 0)",
                            "gun": "gun-local metres (elevation 0, origin at the trunnion)"}[lab]
        fits[lab] = res
        raw[lab] = fit
    # Rotation test: refit the hull pairs with R fixed to the identity.
    by_id = {k["id"]: k for k in kit}
    by_name = {lab: {n["name"]: n for n in nodes[lab]} for lab in nodes}
    K = np.array([by_id[p["kit_id"]]["com_mm"] for p in fits["hull"]["pairs"]])
    E = np.array([by_name["hull"][p["node"]]["C"] for p in fits["hull"]["pairs"]])
    s_free, R_free, t_free = raw["hull"]
    s_i, t_i = fit_scale_translation(K, E)
    res_free = residual_block(s_free * K @ R_free.T + t_free - E)
    res_i = residual_block(s_i * K + t_i - E)
    # Turret and gun GLB frames implied by the hull transform and the game's rig constants.
    checks = {}
    for lab, offset in (("turret", GAME_TURRET_PIVOT), ("gun", GAME_TURRET_PIVOT + GAME_GUN_TRUNNION)):
        Kl = np.array([by_id[p["kit_id"]]["com_mm"] for p in fits[lab]["pairs"]])
        El = np.array([by_name[lab][p["node"]]["C"] for p in fits[lab]["pairs"]])
        pred = s_i * Kl + t_i - offset
        checks[lab] = {
            "pairs": len(Kl),
            "offset_used_m": rnd(offset, 6),
            "residual_using_hull_transform_and_game_rig": residual_block(pred - El),
            "glb_origin_implied_by_free_fit_hull_m": rnd(t_i - np.array(fits[lab]["t_m"]), 6),
        }
    # Overall extents cross-check (an independent, coarse estimate of the same transform).
    kit_all = load_kit_meshes(args.out, kit)
    good = [k for k in kit if k["valid"] and k["volume_mm3"] > 0]
    bad = [k["id"] for k in kit if not (k["valid"] and k["volume_mm3"] > 0)]
    KP = np.concatenate([kit_all[k["id"]][0] for k in good])
    glb_all = np.concatenate([np.concatenate([n["P"] for n in nodes["hull"]]),
                              np.concatenate([n["P"] for n in nodes["turret"]]) + GAME_TURRET_PIVOT,
                              np.concatenate([n["P"] for n in nodes["gun"]]) + GAME_TURRET_PIVOT + GAME_GUN_TRUNNION])
    klo, khi = span(KP)
    glo, ghi = span(glb_all)

    def node_of(lab, kid):
        name = next((p["node"] for p in fits[lab]["pairs"] if p["kit_id"] == kid), None)
        return by_name[lab].get(name) if name else None

    def kit_by(vol, com, tol=0.05):
        c = [k for k in kit if abs(k["volume_mm3"] - vol) < 0.01 and np.linalg.norm(np.array(k["com_mm"]) - com) < tol]
        if len(c) != 1:
            raise SystemExit("register: expected one kit solid V=%.3f at %s, found %d" % (vol, com, len(c)))
        return c[0]
    feats = {}
    feats["overall_length_with_gun_z"] = (khi[2] - klo[2], ghi[2] - glo[2])
    feats["overall_width_x"] = (khi[0] - klo[0], ghi[0] - glo[0])
    roof = kit_by(3896.14, [0.1, 59.1, -63.3], 0.2)          # 12Z01_COVER turret roof (頂蓋)
    roof_node = node_of("turret", roof["id"])
    feats["ground_to_turret_roof_top_y"] = (kit_all[roof["id"]][0][:, 1].max() - klo[1],
                                            roof_node["hi"][1] + GAME_TURRET_PIVOT[1] - glo[1])
    links = [k for k in kit if abs(k["volume_mm3"] - 110.83) < 0.02 and k["com_mm"][1] < 15]
    link_nodes = [n for n in nodes["hull"] if abs(abs(n["V"]) / s_i ** 3 - 110.83) < 1.5 and n["C"][1] < 15 * s_i + t_i[1]]
    LP = {sd: np.concatenate([kit_all[k["id"]][0] for k in links if np.sign(k["com_mm"][0]) == sd]) for sd in (1, -1)}
    GLP = {sd: np.concatenate([n["P"] for n in link_nodes if np.sign(n["C"][0]) == sd]) for sd in (1, -1)}
    mid = lambda P: 0.5 * (P[:, 0].min() + P[:, 0].max())
    for sd, nm in ((1, "left"), (-1, "right")):
        feats["track_width_%s_x" % nm] = (np.ptp(LP[sd][:, 0]), np.ptp(GLP[sd][:, 0]))
    feats["track_centre_spacing_x"] = (mid(LP[1]) - mid(LP[-1]), mid(GLP[1]) - mid(GLP[-1]))
    ring = kit_by(1748.411, [-0.05, 37.93, -67.85])          # 10A12_GEAR turret ring gear (齒輪圈)
    ring_P = kit_all[ring["id"]][0]
    ring_node = node_of("turret", ring["id"])
    c0 = 0.5 * (ring_P.min(0) + ring_P.max(0))
    rr = np.hypot(ring_P[:, 0] - c0[0], ring_P[:, 2] - c0[2])
    rim = ring_P[rr < rr.min() + 0.3]  # inner rim of the ring gear (the bore the turret turns in)
    A = np.c_[2 * rim[:, 0], 2 * rim[:, 2], np.ones(len(rim))]
    cx, cz, _c = np.linalg.lstsq(A, rim[:, 0] ** 2 + rim[:, 2] ** 2, rcond=None)[0]
    feature_rows = []
    for name, (kv, gv) in feats.items():
        feature_rows.append({"feature": name, "kit_mm": rnd(kv, 4), "glb_m": rnd(gv, 5),
                             "implied_scale_m_per_kit_mm": rnd(gv / kv, 7),
                             "residual_at_fitted_scale_mm": rnd((gv - s_i * kv) * 1000, 3)})
    positions = {
        "ground_line_y": (klo[1], glo[1]),
        "front_extreme_z": (khi[2], ghi[2]),
        "rear_extreme_z": (klo[2], glo[2]),
        "left_extreme_x": (khi[0], ghi[0]),
        "right_extreme_x": (klo[0], glo[0]),
        "turret_ring_gear_top_y": (ring_P[:, 1].max(), ring_node["hi"][1] + GAME_TURRET_PIVOT[1]),
        "turret_ring_gear_bottom_y": (ring_P[:, 1].min(), ring_node["lo"][1] + GAME_TURRET_PIVOT[1]),
        "turret_ring_axis_x": (cx, GAME_TURRET_PIVOT[0]),
        "turret_ring_axis_z": (cz, GAME_TURRET_PIVOT[2]),
    }
    axis_of = {"y": 1, "z": 2, "x": 0}
    pos_rows = []
    for name, (kv, gv) in positions.items():
        ax = axis_of[name[-1]]
        pos_rows.append({"feature": name, "kit_mm": rnd(kv, 4), "engine_m": rnd(gv, 5),
                         "predicted_engine_m": rnd(s_i * kv + t_i[ax], 5),
                         "residual_mm": rnd((s_i * kv + t_i[ax] - gv) * 1000, 3)})
    pos_rows[-1]["note"] = pos_rows[-2]["note"] = (
        "kit ring axis from a circle fit to the 10A12 ring-gear tessellation; engine value is the game's "
        "turret pivot constant (src/tank.rs), so this residual is the game pivot's offset from the CAD ring axis")
    # extents-only fit: scale from overall length / width / track width, translation from ground line,
    # lateral centre and front extreme
    s_ext = float(np.mean([feats[k][1] / feats[k][0] for k in
                           ("overall_length_with_gun_z", "overall_width_x", "track_width_left_x", "track_width_right_x")]))
    t_ext = np.array([0.5 * (glo[0] + ghi[0]) - s_ext * 0.5 * (klo[0] + khi[0]),
                      glo[1] - s_ext * klo[1],
                      ghi[2] - s_ext * khi[2]])
    reg = {
        "schema": "panzer.tiger-cad-registration.v1",
        "summary": ("engine_hull_m = s * kit_mm + t, rotation = identity (kit +X = vehicle left = engine +X, "
                    "kit +Y = up, kit +Z = forward). Turret-local = engine_hull - game turret pivot."),
        "transform_kit_mm_to_engine_hull_m": {
            "s_m_per_kit_mm": rnd(s_i, 9), "R": [[1, 0, 0], [0, 1, 0], [0, 0, 1]], "t_m": rnd(t_i, 6),
            "fit": "least squares on %d hull solid<->GLB-node volume-centroid pairs, rotation fixed to identity" % len(K),
            "residual": res_i,
        },
        "free_rotation_fit": {"s_m_per_kit_mm": rnd(s_free, 9), "R": rnd(R_free.tolist(), 9), "t_m": rnd(t_free, 6),
                              "rotation_off_identity_deg": fits["hull"]["rotation_off_permutation_deg"],
                              "residual": res_free},
        "scale_vs_nominal_1_35": {
            "nominal_m_per_kit_mm": NOMINAL_KIT_SCALE,
            "game_over_nominal": rnd(s_i / NOMINAL_KIT_SCALE, 6),
            "note": ("The game's Tiger GLBs are this kit CAD scaled by s; at the kit's stated 1:35 the "
                     "vehicle would be smaller by this factor. Proportions agree (one uniform scale, "
                     "identity rotation); the absolute size does not."),
        },
        "game_rig_constants_used": {"turret_pivot_hull_m": GAME_TURRET_PIVOT.tolist(),
                                    "gun_trunnion_turret_m": GAME_GUN_TRUNNION.tolist(),
                                    "source": "src/tank.rs TIGER_PARTS"},
        "turret_and_gun_glb_checks": checks,
        "per_glb_free_fits": {lab: {k: v for k, v in fits[lab].items() if k != "pairs"} for lab in fits},
        "extents_cross_check": {
            "method": ("kit tessellation extents (all valid solids) vs the game GLBs assembled with the game's "
                       "rig constants (turret yaw 0, gun elevation 0); spans give an independent scale, "
                       "positions are compared at the correspondence fit. Height uses the turret roof plate "
                       "because the kit's open hatches stand above the game's closed ones."),
            "excluded_invalid_solids": bad,
            "spans": feature_rows,
            "positions": pos_rows,
            "extents_only_fit": {"s_m_per_kit_mm": rnd(s_ext, 7), "t_m": rnd(t_ext, 5),
                                 "differs_from_correspondence_fit": {"ds_rel": rnd(s_ext / s_i - 1, 6),
                                                                     "dt_mm": rnd((t_ext - t_i) * 1000, 3)}},
            "track_link_nodes_in_glb": len(link_nodes), "track_links_in_kit": len(links),
        },
        "pairs": {lab: fits[lab]["pairs"] for lab in fits},
    }
    dump(os.path.join(args.out, "registration.json"), reg)
    print("[register] s=%.7f m/kit-mm (x%.4f of 1:35), t=%s m, residual rms %.3f mm (max %.3f) over %d pairs, %.1f s"
          % (s_i, s_i / NOMINAL_KIT_SCALE, np.round(t_i, 5).tolist(), res_i["norm_rms_mm"], res_i["norm_max_mm"],
             len(K), time.time() - t0))
    return reg


# ----------------------------------------------------------------------------------------------
# Components: identification table, envelopes, meshes, catalog
# ----------------------------------------------------------------------------------------------
ROUND_V = 122.79   # 23M03 complete 8.8 cm round, kit mm^3 (70 identical solids)
SHELF_V = 62.699   # 25M06 shell-rack shelf, kit mm^3

# Member selectors in kit millimetres: (kit part, volume mm^3, centre of mass mm) picks exactly one
# solid (|dV| < 0.01 mm^3, |dC| < 0.05 mm); (kit part, volume, box_lo, box_hi, count) picks every
# solid of that volume (|dV| < 0.02) whose centre lies in the box and asserts the count.
C = lambda part, v, c: {"part": part, "vol": v, "com": c}
G = lambda part, v, lo, hi, n: {"part": part, "vol": v, "lo": lo, "hi": hi, "count": n}

COMPONENTS = [
    {"id": "engine_block", "name": "Engine block (kit 'MAIN 引擎主體', engine main body)", "frame": "hull",
     "confidence": "kit_label",
     "members": [C("28U05", 10406.024, [0.675, 14.300, -118.276])],
     "evidence": ("SolidWorks part TG_RM5010-28U05 'MAIN(引擎主體)' (engine main body) in assembly 0B29 "
                  "EngineSystem, merging kit pieces U6/U7/U16/U21/U22/U29/U32/H35/H55/H61/H63/H64; its cached "
                  "mass 10.61 g equals this solid's volume x ABS 1.02 g/cm^3 (10.614 g); identical invariants to "
                  "the largest solid of the separate EngineSystem STEP; rear compartment centreline between the "
                  "two side compartments."),
     "doubts": ("One merged kit body: the engine-mounted kit pieces listed are fused into it, so sub-parts "
                "cannot be separated. Its B-rep carries five small internal void shells (kit_cavities).")},
    {"id": "engine_air_filters", "name": "Engine air filters, pair (kit 28U20 'FILTER TANK')", "frame": "hull",
     "confidence": "kit_label",
     "members": [C("28U20", 639.637, [0.0, 30.807, -123.450]), C("28U20", 639.637, [0.0, 30.807, -110.450])],
     "evidence": ("File label 'FILTER TANK' (28U20, EngineSystem; 0.65 g = 0.652 g); the only EngineSystem part "
                  "instanced twice (14 STEP solids from 13 part files); two identical axisymmetric drums on top "
                  "of the engine block."),
     "doubts": "The SolidWorks part carries no Chinese description, only the file label."},
    {"id": "radiator_left", "name": "Radiator, left side compartment (kit 27K76 'COOLBOX 冷卻箱')", "frame": "hull",
     "confidence": "kit_label",
     "members": [C("27K76", 2793.309, [29.512, 27.501, -124.609])],
     "evidence": ("Part 27K76 'COOLBOX(冷卻箱)' (cooling box) in 0B23 EngineRoom, 2.85 g = 2.849 g (unique); flat "
                  "block 9.3 kit mm deep across the airflow, between the fuel-tank shells (front) and the twin-fan "
                  "shroud (rear) of the left rear side compartment, filler neck on top."),
     "doubts": "The kit models the core and header as one body; no tubes or coolant are modelled."},
    {"id": "radiator_right", "name": "Radiator, right side compartment (kit 27K77 'COOLBOX 冷卻箱')", "frame": "hull",
     "confidence": "kit_label",
     "members": [C("27K77", 2780.928, [-29.512, 27.461, -124.611])],
     "evidence": "Mirror of radiator_left: part 27K77 'COOLBOX(冷卻箱)', 2.84 g = 2.837 g (unique).",
     "doubts": "As radiator_left."},
    {"id": "cooling_fans_left", "name": "Cooling fan pair and shroud, left (kit 27H13 'COVER 風扇罩' + rotors)",
     "frame": "hull", "confidence": "kit_label+geometry",
     "members": [C("27H13", 589.556, [29.500, 26.482, -132.086]), C("27H25", 205.006, [29.500, 31.295, -136.793])],
     "evidence": ("Part 27H13 'COVER(風扇罩)' (fan cover/shroud, 0B23 EngineRoom, 0.60 g = 0.601 g, one per side) "
                  "with two circular ducts; the companion solid is the pair of eight-bladed fan rotors with hubs "
                  "seated in those ducts (geometry), 0.21 g, consistent with the unlabelled EngineRoom part "
                  "27H25 (the other 0.21 g candidates are turret parts). The game's fan derivative "
                  "(catalog/tiger_workshop_cooling.v1.json) was cut from this rotor solid's mesh."),
     "doubts": ("The rotor solid's part number is inferred from mass and folder (27H25_PLATE carries no "
                "description).")},
    {"id": "cooling_fans_right", "name": "Cooling fan pair and shroud, right (kit 27H13 'COVER 風扇罩' + rotors)",
     "frame": "hull", "confidence": "kit_label+geometry",
     "members": [C("27H13", 589.556, [-29.500, 26.482, -132.086]), C("27H25", 205.006, [-29.500, 31.295, -136.793])],
     "evidence": "Mirror of cooling_fans_left.",
     "doubts": "As cooling_fans_left."},
    {"id": "fuel_tank_left", "name": "Fuel tank shells, left side compartment (kit 27H12 'BOX' + 28H21/22 'COVER 油箱外蓋')",
     "frame": "hull", "confidence": "kit_label+geometry",
     "members": [C("27H12", 1152.603, [29.500, 25.640, -105.397]), C("28H21|28H22", 845.780, [29.493, 28.029, -111.183])],
     "evidence": ("Part pair 28H21/28H22 'COVER(油箱外蓋)' (fuel tank outer cover, 0B23 EngineRoom, 0.86 g, "
                  "mirror pair) and the unlabelled box 27H12 (1.18 g; the only other 1.18 g part, 21K57, is the "
                  "front unit above the gearbox) carrying a filler neck on top; the two thin-walled wedge shells "
                  "nest between the engine-room front bulkhead and the radiator. The game's fuel-service "
                  "derivative (catalog/tiger_fuel_service.v1.json, removed root Hull_0902) was made from the "
                  "27H12 solid of this side."),
     "doubts": ("The kit labels only the outer cover as fuel-tank; whether the space is one tank or two is not "
                "shown. Both parts are hollow kit mouldings with no modelled contents (see kit_shell).")},
    {"id": "fuel_tank_right", "name": "Fuel tank shells, right side compartment (kit 27H12 'BOX' + 28H21/22 'COVER 油箱外蓋')",
     "frame": "hull", "confidence": "kit_label+geometry",
     "members": [C("27H12", 1152.603, [-29.500, 25.640, -105.397]), C("28H21|28H22", 845.806, [-29.493, 28.029, -111.183])],
     "evidence": "Mirror of fuel_tank_left (game removed root Hull_0901 is this side's 27H12 solid).",
     "doubts": "As fuel_tank_left."},
    {"id": "gearbox", "name": "Gearbox casing (kit 21K49 'BOX 轉向機')", "frame": "hull",
     "confidence": "kit_label+geometry",
     "members": [C("21K49", 4901.720, [-1.181, 12.500, -23.131])],
     "evidence": ("Part 21K49 'BOX(轉向機)' (steering gear) in 0B21 DriverRoom, 5.00 g = 5.000 g (unique): the "
                  "longitudinal casing on the centreline between the driver's and radio operator's stations, "
                  "directly behind the transverse steering unit and in line with the propeller shaft."),
     "doubts": ("The kit names all three transmission housings 轉向機 (steering gear: main / middle / rear); "
                "naming this one the gearbox rests on its longitudinal shape and position (gearbox behind the "
                "transversely mounted steering unit), not on the kit label.")},
    {"id": "steering_unit", "name": "Steering unit with end covers (kit 21K54 / 21K51 / 21K59)", "frame": "hull",
     "confidence": "kit_label+geometry",
     "members": [C("21K54", 1401.082, [0.378, 12.286, 0.237]), C("21K51", 2522.905, [0.0, 13.938, 7.929]),
                 C("21K59", 846.070, [23.112, 12.457, 0.543]), C("21K59", 846.070, [-23.112, 12.463, 0.543])],
     "evidence": ("Parts 21K54 'BOX(轉向機中)' (steering gear, middle; 1.43 g) and 21K51 'BOX(轉向機後)' "
                  "(steering gear, rear; 2.57 g) form the transverse housing at the hull front, with 21K59 "
                  "'COVER(外罩)' (outer cover, 0.86 g, two instances) drum covers on both ends facing the final "
                  "drives; all four in 0B21 DriverRoom and in the Base Unit STEP."),
     "doubts": "The transverse housing's internal split (steering gear vs brakes) is not modelled."},
    {"id": "radio", "name": "Radio sets (probable; kit 21K57 'BOX 控制盒', control box)", "frame": "hull",
     "confidence": "inferred",
     "members": [C("21K57", 1156.022, [-0.334, 27.663, -24.351])],
     "evidence": ("Part 21K57 'BOX(控制盒)' (control box, 0B21 DriverRoom, 1.18 g = 1.179 g; the other 1.18 g "
                  "part 27H12 is in the engine room): two stacked boxes on top of the gearbox between the driver "
                  "and the radio operator, with a face of knobs, plugs and panels turned towards the radio "
                  "operator's side (-X), where the Tiger I carried its radio sets."),
     "doubts": "The kit label says 'control box', not radio: the radio identification is inferred from position and face detail."},
    {"id": "final_drive_left", "name": "Final drive housing, left (kit 29D56 'DRIVER' + 18-22/23 'COVER 前輪殼')",
     "frame": "hull", "confidence": "kit_label+geometry",
     "members": [C("29D56", 572.607, [31.116, 8.403, 5.752]), C("18-22|18-23", 599.643, [26.610, 10.538, 11.603])],
     "evidence": ("Part 29D56 'DRIVER' (0.58 g = 0.584 g, two instances) is coaxial with the drive sprocket "
                  "(sprocket axis at kit y 8.2, z 6.0 mm) between the hull side and the sprocket; the mirror pair "
                  "18-22/18-23 'COVER(前輪殼)' (front-wheel housing, 0.61 g) is the casing plate behind it on the "
                  "hull side."),
     "doubts": "Only the external housing is modelled; the reduction gears inside are not."},
    {"id": "final_drive_right", "name": "Final drive housing, right (kit 29D56 'DRIVER' + 18-22/23 'COVER 前輪殼')",
     "frame": "hull", "confidence": "kit_label+geometry",
     "members": [C("29D56", 572.607, [-31.116, 8.434, 5.780]), C("18-22|18-23", 599.584, [-26.610, 10.538, 11.602])],
     "evidence": "Mirror of final_drive_left.",
     "doubts": "As final_drive_left."},
    {"id": "ammo_sponson_left_rear", "name": "Sponson ammunition bin, left rear (16 rounds)", "frame": "hull",
     "confidence": "kit_label",
     "members": [G("23M03", ROUND_V, [28, 20, -95], [43, 36, -66], 16), G("25M06-0A", SHELF_V, [34, 20, -84], [37, 35, -79], 4),
                 C("24H03", 517.061, [27.959, 27.743, -82.259])],
     "evidence": ("23M03 'AMMO(砲彈)' (shell: complete 8.8 cm round, 0.13 g = 0.125 g, 25.2 x 3.0 kit mm, 70 "
                  "identical solids, 4 x 4 in this bin), 25M06 'BLACKET(砲彈架)' (shell rack) shelves and 24H03 "
                  "'COVER(砲彈儲槽蓋)' (shell storage bin cover) closing the bin towards the fighting compartment."),
     "doubts": "The kit carries 70 rounds in total; the vehicle's full stowage is not modelled.",
     "game_mesh_note": ("tiger_hull.glb omits these 16 rounds (tiger_rack_source.json: cartridges excluded from "
                        "the static hull); the game draws rounds at tiger_interior.json shell slots on these rounds' "
                        "axes (64 of 65 slots: lateral offset <= 5.5 mm, slot point 74 mm along the axis from the "
                        "solid centroid). The bin cover is in no game mesh; the shelves are.")},
    {"id": "ammo_sponson_left_front", "name": "Sponson ammunition bin, left front (16 rounds)", "frame": "hull",
     "confidence": "kit_label",
     "members": [G("23M03", ROUND_V, [28, 20, -66], [43, 36, -40], 16), G("25M06-0A", SHELF_V, [34, 20, -55], [37, 35, -50], 4),
                 C("24H01|24H02", 462.615, [27.956, 28.484, -53.025])],
     "evidence": "As ammo_sponson_left_rear; cover 24H01/24H02 'COVER(砲彈儲槽蓋)' (0.47 g pair).",
     "doubts": "As ammo_sponson_left_rear.",
     "game_mesh_note": ("tiger_hull.glb omits these 16 rounds (tiger_rack_source.json: cartridges excluded from "
                        "the static hull); the game draws rounds at tiger_interior.json shell slots on these rounds' "
                        "axes (64 of 65 slots: lateral offset <= 5.5 mm, slot point 74 mm along the axis from the "
                        "solid centroid). The bin cover is in no game mesh; the shelves are.")},
    {"id": "ammo_sponson_right_rear", "name": "Sponson ammunition bin, right rear (16 rounds)", "frame": "hull",
     "confidence": "kit_label",
     "members": [G("23M03", ROUND_V, [-43, 20, -95], [-28, 36, -66], 16), G("25M06-0A", SHELF_V, [-37, 20, -84], [-34, 35, -79], 4),
                 C("24H04", 562.677, [-27.865, 27.476, -83.208])],
     "evidence": "As ammo_sponson_left_rear; cover 24H04 'COVER(砲彈儲槽蓋)' (0.57 g).",
     "doubts": "As ammo_sponson_left_rear; 25H27 'BLACKET' has the same 0.57 g mass, the cover is chosen by position.",
     "game_mesh_note": ("tiger_hull.glb omits these 16 rounds (tiger_rack_source.json: cartridges excluded from "
                        "the static hull); the game draws rounds at tiger_interior.json shell slots on these rounds' "
                        "axes (64 of 65 slots: lateral offset <= 5.5 mm, slot point 74 mm along the axis from the "
                        "solid centroid). The bin cover is in no game mesh; the shelves are.")},
    {"id": "ammo_sponson_right_front", "name": "Sponson ammunition bin, right front (16 rounds)", "frame": "hull",
     "confidence": "kit_label",
     "members": [G("23M03", ROUND_V, [-43, 20, -66], [-28, 36, -40], 16), G("25M06-0A", SHELF_V, [-37, 20, -55], [-34, 35, -50], 4),
                 C("24H01|24H02", 462.615, [-27.956, 28.484, -53.025])],
     "evidence": "As ammo_sponson_left_rear; cover 24H01/24H02.",
     "doubts": "As ammo_sponson_left_rear.",
     "game_mesh_note": ("tiger_hull.glb omits these 16 rounds (tiger_rack_source.json: cartridges excluded from "
                        "the static hull); the game draws rounds at tiger_interior.json shell slots on these rounds' "
                        "axes (64 of 65 slots: lateral offset <= 5.5 mm, slot point 74 mm along the axis from the "
                        "solid centroid). The bin cover is in no game mesh; the shelves are.")},
    {"id": "ammo_floor_left", "name": "Hull-floor rounds, left of the propeller shaft (3 rounds)", "frame": "hull",
     "confidence": "kit_label",
     "members": [G("23M03", ROUND_V, [9, 5, -70], [20, 10, -60], 3)],
     "evidence": "Three 23M03 rounds lying on the hull floor under the turret basket floor.",
     "doubts": "Their bin/tray structure is not identified; the envelope covers the rounds only."},
    {"id": "ammo_floor_right", "name": "Hull-floor rounds, right of the propeller shaft (3 rounds)", "frame": "hull",
     "confidence": "kit_label",
     "members": [G("23M03", ROUND_V, [-20, 5, -70], [-9, 10, -60], 3)],
     "evidence": "Mirror of ammo_floor_left.",
     "doubts": "As ammo_floor_left."},
    {"id": "hull_mg", "name": "Hull machine gun (kit 28B-K81 + 28B-K07 'GUN 機槍')", "frame": "hull",
     "confidence": "kit_label+geometry",
     "members": [C("28B-K81", 49.324, [-16.359, 30.961, -9.294]), C("28B-K07", 26.932, [-16.394, 30.447, -18.068])],
     "evidence": ("Two collinear rods on one axis (kit x -16.4, y 30.5-31.0, along z) behind the front plate on "
                  "the right (radio operator's) side; masses 0.05 g and 0.03 g match the front-armour unit's "
                  "28B-K81 and 28B-K07 'GUN(機槍)' (machine gun); the game node 'MG Operater' sits beside it."),
     "doubts": ("Many small parts share these masses; the labels rest on mass plus position. The ball mount "
                "and trigger parts are not included.")},
    {"id": "driver_seat", "name": "Driver's seat (kit 20K67 'CHAIR 椅子' + 20K65 'CHAIR 椅座架')", "frame": "hull",
     "confidence": "kit_label+geometry",
     "members": [C("20K67", 168.456, [14.500, 9.877, -18.541]), C("20K65", 58.742, [14.500, 4.998, -15.110])],
     "evidence": ("A chair with backrest (0.17 g, 20K67 'CHAIR(椅子)') on a four-legged frame (0.06 g, 20K65 "
                  "'CHAIR(椅座架)', seat frame), both 0B21 DriverRoom, at the left front driver's station."),
     "doubts": "Other parts share both masses; the labels rest on mass plus shape and position."},
    {"id": "fuel_cans_turret_floor", "name": "Fuel cans on the turret floor, three (kit 05O27 'FUEL 油箱')",
     "frame": "turret", "confidence": "kit_label+geometry",
     "members": [G("05O27", 480.371, [-10, 15, -85], [10, 28, -78], 3)],
     "evidence": ("Three identical boxes standing side by side at the rear of the turret basket floor; 0.49 g "
                  "matches 05O27 'FUEL(油箱)' (fuel tank, merged kit pieces O27-O30, 0C05 Room Unit); each is "
                  "4.5 x 12.5 x 10.1 kit mm (0.16 x 0.44 x 0.35 m at 1:35), a 20-litre canister's proportions."),
     "doubts": ("19K28_BASE (floor hatch) has the same mass; the CAD does not establish that real vehicles "
                "stowed cans here."),
     "game_mesh_note": ("No game mesh draws these cans. A damage volume here would be a hidden primitive with no "
                        "visible geometry (CLAUDE.md, physical truth rule 3): CAD record only until the cans are drawn.")},
    {"id": "gun_breech", "name": "88 mm gun breech ring and block (kit 02U03 'ROOM 供彈座')", "frame": "turret",
     "confidence": "kit_label", "elevates_with_gun": True,
     "members": [C("02U03", 1091.077, [0.078, 48.092, -59.537])],
     "evidence": ("Part 02U03 'ROOM(供彈座)' (loading seat, merged U3+U4, 0C02 Barrel Base Unit, 1.11 g = 1.113 g, "
                  "unique) on the bore axis behind the trunnion; it is the solid the game's tiger_gun.glb names "
                  "'Breech' (matched by invariants)."),
     "doubts": ("Breech ring and block are one kit body (02U03 merges kit pieces U3+U4). The kit also lists a "
                "small 02J54 'BLOCK(炮門)' (breech block, 0.08 g) that was not located among the many 0.08 g solids.")},
    {"id": "gun_cradle", "name": "88 mm gun cradle and recoil cylinders (kit 02J07 'BASE 砲管座' + 02J50/02J51)",
     "frame": "turret", "confidence": "kit_label", "elevates_with_gun": True,
     "members": [C("02J07", 720.276, [-0.070, 48.272, -43.075]), C("02J51", 267.842, [-6.475, 48.243, -45.588]),
                 C("02J50", 254.436, [6.661, 48.228, -44.934])],
     "evidence": ("Part 02J07 'BASE(砲管座)' (gun-tube seat, merged J07+U18+J31, 0.73 g) on the bore axis between "
                  "the mantlet base and the breech, flanked by 02J50/02J51 'OIL BOTTLE(油壓罐)' (hydraulic "
                  "cylinders: recoil brake and recuperator, 0.26/0.27 g, unique) along the barrel at bore height."),
     "doubts": "05U13 'MOTOR' also weighs 0.73 g; it is the turret-floor solid (turret_traverse_motor)."},
    {"id": "coax_mg", "name": "Coaxial machine gun (kit 03J19 'GUN 機槍')", "frame": "turret",
     "confidence": "kit_label+geometry", "elevates_with_gun": True,
     "members": [C("03J19", 180.594, [-12.597, 48.934, -44.136])],
     "evidence": ("An MG-length body (32.9 kit mm = 1.15 m at 1:35) right of the gun at bore height, ammunition "
                  "belt links running to it; 0.18 g matches 03J19 'GUN(機槍)' (merged J19+J68+J16, 0C03 Gun Unit); "
                  "its axis passes the game's coax port (src/tank.rs TIGER_COAX_PORT, gun-frame x -0.497 m)."),
     "doubts": "Several small parts weigh 0.18 g; the label rests on mass plus shape and position."},
    {"id": "gunner_sight", "name": "Gunner's sight (kit 01J18 'VIEWER 觀測鏡')", "frame": "turret",
     "confidence": "kit_label+geometry", "elevates_with_gun": True,
     "members": [C("01J18", 174.442, [13.896, 50.119, -40.850])],
     "evidence": ("A 24 kit mm tube left of the gun at bore height whose ends coincide with the game's sight "
                  "eyepiece and objective (src/tank.rs TIGER_SIGHT_EYEPIECE / TIGER_SIGHT_OBJECTIVE); 0.18 g "
                  "matches 01J18 'VIEWER(觀測鏡)' (merged J18+J67, 0C01 Barrel Unit)."),
     "doubts": "Mass shared with other parts; the label rests on mass plus position.",
     "game_mesh_note": ("Only about a third of this kit solid's surface lies within 2 mm of the game's meshes: the "
                        "game's sight geometry differs from the kit part. The box locates the kit sight.")},
    {"id": "turret_traverse_motor", "name": "Turret traverse motor (kit 05U13 'MOTOR 馬達')", "frame": "turret",
     "confidence": "kit_label+geometry",
     "members": [C("05U13", 719.919, [10.290, 20.313, -67.764])],
     "evidence": ("Part 05U13 'MOTOR(馬達)' (merged U13+U14, 0C05 Room Unit, 0.73 g = 0.734 g) on the turret "
                  "basket floor beside the turret axis."),
     "doubts": ("02J07 has the same mass (it is the gun cradle on the bore axis); the motor type is not stated "
                "by the kit. The kit body includes the tall vertical drive shaft, so the box is mostly empty.")},
    {"id": "gunner_seat", "name": "Gunner's seat cushion (kit 06J20B 'SEAT 座墊')", "frame": "turret",
     "confidence": "kit_label+geometry",
     "members": [C("06J20B", 196.498, [14.982, 29.218, -62.159])],
     "evidence": ("Round seat cushion at the gunner's station left of the gun, 0.20 g = 06J20B 'SEAT(座墊)' "
                  "(seat cushion, 0C05 Room Unit); its game mesh (Turret_0092) uses the game's 'seats_black' "
                  "material."),
     "doubts": "Cushion only; the seat post and frame are separate small parts not included."},
]

GLOSS = {  # English for the kit's Chinese part names used in COMPONENTS (translation, not kit text)
    "引擎主體": "engine main body", "冷卻箱": "cooling box", "風扇罩": "fan cover", "油箱外蓋": "fuel tank outer cover",
    "轉向機": "steering gear", "轉向機中": "steering gear, middle", "轉向機後": "steering gear, rear",
    "外罩": "outer cover", "控制盒": "control box", "前輪殼": "front-wheel housing", "砲彈": "shell (round)",
    "砲彈架": "shell rack", "砲彈儲槽蓋": "shell storage bin cover", "機槍": "machine gun", "椅子": "chair",
    "椅座架": "seat frame", "油箱": "fuel tank", "供彈座": "loading seat", "砲管座": "gun-tube seat",
    "油壓罐": "hydraulic cylinder", "觀測鏡": "sight", "馬達": "motor", "座墊": "seat cushion",
}


def select_members(comp, kit):
    out = []
    for m in comp["members"]:
        if "com" in m:
            hits = [k for k in kit if abs(k["volume_mm3"] - m["vol"]) < 0.01
                    and np.linalg.norm(np.array(k["com_mm"]) - m["com"]) < 0.05]
            want = 1
        else:
            lo, hi = np.array(m["lo"]), np.array(m["hi"])
            hits = [k for k in kit if abs(k["volume_mm3"] - m["vol"]) < 0.02
                    and np.all(np.array(k["com_mm"]) >= lo) and np.all(np.array(k["com_mm"]) <= hi)]
            want = m["count"]
        if len(hits) != want:
            raise SystemExit("component %s member %s: expected %d solids, found %d"
                             % (comp["id"], m["part"], want, len(hits)))
        out.append((m["part"], sorted(k["id"] for k in hits)))
    return out


def fit_obb(P):
    """Smallest of: frame-aligned box, PCA box, and 1-degree sweeps about each frame axis."""
    def box(A):
        L = P @ A.T
        lo, hi = L.min(0), L.max(0)
        return float(np.prod(hi - lo)), A, lo, hi
    cands = [("frame_aligned", box(np.eye(3)))]
    w, v = np.linalg.eigh(np.cov((P - P.mean(0)).T))
    A = v[:, ::-1].T.copy()
    if np.linalg.det(A) < 0:
        A[2] = -A[2]
    cands.append(("pca", box(A)))
    for ax in range(3):
        for deg in range(1, 90):
            a = math.radians(deg)
            c, s = math.cos(a), math.sin(a)
            Rm = np.eye(3)
            i, j = [q for q in range(3) if q != ax]
            Rm[i, i], Rm[i, j], Rm[j, i], Rm[j, j] = c, -s, s, c
            cands.append(("sweep_%s_%ddeg" % ("xyz"[ax], deg), box(Rm)))
    best = min(cands, key=lambda q: q[1][0])
    aligned = cands[0]
    # Prefer the frame-aligned box unless a rotated one is more than 3 % smaller (reported either way).
    method, (vol, A, lo, hi) = aligned if aligned[1][0] <= 1.03 * best[1][0] else best
    centre = 0.5 * (lo + hi) @ A
    half = 0.5 * (hi - lo)
    # Canonical order: box axis k is the one closest to frame axis k, pointing along it.
    order, used = [], set()
    for k in range(3):
        j = max((j for j in range(3) if j not in used), key=lambda j: abs(A[j, k]))
        used.add(j)
        order.append(j)
    A = A[order] * np.sign([A[order[k], k] or 1.0 for k in range(3)])[:, None]
    half = half[order]
    if np.linalg.det(A) < 0:  # keep right-handed (flip the least-aligned axis)
        A[2] = -A[2]
    return {"method": method, "axes": A, "center": centre, "half": half, "volume": vol,
            "volume_vs_smallest_candidate": vol / best[1][0], "smallest_method": best[0]}


def ray_hollowness(P, T, n=24):
    """Cast an n x n grid of rays along each kit axis across a solid's box. Per axis it returns the
    fraction of grid rays that hit the solid and, of those, the fraction 'through a cavity': at
    least two separate wall segments with material filling under 30 % of the span between first
    entry and last exit. A shell (closed or cup-shaped) is hit by most rays on some axis and most of
    those rays cross a cavity; plates, ribs, rods and open frames are not."""
    tri = P[T]
    lo, hi = P.min(0), P.max(0)
    out = {}
    for ax in range(3):
        u, v = [q for q in range(3) if q != ax]
        fu = (np.arange(n) + 0.5 + 0.0137) / n
        fv = (np.arange(n) + 0.5 + 0.0291) / n
        gu, gv = np.meshgrid(lo[u] + fu * (hi[u] - lo[u]), lo[v] + fv * (hi[v] - lo[v]))
        R = np.c_[gu.ravel(), gv.ravel()]
        a, b, c = tri[:, 0], tri[:, 1], tri[:, 2]
        den = (b[:, v] - c[:, v]) * (a[:, u] - c[:, u]) + (c[:, u] - b[:, u]) * (a[:, v] - c[:, v])
        ok = np.abs(den) > 1e-12
        a, b, c, den = a[ok], b[ok], c[ok], den[ok]
        cavity_rays = hit_rays = 0
        for k in range(0, len(R), 64):
            r = R[k:k + 64]
            du = r[:, None, 0] - c[None, :, u]
            dv = r[:, None, 1] - c[None, :, v]
            l1 = ((b[:, v] - c[:, v])[None] * du + (c[:, u] - b[:, u])[None] * dv) / den[None]
            l2 = ((c[:, v] - a[:, v])[None] * du + (a[:, u] - c[:, u])[None] * dv) / den[None]
            l3 = 1 - l1 - l2
            inside = (l1 >= 0) & (l2 >= 0) & (l3 >= 0)
            depth = l1 * a[None, :, ax] + l2 * b[None, :, ax] + l3 * c[None, :, ax]
            for q in range(len(r)):
                d = np.sort(depth[q][inside[q]])
                if len(d) < 2 or len(d) % 2:
                    continue
                hit_rays += 1
                seg = d.reshape(-1, 2)
                spanlen = seg[-1, 1] - seg[0, 0]
                if len(seg) >= 2 and spanlen > 0 and float((seg[:, 1] - seg[:, 0]).sum()) < 0.3 * spanlen:
                    cavity_rays += 1
        out["xyz"[ax]] = {"hit": hit_rays / len(R), "cavity_of_hits": cavity_rays / hit_rays if hit_rays else 0.0}
    return out


def write_obj(path, groups, header):
    """groups: list of (object name, P (n,3) metres, T (m,3))."""
    with open(path, "w", newline="\n") as f:
        f.write("# %s\n" % header)
        base = 1
        for name, P, T in groups:
            f.write("o %s\n" % name)
            f.write("".join("v %.6f %.6f %.6f\n" % tuple(p) for p in P))
            f.write("".join("f %d %d %d\n" % (a + base, b + base, c + base) for a, b, c in T))
            base += len(P)


def sw_lookup(sw_parts):
    out = {}
    for p in sw_parts:
        if p["kind"] == "SLDPRT":
            out.setdefault(p["kit_number"], p)
    return out


def part_label(parts, sw):
    """Kit part record(s) for a member label like '28H21|28H22'."""
    recs = []
    for num in parts.split("|"):
        p = sw.get(num)
        if p is None:
            recs.append({"part": num, "note": "no SolidWorks file with this number"})
            continue
        name = p.get("part_name") or p["label"]
        zh = name[name.find("(") + 1:name.rfind(")")] if "(" in name else ""
        recs.append({"part": "TG_RM5010-" + num, "file_label": p["label"], "kit_name": name,
                     "gloss_en": GLOSS.get(zh), "assembly": p.get("assembly_drawing_no"),
                     "sw_mass_g": p.get("mass_g"), "spec": p.get("spec")})
    return recs



COVER_CELL_M = 0.004  # voxel edge for the game-mesh coverage test, engine metres


def _cell_keys(P):
    c = np.floor(P / COVER_CELL_M).astype(np.int64) + 4096
    return (c[:, 0] << 26) | (c[:, 1] << 13) | c[:, 2]


def game_mesh_cells(baked):
    """Per GLB (assembled with the game's rig constants at yaw 0 / elevation 0): sorted voxel keys
    of every vertex, and the triangles of every node with its bounding box for exact checks."""
    off = {"hull": np.zeros(3), "turret": GAME_TURRET_PIVOT, "gun": GAME_TURRET_PIVOT + GAME_GUN_TRUNNION}
    out, tris = {}, []
    for lab in ("hull", "turret", "gun"):
        ns = glb_node_meshes(os.path.join(baked, "tiger_%s.glb" % lab))
        out[lab] = np.unique(_cell_keys(np.concatenate([n["P"] for n in ns]) + off[lab]))
        for n in ns:
            tris.append((n["lo"] + off[lab], n["hi"] + off[lab], n["P"][n["T"]] + off[lab]))
    return {"keys": out, "tris": tris}


def surface_coverage(P_hull, glb, tol_m=0.002, samples=400):
    """Exact test: fraction of sampled solid vertices (hull frame) within tol_m of any game-GLB
    triangle (point-triangle distance, candidate nodes by bounding box)."""
    if len(P_hull) > samples:
        P_hull = P_hull[np.linspace(0, len(P_hull) - 1, samples).astype(int)]
    lo, hi = P_hull.min(0) - tol_m, P_hull.max(0) + tol_m
    T = [tr for nlo, nhi, tr in glb["tris"] if np.all(nhi >= lo) and np.all(nlo <= hi)]
    if not T:
        return 0.0
    T = np.concatenate(T)
    best = np.full(len(P_hull), np.inf)
    for k in range(0, len(T), 20000):
        a, b, c = T[k:k + 20000, 0], T[k:k + 20000, 1], T[k:k + 20000, 2]
        ab, ac = b - a, c - a
        n = np.cross(ab, ac)
        nn = np.linalg.norm(n, axis=1)
        ok = nn > 1e-14
        a, b, c, ab, ac, n, nn = a[ok], b[ok], c[ok], ab[ok], ac[ok], n[ok], nn[ok]
        n = n / nn[:, None]
        for q0 in range(0, len(P_hull), 50):
            q = P_hull[q0:q0 + 50][:, None, :]
            ap = q - a[None]
            dist_plane = np.einsum("pti,ti->pt", ap, n)
            proj = q - dist_plane[..., None] * n[None]
            # barycentric of the projection
            v0, v1, v2 = ab[None], ac[None], proj - a[None]
            d00 = np.einsum("pti,pti->pt", np.broadcast_to(v0, v2.shape), np.broadcast_to(v0, v2.shape))
            d01 = np.einsum("pti,pti->pt", np.broadcast_to(v0, v2.shape), np.broadcast_to(v1, v2.shape))
            d11 = np.einsum("pti,pti->pt", np.broadcast_to(v1, v2.shape), np.broadcast_to(v1, v2.shape))
            d20 = np.einsum("pti,pti->pt", v2, np.broadcast_to(v0, v2.shape))
            d21 = np.einsum("pti,pti->pt", v2, np.broadcast_to(v1, v2.shape))
            den = d00 * d11 - d01 * d01
            vv = (d11 * d20 - d01 * d21) / den
            ww = (d00 * d21 - d01 * d20) / den
            inside = (vv >= 0) & (ww >= 0) & (vv + ww <= 1)
            d = np.where(inside, np.abs(dist_plane), np.inf)
            for e0, e1 in ((a, b), (b, c), (c, a)):
                ev = (e1 - e0)[None]
                tt = np.clip(np.einsum("pti,pti->pt", q - e0[None], np.broadcast_to(ev, ap.shape)) /
                             np.maximum(np.einsum("pti,pti->pt", np.broadcast_to(ev, ap.shape), np.broadcast_to(ev, ap.shape)), 1e-30), 0, 1)
                cp = e0[None] + tt[..., None] * ev
                d = np.minimum(d, np.linalg.norm(q - cp, axis=2))
            best[q0:q0 + 50] = np.minimum(best[q0:q0 + 50], d.min(1))
    return float((best <= tol_m).mean())


def game_mesh_coverage(P_hull, cells, samples=3000):
    """Fraction of a solid's tessellation vertices (hull frame) that have a game-GLB vertex in the
    same or a neighbouring 4 mm cell: ~1 where the game renders this solid, ~0 where it does not."""
    if len(P_hull) > samples:
        P_hull = P_hull[np.linspace(0, len(P_hull) - 1, samples).astype(int)]
    best = (0.0, None)
    offs = np.array([[dx, dy, dz] for dx in (-1, 0, 1) for dy in (-1, 0, 1) for dz in (-1, 0, 1)])
    base = np.floor(P_hull / COVER_CELL_M).astype(np.int64) + 4096
    for lab, keys in cells["keys"].items():
        hit = np.zeros(len(base), bool)
        for o in offs:
            c = base + o
            k = (c[:, 0] << 26) | (c[:, 1] << 13) | c[:, 2]
            pos = np.searchsorted(keys, k)
            pos[pos >= len(keys)] = 0
            hit |= keys[pos] == k
        f = float(hit.mean())
        if f > best[0]:
            best = (f, lab)
    return best


def stage_components(args):
    t0 = time.time()
    d, kit = load_solids(args.out)
    reg = json.load(open(os.path.join(args.out, "registration.json")))
    sw = sw_lookup(json.load(open(os.path.join(args.out, "sw_parts.json")))["parts"])
    tr = reg["transform_kit_mm_to_engine_hull_m"]
    s, t = tr["s_m_per_kit_mm"], np.array(tr["t_m"])
    chord_mm = d["tessellation"]["chord_tolerance_mm"]
    by_id = {k["id"]: k for k in kit}
    glb_name = {p["kit_id"]: "%s:%s" % (lab, p["node"]) for lab, ps in reg["pairs"].items() for p in ps}
    os.makedirs(os.path.join(args.out, "components"), exist_ok=True)
    os.makedirs(os.path.join(args.out, "assemblies"), exist_ok=True)
    all_ids = sorted({i for c in COMPONENTS for _, ids in select_members(c, kit) for i in ids})
    meshes = load_kit_meshes(args.out, kit, all_ids)
    unions = {}
    up = os.path.join(args.out, "component_unions.json")
    if os.path.exists(up):
        unions = json.load(open(up)).get("components", {})
    comps = []
    hollow_cache = {}
    cells = game_mesh_cells(args.baked)
    for c in COMPONENTS:
        frame_off = np.zeros(3) if c["frame"] == "hull" else GAME_TURRET_PIVOT
        members = select_members(c, kit)
        ids = sorted(i for _, g in members for i in g)
        P = np.concatenate([meshes[i][0] for i in ids]) * s + t - frame_off
        obb = fit_obb(P)
        L = (P - obb["center"]) @ obb["axes"].T
        excess = np.maximum(np.abs(L) - obb["half"], 0.0)
        v_sum_kit = sum(by_id[i]["volume_mm3"] for i in ids)
        kit_obb = {"center_mm": rnd((obb["center"] + frame_off - t) / s, 6), "axes": rnd(obb["axes"].tolist(), 9),
                   "half_extents_mm": rnd(obb["half"] / s, 6)}
        u = unions.get(c["id"])
        if u and (u.get("solid_ids") != ids or u.get("obb_kit_mm") != kit_obb):
            u = None  # stale union measurement: never mix
        v_solid_kit = u["fused_volume_mm3"] if (u and "fused_volume_mm3" in u) else v_sum_kit
        box_v = obb["volume"]
        rec = {
            "id": c["id"], "name": c["name"], "frame": c["frame"], "confidence": c["confidence"],
            "kit_parts": [{"member": part, "solid_ids": g, "parts": part_label(part, sw),
                           "solid_mass_g_abs": rnd(sum(by_id[i]["volume_mm3"] for i in g) * ABS_DENSITY_G_PER_MM3 / len(g), 4)}
                          for part, g in members],
            "solid_ids": ids,
            "solid_fingerprints": [by_id[i]["fingerprint"] for i in ids],
            "glb_nodes": sorted({glb_name[i] for i in ids if i in glb_name}),
            "evidence": c["evidence"], "doubts": c["doubts"],
            "obb": {"center_m": rnd(obb["center"], 6), "axes": rnd(obb["axes"].tolist(), 7),
                    "half_extents_m": rnd(obb["half"], 6), "method": obb["method"],
                    "volume_vs_smallest_candidate": rnd(obb["volume_vs_smallest_candidate"], 4),
                    "smallest_candidate_method": obb["smallest_method"]},
            "obb_kit_mm": kit_obb,
            "volume_m3": {"solids_sum": rnd(v_sum_kit * s ** 3, 9),
                          "solids_fused": rnd(u["fused_volume_mm3"] * s ** 3, 9) if (u and "fused_volume_mm3" in u) else None,
                          "box": rnd(box_v, 9)},
            "envelope_error": {
                "max_vertex_outside_box_mm": rnd(float(np.linalg.norm(excess, axis=1).max()) * 1000, 6),
                "max_brep_outside_box_mm": rnd(u["max_brep_outside_box_kit_mm"] * s * 1000, 4) if u else None,
                "chord_tolerance_mm": rnd(chord_mm * s * 1000, 4),
                "empty_fraction": rnd(1.0 - v_solid_kit * s ** 3 / box_v, 4),
            },
        }
        if c["frame"] == "turret":
            rec["frame_note"] = "turret-local = engine hull - game turret pivot %s at turret yaw 0" % GAME_TURRET_PIVOT.tolist()
        if c.get("elevates_with_gun"):
            rec["elevates_with_gun"] = {"trunnion_turret_m": GAME_GUN_TRUNNION.tolist(),
                                        "cad_elevation_deg": 0.0,
                                        "note": ("box given at 0 deg elevation (the CAD's pose); it rotates with the gun about the "
                                                 "turret-local x axis through the trunnion point")}
        shells = []
        for i in ids:
            k = by_id[i]
            bb = np.array(k["aabb_mesh_mm"]["max"]) - np.array(k["aabb_mesh_mm"]["min"])
            fill = k["volume_mm3"] / float(np.prod(bb))
            wall = 2 * k["volume_mm3"] / k["area_mm2"]
            if fill >= 0.35 or bb.min() < 6 * wall:
                continue
            if i not in hollow_cache:
                hollow_cache[i] = ray_hollowness(*meshes[i])
            h = hollow_cache[i]
            if any(f["hit"] >= 0.5 and f["cavity_of_hits"] >= 0.5 for f in h.values()):
                shells.append({"solid_id": i, "mean_wall_kit_mm_2V_over_A": rnd(wall, 3), "box_fill": rnd(fill, 3),
                               "ray_test_by_axis": {kk: {q: rnd(w, 3) for q, w in vv.items()} for kk, vv in h.items()}})
        if shells:
            rec["kit_shell"] = {"solids": shells, "note": (
                "hollow kit moulding (rays cross separate thin walls around empty space): mean wall = "
                "2 x volume / area in kit millimetres. A 1/35 plastic part's wall is not the real casing and "
                "must not be used as one; the kit models no contents.")}
        if u and u.get("cavities"):
            rec["kit_cavities"] = {"solids": u["cavities"], "note": "internal void shells in the kit B-rep (volume in kit mm^3)"}
        cov = {i: game_mesh_coverage(meshes[i][0] * s + t, cells) for i in ids}
        for i in ids:  # vertex proximity misses large flat game triangles: settle those exactly
            if 0.05 <= cov[i][0] < 0.8:
                cov[i] = (surface_coverage(meshes[i][0] * s + t, cells), "exact")
        absent = [i for i in ids if cov[i][0] < 0.2]
        partial = [i for i in ids if 0.2 <= cov[i][0] < 0.8]
        rec["game_mesh"] = {
            "status": "absent" if len(absent) == len(ids) else ("partial" if absent or partial else "present"),
            "solids_absent": absent, "solids_partial": partial,
            "coverage_by_solid": {str(i): [rnd(cov[i][0], 3), cov[i][1]] for i in ids},
            "test": ("fraction of the solid's tessellation vertices with a game-GLB vertex within one 4 mm "
                     "cell (GLBs assembled with the game's rig constants); where that is between 0.05 and "
                     "0.8 it is replaced ('exact') by the fraction of 400 sampled vertices within 2 mm of a "
                     "game triangle")}
        if c.get("game_mesh_note"):
            rec["game_mesh"]["note"] = c["game_mesh_note"]
        comps.append(rec)
        groups = [("%s/%s/solid_%04d" % (c["id"], part.replace("|", "_or_"), i),
                   meshes[i][0] * s + t - frame_off, meshes[i][1]) for part, g in members for i in g]
        write_obj(os.path.join(args.out, "components", c["id"] + ".obj"), groups,
                  "%s - %s frame, engine metres (Tiger kit CAD solids; names component/kit part/solid id)"
                  % (c["id"], c["frame"]))
    asm, uniq = write_assemblies_and_index(args, kit, comps, reg, s, t)
    dump(os.path.join(args.out, "components.json"), {
        "schema": "panzer.tiger-cad-components.v1",
        "transform_kit_mm_to_engine_hull_m": {"s": s, "t_m": tr["t_m"], "R": "identity"},
        "turret_pivot_hull_m": GAME_TURRET_PIVOT.tolist(),
        "components": comps})
    print("[components] %d components, %.1f s" % (len(comps), time.time() - t0))
    return comps


NOT_IDENTIFIED = [
    {"what": "radio operator's, commander's and loader's seats",
     "why": "kit seat parts (06J20A, 06J24, 06J46, 06J75, 19K05, 19K12A) share masses with many small parts and were not located with evidence"},
    {"what": "two unlabelled boxes on the hull floor ahead of the engine bulkhead (kit 24H37_BOX, '---', 1.17 g each)",
     "where_kit_mm": [[-7.5, 9.2, -86.8], [7.5, 9.2, -85.0]],
     "why": "no description in the SolidWorks file and no distinctive shape; function unknown"},
    {"what": "hydraulic reservoirs in the driver's compartment (19K15 / 20H65 'HYDRAULIC 油壓罐')",
     "where_kit_mm": [[-22.3, 10.7, -10.1], [22.3, 10.4, -23.1]],
     "why": "located by mass and position but their system (brakes, steering, traverse) is not stated by the kit"},
    {"what": "engine ancillaries (exhaust manifolds 28H43/28H46, cylinder-bank COOLBOX 28U01/28U02, pipes, carburettors, 28U23/28U31)",
     "why": "named per solid in assemblies/engine_system.obj but not given damage envelopes (they sit on the engine block box)"},
    {"what": "further fuel tanks, batteries, turret-traverse oil reservoir, MG ammunition bags",
     "why": "not present as distinguishable kit parts, or not located with evidence"},
    {"what": "kit solid 35 (invalid B-rep) and solid 541 (negative volume)",
     "why": "defective STEP bodies; excluded from extents, never used in components"},
]


GUN_GROUP = [  # 88 mm gun parts for an exploded view (turret frame); labels by mass within 0C01/0C02 + position
    C("02U03_ROOM breech ring", 1091.077, [0.078, 48.092, -59.537]),
    C("02J07_BASE cradle", 720.276, [-0.070, 48.272, -43.075]),
    C("02J51_OIL BOTTLE recoil cylinder", 267.842, [-6.475, 48.243, -45.588]),
    C("02J50_OIL BOTTLE recoil cylinder", 254.436, [6.661, 48.228, -44.934]),
    C("02J02_PLATE recoil guard (side plate)", 425.928, [5.077, 46.512, -71.362]),
    C("01A03_SHIELD mantlet", 3081.27, [1.5, 48.3, -28.5]),
    C("01A33_BASE mantlet base", 996.30, [1.6, 48.2, -33.4]),
    C("01A04_BARREL barrel section", 984.42, [0.1, 48.2, -13.5]),
    C("01A30_BARREL barrel section", 936.48, [0.1, 48.2, 0.8]),
    C("01A29_BARREL barrel section", 245.04, [0.1, 48.2, 44.9]),
    C("01A31_BARREL muzzle brake", 217.82, [0.1, 48.3, 73.3]),
]


def _pick(kit, sel, tol_c=0.2):
    hits = [k for k in kit if abs(k["volume_mm3"] - sel["vol"]) < 0.02
            and np.linalg.norm(np.array(k["com_mm"]) - sel["com"]) < tol_c]
    if len(hits) != 1:
        raise SystemExit("assembly selector %s: expected 1 solid, found %d" % (sel["part"], len(hits)))
    return hits[0]["id"]


def mass_candidates(vol_mm3, sw_parts, folders=None):
    g = round(vol_mm3 * ABS_DENSITY_G_PER_MM3, 2)
    return [p["kit_number"] + "_" + p["label"] for p in sw_parts
            if p["kind"] == "SLDPRT" and p.get("mass_g") is not None and abs(p["mass_g"] - g) < 0.0051
            and (folders is None or any(f in p["folder"] for f in folders))]


def write_assemblies_and_index(args, kit, comps, reg, s, t):
    """Named exploded-view meshes for three assemblies and a per-solid label index."""
    d = json.load(open(os.path.join(args.out, "solids.json")))
    sw_parts = json.load(open(os.path.join(args.out, "sw_parts.json")))["parts"]
    eng_ids = sorted(m["full_id"] for m in d["subassembly_matches"]["engine"]["matches"] if m["full_id"] is not None)
    base_ids = {m["full_id"] for m in d["subassembly_matches"]["base"]["matches"] if m["full_id"] is not None}
    by_id = {k["id"]: k for k in kit}
    asm = {}
    asm["engine_system"] = ("hull", [("|".join(mass_candidates(by_id[i]["volume_mm3"], sw_parts, ["0B29_EngineSystem"])) or "unlabelled", i)
                                     for i in eng_ids])
    tr = []
    for cid in ("gearbox", "steering_unit", "radio", "final_drive_left", "final_drive_right"):
        c = next(c for c in comps if c["id"] == cid)
        for kp in c["kit_parts"]:
            for i in kp["solid_ids"]:
                tr.append(("%s:%s" % (cid, kp["member"]), i))
    asm["transmission_final_drives"] = ("hull", tr)
    asm["gun_88mm"] = ("turret", [(sel["part"], _pick(kit, sel)) for sel in GUN_GROUP])
    ids = sorted({i for _, (_, lst) in asm.items() for _, i in lst})
    meshes = load_kit_meshes(args.out, kit, ids)
    summary = {}
    for name, (frame, lst) in asm.items():
        off = np.zeros(3) if frame == "hull" else GAME_TURRET_PIVOT
        groups = [("%s/solid_%04d" % (lab.replace(" ", "_"), i), meshes[i][0] * s + t - off, meshes[i][1]) for lab, i in lst]
        write_obj(os.path.join(args.out, "assemblies", name + ".obj"), groups,
                  "%s - %s frame, engine metres; object = kit label(s)/solid id ('A|B' = mass cannot separate A and B)"
                  % (name, frame))
        summary[name] = {"frame": frame, "solids": [{"label": lab, "solid_id": i} for lab, i in lst]}
    comp_of = {i: c["id"] for c in comps for i in c["solid_ids"]}
    glb_of = {p["kit_id"]: (lab, p["node"]) for lab, ps in reg["pairs"].items() for p in ps}
    rows, uniq = [], 0
    for k in kit:
        cands = mass_candidates(k["volume_mm3"], sw_parts)
        uniq += len(cands) == 1
        g = glb_of.get(k["id"])
        rows.append({"id": k["id"], "fingerprint": k["fingerprint"], "valid": k["valid"] and k["volume_mm3"] > 0,
                     "in_base_unit_step": k["id"] in base_ids, "in_engine_system_step": k["id"] in eng_ids,
                     "glb": ("%s:%s" % g) if g else None, "component": comp_of.get(k["id"]),
                     "mass_g_abs": rnd(k["volume_mm3"] * ABS_DENSITY_G_PER_MM3, 4),
                     "sw_mass_candidates": cands[:12], "sw_mass_candidate_count": len(cands)})
    dump(os.path.join(args.out, "solid_index.json"), {
        "schema": "panzer.tiger-cad-solid-index.v1",
        "note": ("STEP solids carry no names; sw_mass_candidates are the SolidWorks parts whose cached mass "
                 "(2 decimals, ABS) equals the solid's volume x 1.02 g/cm^3. One candidate is a usable name; "
                 "many are not without position evidence."),
        "solids_with_exactly_one_mass_candidate": uniq,
        "assemblies": summary, "solids": rows})
    return summary, uniq


def stage_catalog(args):
    d, kit = load_solids(args.out)
    reg = json.load(open(os.path.join(args.out, "registration.json")))
    comps = json.load(open(os.path.join(args.out, "components.json")))["components"]
    tr = reg["transform_kit_mm_to_engine_hull_m"]
    chord_mm = d["tessellation"]["chord_tolerance_mm"]
    out, cad_only = [], []
    for c in comps:
        rec = {
            "id": c["id"], "name": c["name"], "frame": c["frame"], "confidence": c["confidence"],
            "kit_parts": [{"member": kp["member"], "solid_ids": kp["solid_ids"],
                           "kit_name": " | ".join(p.get("kit_name", p["part"]) for p in kp["parts"]),
                           "gloss_en": " | ".join(filter(None, (p.get("gloss_en") for p in kp["parts"]))) or None,
                           "sw_mass_g": [p.get("sw_mass_g") for p in kp["parts"]],
                           "solid_mass_g_abs": kp["solid_mass_g_abs"]} for kp in c["kit_parts"]],
            "solid_ids": c["solid_ids"],
            "evidence": c["evidence"], "doubts": c["doubts"],
            "obb": {k: c["obb"][k] for k in ("center_m", "axes", "half_extents_m")},
            "volume_m3": {"solids": c["volume_m3"]["solids_fused"] if c["volume_m3"]["solids_fused"] is not None
                          else c["volume_m3"]["solids_sum"], "box": c["volume_m3"]["box"]},
            "envelope_error": c["envelope_error"],
        }
        for k in ("frame_note", "elevates_with_gun", "kit_shell", "kit_cavities"):
            if k in c:
                rec[k] = c[k]
        gm = c["game_mesh"]
        rec["game_mesh"] = {k: gm[k] for k in ("status", "solids_absent", "solids_partial", "note") if k in gm}
        (cad_only if gm["status"] == "absent" else out).append(rec)
    cat = {
        "schema": "panzer.tiger-cad-components.v1",
        "generated_by": ["tools/bake_tiger_cad_components.py (FreeCAD)", "tools/tiger_cad_register.py"],
        "report": "docs/research/TIGER_CAD_COMPONENTS_20261102.md",
        "source": {
            "cad": "Rye Field Model RM-5010 Tiger I 1/35 full-interior kit CAD (SolidWorks AP214 STEP + SLDPRT metadata)",
            "step_sha256": {k: v["sha256"] for k, v in d["files"].items()},
            "labels": "kit part numbers and names from the SolidWorks files' custom properties (Tiger-I(SW).zip), linked to STEP solids by cached mass (ABS 1.02 g/cm^3) plus position",
        },
        "frames": {
            "hull": "engine hull metres: origin at ground contact, +Y up, +Z forward (barrel), +X vehicle left",
            "turret": "turret-local metres = hull minus the game turret pivot, turret yaw 0",
            "turret_pivot_hull_m": GAME_TURRET_PIVOT.tolist(),
            "gun_trunnion_turret_m": GAME_GUN_TRUNNION.tolist(),
        },
        "transform_kit_mm_to_engine_hull_m": {"s_m_per_kit_mm": tr["s_m_per_kit_mm"], "R": "identity", "t_m": tr["t_m"],
                                              "pairs": int(tr["fit"].split()[3]), "residual_mm": tr["residual"]},
        "scale_note": ("The game's Tiger is this kit CAD at %.7f m per kit mm, %.4f x the kit's nominal 1:35 "
                       "(0.035 m/mm): every box below is in that game scale, about 11 %% larger than the real "
                       "vehicle the kit represents." % (tr["s_m_per_kit_mm"], tr["s_m_per_kit_mm"] / NOMINAL_KIT_SCALE)),
        "tessellation_chord_tolerance": {"kit_mm": chord_mm, "engine_mm": rnd(chord_mm * tr["s_m_per_kit_mm"] * 1000, 4)},
        "envelope_error_fields": {
            "max_vertex_outside_box_mm": "largest distance of any tessellated vertex outside the box (box fitted to the vertices, so ~0)",
            "max_brep_outside_box_mm": ("largest excursion of the exact B-rep surface past any box face (OCC optimal "
                                        "bounds without triangulation, per box axis); a point's distance from the box is "
                                        "at most sqrt(3) times this"),
            "chord_tolerance_mm": "tessellation chord tolerance used for the vertices, engine millimetres",
            "empty_fraction": "1 - fused solid volume / box volume: how much of the box is not kit material",
        },
        "game_mesh_field": ("status present / partial / absent: whether the game's own GLBs draw this component's "
                            "kit solids (vertex proximity, exact point-to-triangle check where ambiguous)"),
        "components": out,
        "cad_only_components": {
            "warning": ("In the kit CAD but drawn by no game mesh. Using them as damage volumes would add hidden "
                        "primitives with no visible geometry (CLAUDE.md physical truth rule 3)."),
            "components": cad_only},
        "not_identified": NOT_IDENTIFIED,
    }
    dump(args.catalog, cat)
    print("[catalog] %d components + %d CAD-only -> %s" % (len(out), len(cad_only), args.catalog))


def run_freecad_union(args):
    exe = args.freecad
    env = dict(os.environ, TIGER_CAD_STAGE="union", TIGER_CAD_DIR=args.cad_dir, TIGER_CAD_OUT=os.path.abspath(args.out))
    script = os.path.join(os.path.dirname(os.path.abspath(__file__)), "bake_tiger_cad_components.py")
    import subprocess
    t0 = time.time()
    r = subprocess.run([exe, script], env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True)
    if r.returncode != 0 or "union stage" not in r.stdout:
        raise SystemExit("FreeCAD union stage failed:\n" + r.stdout[-2000:] + r.stderr[-2000:])
    print("[union] FreeCAD union stage %.1f s" % (time.time() - t0))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--stage", default="all", choices=["all", "sw", "register", "components", "union", "catalog"])
    ap.add_argument("--freecad", default="FreeCADCmd", help="FreeCAD console binary for the union stage")
    ap.add_argument("--cad-dir", default=DEFAULT_CAD_DIR)
    ap.add_argument("--baked", default=DEFAULT_BAKED)
    ap.add_argument("--out", default=os.path.join("assets", "fea", "tiger_cad"))
    ap.add_argument("--catalog", default=os.path.join("catalog", "tiger_cad_components.v1.json"))
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    t0 = time.time()
    if args.stage in ("all", "sw"):
        stage_sw(args)
    if args.stage in ("all", "register"):
        stage_register(args)
    if args.stage in ("all", "components"):
        stage_components(args)
    if args.stage in ("all", "union"):
        run_freecad_union(args)
        stage_components(args)  # merge the union measurements (stale ones are ignored)
    if args.stage in ("all", "catalog"):
        stage_catalog(args)
    print("[tiger_cad_register] %s done in %.1f s" % (args.stage, time.time() - t0))


if __name__ == "__main__":
    main()
