"""Measure every solid of the owner's Tiger I kit CAD (Rye Field Model RM-5010, 1/35) in FreeCAD.

Stage 1 of the Tiger CAD component bake (stage 2, registration and component identification, is the
plain-Python tools/tiger_cad_register.py). This script only reads STEP files and writes measurements;
it never edits the CAD and needs no GPU.

Run from the repository root, headless, arguments through environment variables ONLY (FreeCADCmd
opens positional arguments as documents and then waits on stdin):

    TIGER_CAD_DIR=/path/to/step/folder FreeCADCmd tools/bake_tiger_cad_components.py < /dev/null
    python3 -I tools/tiger_cad_register.py      # stage 2 (calls the 'union' stage below itself)

Measured on the owner's Linux box (FreeCAD 1.1.4): about 3 minutes for the solids stage (reading the
176 MB full-assembly STEP takes ~25 s, tessellating its 1490 solids ~100 s), about 1 minute for the
union stage.

Environment variables (all optional):
    TIGER_CAD_DIR        folder holding the three STEP files (default: the owner's reference archive,
                         see DEFAULT_CAD_DIR below)
    TIGER_CAD_OUT        output folder (default: <cwd>/assets/fea/tiger_cad, gitignored)
    TIGER_CAD_CHORD_MM   tessellation chord tolerance (linear deflection), kit millimetres
                         (default 0.02 mm kit = 0.7 mm at 1:35)
    TIGER_CAD_ANGLE_DEG  tessellation angular deflection, degrees (default 20)
    TIGER_CAD_FILES      comma list of file keys to read: full,base,engine (default all three)
    TIGER_CAD_MESH       comma list of file keys to tessellate (default: full)
    TIGER_CAD_STAGE      'solids' (default) or 'union'. 'union' reads components.json written by
                         tools/tiger_cad_register.py and, per component, measures the boolean-fused
                         volume of its solids (overlap check), the volume of internal cavity shells,
                         and the exact B-rep extent in the component's box frame (how far the true
                         surface reaches past the box fitted to tessellation vertices), writing
                         component_unions.json. tools/tiger_cad_register.py runs it for you.

Outputs in TIGER_CAD_OUT (kit millimetres, the STEP file's own frame, unit density for inertia):
    solids.json          every solid of every file: canonical id, STEP reader order, volume, area,
                         centre of mass, inertia tensor about the centre of mass, sorted principal
                         moments and axes, axis-aligned box (OCC, plus tessellation where meshed),
                         principal-axis oriented box (tessellation vertices where meshed, exact
                         B-rep bounds otherwise), validity/closure, mesh stats, and the
                         subassembly-to-full-assembly matches with their tolerances.
    meshes_<key>.bin     tessellation of every solid of that file, little-endian:
                           8 bytes  magic b'TGRCADM1'
                           u32      format version (1)
                           u32      solid count N
                           N blocks, in canonical id order:
                             u32 id, u32 n_verts, u32 n_tris
                             f32[n_verts*3]  vertex positions, kit mm, STEP frame
                             u32[n_tris*3]   triangle vertex indices (OCC facet orientation)
                         solids.json gives each block's byte offset ("mesh.offset").
    bake_log.json        timings and input file sizes/SHA-256 (the only file with wall-clock data).

Canonical solid ids sort solids by centre of mass (z, y, x rounded to 1e-3 mm), then volume, then
area, so ids do not depend on the STEP reader's traversal order. Every solid also carries a
"fingerprint" (rounded volume/area/centre) for cross-checking ids between runs.

Matching a subassembly STEP to the full assembly uses rigid-motion invariants only: volume, area
and the sorted principal moments of inertia about the centre of mass (relative tolerances
MATCH_REL_TOL below). Candidates with unique invariants seed a least-squares rigid fit (Kabsch,
proper rotation) of subassembly centres onto full-assembly centres; every subassembly solid is
then assigned to the invariant-compatible full-assembly solid nearest its transformed centre,
accepted only within MATCH_POS_TOL_MM. Mirror-image parts have identical invariants; the rigid fit
separates them by position. Residuals of every accepted match are written out.
"""
import hashlib
import json
import math
import os
import struct
import sys
import time

import FreeCAD  # noqa: F401  (FreeCADCmd provides it)
import MeshPart
import Part
import numpy as np

DEFAULT_CAD_DIR = ("/windows/shop/ts_assets/_archive/_extracted/_Reference_CAD_Tigers/"
                   "panzer-vi-tiger-1-35-1.snapshot.2/panzer-vi-tiger-1-35-1.snapshot.2")
STEP_FILES = {
    "full": "TG_RM5010-0A_Tiger I.STEP",
    "base": "TG_RM5010-0B16_Base Unit.STEP",
    "engine": "TG_RM5010-0B29_EngineSystem.STEP",
}
MESH_MAGIC = b"TGRCADM1"
MESH_VERSION = 1
# Invariant tolerances for subassembly matching. Both files are OCC readings of the same
# SolidWorks bodies, so invariants agree far tighter than this; the tolerance only has to reject
# different parts. Measured agreement is reported per match.
MATCH_REL_TOL = {"volume": 1e-4, "area": 1e-4, "moments": 1e-3}
MATCH_POS_TOL_MM = 0.01


def env(name, default):
    v = os.environ.get(name)
    return default if v in (None, "") else v


def r(x, nd=6):
    """Round for deterministic JSON (no -0.0)."""
    if isinstance(x, (list, tuple, np.ndarray)):
        return [r(v, nd) for v in x]
    v = round(float(x), nd)
    return 0.0 if v == 0 else v


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 22), b""):
            h.update(chunk)
    return h.hexdigest()


def inertia_tensor(solid):
    m = solid.MatrixOfInertia  # about the centre of mass, unit density (OCC GProp)
    return np.array([[m.A11, m.A12, m.A13], [m.A21, m.A22, m.A23], [m.A31, m.A32, m.A33]], float)


def principal(I):
    """Sorted principal moments (ascending) and axes as rows, sign-fixed and right-handed."""
    w, v = np.linalg.eigh(0.5 * (I + I.T))
    order = np.argsort(w)
    w = w[order]
    axes = v[:, order].T.copy()
    for k in range(3):
        a = axes[k]
        j = int(np.argmax(np.abs(a)))
        if a[j] < 0:
            axes[k] = -a
    if np.dot(np.cross(axes[0], axes[1]), axes[2]) < 0:
        axes[2] = -axes[2]
    return w, axes


def tessellate(solid, chord, angle_rad):
    mesh = MeshPart.meshFromShape(Shape=solid, LinearDeflection=chord,
                                  AngularDeflection=angle_rad, Relative=False)
    pts, facets = mesh.Topology
    P = np.array([[p.x, p.y, p.z] for p in pts], dtype=np.float64).reshape(-1, 3)
    T = np.array(facets, dtype=np.int64).reshape(-1, 3)
    return P, T


def mesh_volume(P, T):
    if len(T) == 0:
        return 0.0
    a, b, c = P[T[:, 0]], P[T[:, 1]], P[T[:, 2]]
    return float(np.einsum("ij,ij->i", a, np.cross(b, c)).sum() / 6.0)


def obb_from_points(P, center, axes):
    loc = (P - center) @ axes.T
    lo, hi = loc.min(0), loc.max(0)
    c_loc = 0.5 * (lo + hi)
    return center + c_loc @ axes, 0.5 * (hi - lo)


def measure_file(key, path, chord, angle_rad, do_mesh, out_dir, log):
    t0 = time.time()
    shape = Part.read(path)
    t_read = time.time() - t0
    solids = list(shape.Solids)
    loose = {
        "shells_outside_solids": len(shape.Shells) - sum(len(s.Shells) for s in solids),
        "faces_outside_solids": len(shape.Faces) - sum(len(s.Faces) for s in solids),
    }
    rows = []
    for order, so in enumerate(solids):
        V = float(so.Volume)
        A = float(so.Area)
        c = so.CenterOfMass
        C = np.array([c.x, c.y, c.z], float)
        I = inertia_tensor(so)
        w, axes = principal(I)
        bb = so.BoundBox
        rows.append({
            "solid": so, "order": order, "V": V, "A": A, "C": C, "I": I, "w": w, "axes": axes,
            "occ_bb": [bb.XMin, bb.YMin, bb.ZMin, bb.XMax, bb.YMax, bb.ZMax],
        })
    rows.sort(key=lambda q: (round(q["C"][2], 3), round(q["C"][1], 3), round(q["C"][0], 3),
                             round(q["V"], 3), round(q["A"], 3), q["order"]))
    t_props = time.time() - t0 - t_read
    mesh_path = os.path.join(out_dir, "meshes_%s.bin" % key)
    fh = open(mesh_path + ".tmp", "wb") if do_mesh else None
    if fh:
        fh.write(MESH_MAGIC + struct.pack("<II", MESH_VERSION, len(rows)))
    out = []
    n_tris_total = 0
    for sid, q in enumerate(rows):
        so = q["solid"]
        rec = {
            "id": sid,
            "step_order": q["order"],
            "fingerprint": "V%.3f|A%.3f|C%.3f,%.3f,%.3f" % (q["V"], q["A"], *q["C"]),
            "volume_mm3": r(q["V"]),
            "area_mm2": r(q["A"]),
            "com_mm": r(q["C"]),
            "inertia_com_mm5": r(q["I"].tolist(), 4),
            "principal_moments_mm5": r(q["w"], 4),
            "principal_axes": r(q["axes"].tolist(), 7),
            "aabb_occ_mm": {"min": r(q["occ_bb"][:3]), "max": r(q["occ_bb"][3:])},
            "valid": bool(so.isValid()),
            "closed": bool(so.isClosed()),
            "n_faces": len(so.Faces),
            "n_shells": len(so.Shells),
        }
        if not do_mesh:
            # No tessellation for this file: principal-axis box from the exact B-rep bounds
            # (OCC optimal bounding box, no triangulation) in the principal frame.
            M = FreeCAD.Matrix()
            A = q["axes"]
            for a in range(3):
                for b in range(3):
                    setattr(M, "A%d%d" % (a + 1, b + 1), float(A[a, b]))
            tv = -A @ q["C"]
            M.A14, M.A24, M.A34 = float(tv[0]), float(tv[1]), float(tv[2])
            sh = so.copy()
            sh.transformShape(M)
            ob = sh.optimalBoundingBox(False, False)
            lo = np.array([ob.XMin, ob.YMin, ob.ZMin])
            hi = np.array([ob.XMax, ob.YMax, ob.ZMax])
            rec["obb_principal_mm"] = {"center": r(q["C"] + (0.5 * (lo + hi)) @ A), "axes": r(A.tolist(), 7),
                                       "half_extents": r(0.5 * (hi - lo)), "method": "exact B-rep bounds"}
        if do_mesh:
            P, T = tessellate(so, chord, angle_rad)
            mv = mesh_volume(P, T)
            lo, hi = P.min(0), P.max(0)
            oc, oh = obb_from_points(P, q["C"], q["axes"])
            rec["aabb_mesh_mm"] = {"min": r(lo), "max": r(hi)}
            rec["obb_principal_mm"] = {"center": r(oc), "axes": r(q["axes"].tolist(), 7),
                                       "half_extents": r(oh), "method": "tessellation vertices"}
            rec["mesh"] = {"file": os.path.basename(mesh_path), "offset": fh.tell(),
                           "n_verts": int(len(P)), "n_tris": int(len(T)),
                           "signed_volume_mm3": r(mv),
                           "volume_ratio_mesh_to_brep": r(mv / q["V"] if q["V"] else 0.0, 6)}
            fh.write(struct.pack("<III", sid, len(P), len(T)))
            fh.write(P.astype("<f4").tobytes())
            fh.write(T.astype("<u4").tobytes())
            n_tris_total += len(T)
        out.append(rec)
    if fh:
        fh.close()
        os.replace(mesh_path + ".tmp", mesh_path)
    t_total = time.time() - t0
    bbox = shape.BoundBox
    log[key] = {"read_s": round(t_read, 2), "props_s": round(t_props, 2),
                "total_s": round(t_total, 2), "tris": n_tris_total}
    return {
        "file": os.path.basename(path),
        "solid_count": len(out),
        "bbox_occ_mm": {"min": r([bbox.XMin, bbox.YMin, bbox.ZMin]),
                        "max": r([bbox.XMax, bbox.YMax, bbox.ZMax])},
        "geometry_outside_solids": loose,
        "solids": out,
    }, rows


def kabsch(A, B):
    """Proper rotation R and translation t minimising |R a + t - b|."""
    ca, cb = A.mean(0), B.mean(0)
    H = (A - ca).T @ (B - cb)
    U, S, Vt = np.linalg.svd(H)
    d = np.sign(np.linalg.det(Vt.T @ U.T))
    D = np.diag([1.0, 1.0, d])
    R = Vt.T @ D @ U.T
    return R, cb - R @ ca


def invariants_close(a, b):
    tv = MATCH_REL_TOL
    if abs(a["V"] - b["V"]) > tv["volume"] * max(a["V"], 1e-9):
        return False
    if abs(a["A"] - b["A"]) > tv["area"] * max(a["A"], 1e-9):
        return False
    scale = max(a["w"].max(), 1e-12)
    return bool(np.all(np.abs(a["w"] - b["w"]) <= tv["moments"] * scale))


def match_subassembly(sub_rows, full_rows):
    """Assign subassembly solids to full-assembly solids (see module docstring)."""
    fullV = np.array([q["V"] for q in full_rows])
    order = np.argsort(fullV)
    sortedV = fullV[order]
    cand = []
    for s in sub_rows:
        lo = np.searchsorted(sortedV, s["V"] * (1 - 2 * MATCH_REL_TOL["volume"]) - 1e-9)
        hi = np.searchsorted(sortedV, s["V"] * (1 + 2 * MATCH_REL_TOL["volume"]) + 1e-9)
        cand.append([int(order[k]) for k in range(lo, hi) if invariants_close(s, full_rows[order[k]])])
    # Seeds: invariant classes that are unique on both sides.
    sub_class = {}
    for i, c in enumerate(cand):
        sub_class.setdefault(tuple(c), []).append(i)
    seeds = [(i, c[0]) for i, c in enumerate(cand) if len(c) == 1 and len(sub_class[tuple(c)]) == 1]
    if len(seeds) < 3:
        # Fall back to all pairs inside small classes; the robust loop below rejects outliers.
        seeds = [(i, j) for i, c in enumerate(cand) for j in c if len(c) <= 4]
    if len(seeds) < 3:
        return None
    A = np.array([sub_rows[i]["C"] for i, _ in seeds])
    B = np.array([full_rows[j]["C"] for _, j in seeds])
    keep = np.ones(len(seeds), bool)
    for _ in range(20):
        R, t = kabsch(A[keep], B[keep])
        res = np.linalg.norm(A @ R.T + t - B, axis=1)
        thr = max(MATCH_POS_TOL_MM, 3.0 * float(np.median(res[keep])))
        new = res <= thr
        if new.sum() < 3 or np.array_equal(new, keep):
            break
        keep = new
    R, t = kabsch(A[keep], B[keep])
    used = {}
    matches = []
    for i, s in enumerate(sub_rows):
        p = R @ s["C"] + t
        ranked = sorted((float(np.linalg.norm(full_rows[j]["C"] - p)), j) for j in cand[i])
        best = next(((j, d) for d, j in ranked if j not in used), None)
        if best is None or best[1] > MATCH_POS_TOL_MM:
            matches.append({"sub_id": i, "full_id": None,
                            "n_invariant_candidates": len(cand[i]),
                            "nearest_candidate_mm": None if not ranked else r(ranked[0][0], 6)})
            continue
        j, d = best
        used[j] = i
        f = full_rows[j]
        # Principal-axis agreement (meaningful only for distinct moments).
        ax_err = []
        for k in range(3):
            ax_err.append(r(1.0 - abs(float(np.dot(R @ s["axes"][k], f["axes"][k]))), 9))
        matches.append({
            "sub_id": i, "full_id": j, "n_invariant_candidates": len(cand[i]),
            "centre_residual_mm": r(d, 7),
            "rel_dV": r((f["V"] - s["V"]) / s["V"], 9),
            "rel_dA": r((f["A"] - s["A"]) / s["A"], 9),
            "rel_dI_max": r(float(np.max(np.abs(f["w"] - s["w"])) / max(s["w"].max(), 1e-12)), 9),
            "axis_misalignment_1_minus_cos": ax_err,
        })
    acc = [m for m in matches if m["full_id"] is not None]
    res = np.array([m["centre_residual_mm"] for m in acc]) if acc else np.zeros(1)
    return {
        "rigid_transform_sub_to_full": {"R": r(R.tolist(), 9), "t_mm": r(t, 6),
                                         "det": r(float(np.linalg.det(R)), 9)},
        "tolerances": {"relative": MATCH_REL_TOL, "position_mm": MATCH_POS_TOL_MM},
        "seed_pairs": len(seeds), "seed_inliers": int(keep.sum()),
        "matched": len(acc), "unmatched": len(matches) - len(acc),
        "centre_residual_mm": {"max": r(float(res.max()), 7), "rms": r(float(np.sqrt((res ** 2).mean())), 7)},
        "matches": matches,
    }


def stage_solids():
    cad_dir = env("TIGER_CAD_DIR", DEFAULT_CAD_DIR)
    out_dir = env("TIGER_CAD_OUT", os.path.join(os.getcwd(), "assets", "fea", "tiger_cad"))
    chord = float(env("TIGER_CAD_CHORD_MM", "0.02"))
    angle_deg = float(env("TIGER_CAD_ANGLE_DEG", "20"))
    keys = [k for k in env("TIGER_CAD_FILES", "full,base,engine").split(",") if k]
    mesh_keys = set(k for k in env("TIGER_CAD_MESH", "full").split(",") if k)
    os.makedirs(out_dir, exist_ok=True)
    t0 = time.time()
    log = {"inputs": {}}
    result = {
        "schema": "panzer.tiger-cad-solids.v1",
        "units": "kit millimetres (STEP frame); inertia about centre of mass with unit density (mm^5)",
        "source": "Rye Field Model RM-5010 Tiger I 1/35 full-interior kit CAD (SolidWorks AP214 STEP)",
        "freecad": ".".join(FreeCAD.Version()[:3]),
        "tessellation": {"chord_tolerance_mm": chord, "angular_deflection_deg": angle_deg,
                         "mesher": "MeshPart.meshFromShape (OCC BRepMesh), Relative=False"},
        "files": {},
    }
    rows_by_key = {}
    for key in keys:
        path = os.path.join(cad_dir, STEP_FILES[key])
        log["inputs"][key] = {"file": STEP_FILES[key], "bytes": os.path.getsize(path), "sha256": sha256(path)}
        result["files"][key], rows_by_key[key] = measure_file(
            key, path, chord, math.radians(angle_deg), key in mesh_keys, out_dir, log)
        result["files"][key]["sha256"] = log["inputs"][key]["sha256"]
        print("[bake] %s: %d solids (%.1f s)" % (key, result["files"][key]["solid_count"], log[key]["total_s"]), flush=True)
    result["subassembly_matches"] = {}
    if "full" in rows_by_key:
        for key in ("engine", "base"):
            if key in rows_by_key:
                t1 = time.time()
                m = match_subassembly(rows_by_key[key], rows_by_key["full"])
                result["subassembly_matches"][key] = m
                log["match_%s_s" % key] = round(time.time() - t1, 2)
                if m:
                    print("[bake] match %s -> full: %d matched, %d unmatched, max residual %.2e mm"
                          % (key, m["matched"], m["unmatched"], m["centre_residual_mm"]["max"]), flush=True)
    with open(os.path.join(out_dir, "solids.json"), "w") as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write("\n")
    log["wall_s"] = round(time.time() - t0, 2)
    with open(os.path.join(out_dir, "bake_log.json"), "w") as f:
        json.dump(log, f, indent=1)
        f.write("\n")
    print("[bake] done in %.1f s -> %s" % (log["wall_s"], out_dir), flush=True)


def stage_union():
    """Per component (components.json from tools/tiger_cad_register.py): sum and boolean-fused volume
    of its solids, internal cavity shells, and the exact B-rep extent in the component's box frame
    (how far the true surface reaches past the box fitted to tessellation vertices)."""
    cad_dir = env("TIGER_CAD_DIR", DEFAULT_CAD_DIR)
    out_dir = env("TIGER_CAD_OUT", os.path.join(os.getcwd(), "assets", "fea", "tiger_cad"))
    comps = json.load(open(os.path.join(out_dir, "components.json")))
    solids = json.load(open(os.path.join(out_dir, "solids.json")))
    t0 = time.time()
    shape = Part.read(os.path.join(cad_dir, STEP_FILES["full"]))
    by_order = list(shape.Solids)
    id_to_order = {s["id"]: s["step_order"] for s in solids["files"]["full"]["solids"]}
    out = {}
    for c in comps["components"]:
        ids = c["solid_ids"]
        sol = [by_order[id_to_order[i]] for i in ids]
        vsum = sum(s.Volume for s in sol)
        rec = {"solid_ids": ids, "obb_kit_mm": c["obb_kit_mm"], "sum_volume_mm3": r(vsum)}
        if len(sol) > 1:
            try:
                fused = sol[0].fuse(sol[1:])
                rec["fused_volume_mm3"] = r(fused.Volume)
                rec["overlap_mm3"] = r(vsum - fused.Volume)
                rec["fuse_valid"] = bool(fused.isValid())
            except Exception as ex:  # report, never hide
                rec["fuse_error"] = repr(ex)
        else:
            rec["fused_volume_mm3"] = r(vsum)
            rec["overlap_mm3"] = 0.0
        cav = []
        for i, s in zip(ids, sol):
            if len(s.Shells) > 1:
                outer = Part.Solid(s.OuterShell)
                cav.append({"solid_id": i, "inner_shells": len(s.Shells) - 1,
                            "cavity_volume_mm3": r(abs(outer.Volume) - s.Volume)})
        if cav:
            rec["cavities"] = cav
        ob = c["obb_kit_mm"]
        A = np.array(ob["axes"], float)
        ctr = np.array(ob["center_mm"], float)
        h = np.array(ob["half_extents_mm"], float)
        M = FreeCAD.Matrix()
        for a in range(3):
            for b in range(3):
                setattr(M, "A%d%d" % (a + 1, b + 1), float(A[a, b]))
        tvec = -A @ ctr
        M.A14, M.A24, M.A34 = float(tvec[0]), float(tvec[1]), float(tvec[2])
        worst = np.full(3, -np.inf)
        for s in sol:
            sh = s.copy()
            sh.transformShape(M)
            bb = sh.optimalBoundingBox(False, False)
            ex = np.array([max(bb.XMax - h[0], -h[0] - bb.XMin), max(bb.YMax - h[1], -h[1] - bb.YMin),
                           max(bb.ZMax - h[2], -h[2] - bb.ZMin)])
            worst = np.maximum(worst, ex)
        rec["brep_outside_box_kit_mm_by_axis"] = r(worst, 6)
        rec["max_brep_outside_box_kit_mm"] = r(max(0.0, float(worst.max())), 6)
        out[c["id"]] = rec
    with open(os.path.join(out_dir, "component_unions.json"), "w") as f:
        json.dump({"schema": "panzer.tiger-cad-unions.v1",
                   "note": ("brep_outside_box: exact B-rep extent (OCC optimal bounding box without "
                            "triangulation) in the box frame minus the box half extent; positive = the true "
                            "surface reaches outside the box fitted to tessellation vertices"),
                   "components": out}, f, indent=1)
        f.write("\n")
    print("[bake] union stage: %d components in %.1f s" % (len(out), time.time() - t0), flush=True)


STAGE = env("TIGER_CAD_STAGE", "solids")
if STAGE == "solids":
    stage_solids()
elif STAGE == "union":
    stage_union()
else:
    raise SystemExit("unknown TIGER_CAD_STAGE %r" % STAGE)
