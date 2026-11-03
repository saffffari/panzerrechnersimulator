#!/usr/bin/env python3
"""Build the full interior anatomical voxel volume of one Tiger crewman (face01, relaxed pose).

    python3 -I tools/anatomy/build_anatomy.py                      # build, write volume + receipt
    python3 -I tools/anatomy/build_anatomy.py --verify             # rebuild, compare with receipt
    python3 -I tools/anatomy/build_anatomy.py --capture-edits FILE # store hand edits as an edit layer
    python3 -I tools/anatomy/build_anatomy.py --report FILE        # masses and checks of any volume

Inputs (assets are not in git; `KESSEL_ASSET_ROOT` or --asset-root names the directory holding
`assets/`): the relaxed crew figure the game voxelises (crew_tissue), the AdrielCZ authoring rig,
the crew bake report (pose rotations and input hashes), and catalog/crew_anatomy_tissues.v1.json.

Everything is deterministic: no random numbers, no timestamps in the outputs. The method, every
named approximation and the integration notes are in docs/CREW_ANATOMY_VOXELS.md.
"""

from __future__ import annotations

import argparse
import collections
import hashlib
import json
import math
import os
import sys
import time
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from anatomy_glb import Gltf, euler_xyz_matrix  # noqa: E402
import kvox  # noqa: E402

REPO = HERE.parents[1]
CREW = "assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4"
FIGURE = f"{CREW}/baked/german_panzertruppe_face01_relaxed_static.glb"
RIG = f"{CREW}/authoring/german_panzertruppe_rigged.glb"
BAKE_REPORT = f"{CREW}/bake.report.json"
DEFAULT_OUT = f"{CREW}/anatomy/german_panzertruppe_face01_relaxed_anatomy.kvox"
TABLE = REPO / "catalog/crew_anatomy_tissues.v1.json"
RECEIPT = REPO / "catalog/crew_anatomy_volume.v1.json"
EDITS = REPO / "catalog/crew_anatomy_edits.v1.json"
GENERATOR_VERSION = 1
NO_BONE = 255
LEFT = 1.0  # the figure faces +Z with +Y up, so its anatomical left is +X (the rig's L_ bones)


def log(*args):
    print("[anatomy]", *args, flush=True)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(Path(path).read_bytes())


def norm(v):
    v = np.asarray(v, dtype=np.float64)
    n = np.linalg.norm(v)
    return v / n if n > 0 else v


def smoothstep(e0, e1, x):
    t = np.clip((np.asarray(x, dtype=np.float64) - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


# ------------------------------------------------------------------------------------------------
# Geometry kernels
# ------------------------------------------------------------------------------------------------

def winding_numbers(points: np.ndarray, tris: np.ndarray, chunk: int = 2048) -> np.ndarray:
    """Generalized winding number of each point about a triangle soup (Van Oosterom and Strackee
    1983 solid angles, the same formula as crew_skeleton::winding_number)."""
    out = np.zeros(len(points))
    for s in range(0, len(points), chunk):
        p = points[s:s + chunk]
        a = tris[None, :, 0, :] - p[:, None, :]
        b = tris[None, :, 1, :] - p[:, None, :]
        c = tris[None, :, 2, :] - p[:, None, :]
        la, lb, lc = (np.linalg.norm(v, axis=2) for v in (a, b, c))
        num = np.einsum("nmk,nmk->nm", a, np.cross(b, c))
        den = (la * lb * lc + np.einsum("nmk,nmk->nm", a, b) * lc + np.einsum("nmk,nmk->nm", b, c) * la
               + np.einsum("nmk,nmk->nm", c, a) * lb)
        out[s:s + chunk] = 2.0 * np.arctan2(num, den).sum(axis=1) / (4.0 * math.pi)
    return out


def point_triangle_distance(points: np.ndarray, tris: np.ndarray, chunk: int = 768) -> np.ndarray:
    """Unsigned distance from each point to the nearest triangle (exact)."""
    a, b, c = tris[:, 0], tris[:, 1], tris[:, 2]
    n = np.cross(b - a, c - a)
    area2 = np.linalg.norm(n, axis=1)
    keep = area2 > 1e-14
    a, b, c, n = a[keep], b[keep], c[keep], n[keep] / area2[keep, None]
    edges = [(a, b), (b, c), (c, a)]
    out = np.empty(len(points))
    for s in range(0, len(points), chunk):
        p = points[s:s + chunk][:, None, :]
        ap = p - a[None]
        h = np.einsum("nmk,mk->nm", ap, n)
        q = p - h[..., None] * n[None]
        inside = np.ones(h.shape, dtype=bool)
        for e0, e1 in edges:
            inside &= np.einsum("nmk,mk->nm", np.cross(e1 - e0, q - e0[None]), n) >= 0.0
        d = np.where(inside, np.abs(h), np.inf)
        for e0, e1 in edges:
            ab = e1 - e0
            t = np.clip(np.einsum("nmk,mk->nm", p - e0[None], ab) / np.maximum((ab * ab).sum(1), 1e-18), 0.0, 1.0)
            closest = e0[None] + t[..., None] * ab[None]
            d = np.minimum(d, np.linalg.norm(p - closest, axis=2))
        out[s:s + chunk] = d.min(axis=1)
    return out


class Grid:
    """Voxel centres of a regular grid; arrays are indexed [y, z, x] (the KVOX layout)."""

    def __init__(self, origin, h, dims):
        self.origin = np.asarray(origin, dtype=np.float64)
        self.h = float(h)
        self.dims = [int(v) for v in dims]  # nx, ny, nz
        nx, ny, nz = self.dims
        self.xs = self.origin[0] + (np.arange(nx) + 0.5) * self.h
        self.ys = self.origin[1] + (np.arange(ny) + 0.5) * self.h
        self.zs = self.origin[2] + (np.arange(nz) + 0.5) * self.h
        self.shape = (ny, nz, nx)
        self.Y, self.Z, self.X = np.meshgrid(self.ys, self.zs, self.xs, indexing="ij")

    def box(self, lo, hi):
        lo = np.floor((np.asarray(lo) - self.origin) / self.h).astype(int)
        hi = np.floor((np.asarray(hi) - self.origin) / self.h).astype(int) + 1
        nx, ny, nz = self.dims
        i0, j0, k0 = np.maximum(lo, 0)
        i1, j1, k1 = np.minimum(hi, [nx, ny, nz])
        return (slice(j0, max(j0, j1)), slice(k0, max(k0, k1)), slice(i0, max(i0, i1)))

    def points(self, sl):
        return np.stack([self.X[sl], self.Y[sl], self.Z[sl]], axis=-1)

    def all_points(self):
        return np.stack([self.X.ravel(), self.Y.ravel(), self.Z.ravel()], axis=1)

    def index_of(self, p):
        i, j, k = np.floor((np.asarray(p) - self.origin) / self.h).astype(int)
        return j, k, i

    def row(self, y):
        return int(np.clip(np.floor((y - self.origin[1]) / self.h), 0, self.dims[1] - 1))


def dilate6(mask: np.ndarray, pad: bool = False) -> np.ndarray:
    p = np.pad(mask, 1, constant_values=pad)
    return (p[1:-1, 1:-1, 1:-1] | p[:-2, 1:-1, 1:-1] | p[2:, 1:-1, 1:-1] | p[1:-1, :-2, 1:-1]
            | p[1:-1, 2:, 1:-1] | p[1:-1, 1:-1, :-2] | p[1:-1, 1:-1, 2:])


def shell6(mask: np.ndarray) -> np.ndarray:
    """Voxels of `mask` with a face neighbour outside it."""
    return mask & dilate6(~mask, pad=True)


def segment_distance(P, a, b):
    ab = b - a
    t = np.clip(((P - a) @ ab) / max(ab @ ab, 1e-18), 0.0, 1.0)
    return np.linalg.norm(P - (a + t[..., None] * ab), axis=-1), t


def tube(grid: Grid, pts, radii) -> np.ndarray:
    """Voxels whose centre lies within the (linearly interpolated) radius of a polyline."""
    pts = np.asarray(pts, dtype=np.float64)
    radii = np.broadcast_to(np.asarray(radii, dtype=np.float64), (len(pts),))
    rmax = radii.max()
    sl = grid.box(pts.min(0) - rmax - grid.h, pts.max(0) + rmax + grid.h)
    P = grid.points(sl)
    score = np.full(P.shape[:-1], -np.inf)
    for (a, b, ra, rb) in zip(pts[:-1], pts[1:], radii[:-1], radii[1:]):
        d, t = segment_distance(P, a, b)
        score = np.maximum(score, ra + t * (rb - ra) - d)
    out = np.zeros(grid.shape, dtype=bool)
    out[sl] = score >= 0.0
    return out


def polyline_param(grid: Grid, pts, sl):
    """Distance to a polyline and the arc-length fraction (0..1) of the nearest point."""
    pts = np.asarray(pts, dtype=np.float64)
    lengths = np.linalg.norm(np.diff(pts, axis=0), axis=1)
    total = lengths.sum()
    starts = np.concatenate([[0.0], np.cumsum(lengths)[:-1]])
    P = grid.points(sl)
    best = np.full(P.shape[:-1], np.inf)
    s = np.zeros(P.shape[:-1])
    for a, b, l0, ln in zip(pts[:-1], pts[1:], starts, lengths):
        d, t = segment_distance(P, a, b)
        better = d < best
        best = np.where(better, d, best)
        s = np.where(better, (l0 + t * ln) / total, s)
    return best, s


def raster_line(grid: Grid, pts) -> np.ndarray:
    """A face-connected chain of voxels along a polyline (so a thin vessel or cord never breaks)."""
    pts = np.asarray(pts, dtype=np.float64)
    cells = []
    for a, b in zip(pts[:-1], pts[1:]):
        n = max(2, int(math.ceil(np.linalg.norm(b - a) / (grid.h / 8.0))) + 1)
        t = np.linspace(0.0, 1.0, n)[:, None]
        cells.append(np.floor((a + t * (b - a) - grid.origin) / grid.h).astype(int))
    ijk = np.concatenate(cells)
    chain = [ijk[0]]
    for c in ijk[1:]:
        prev = chain[-1]
        if (c == prev).all():
            continue
        cur = prev.copy()
        for axis in range(3):
            step = int(np.sign(c[axis] - prev[axis]))
            if step:
                cur = cur.copy()
                cur[axis] += step
                chain.append(cur)
    out = np.zeros(grid.shape, dtype=bool)
    nx, ny, nz = grid.dims
    for i, j, k in chain:
        if 0 <= i < nx and 0 <= j < ny and 0 <= k < nz:
            out[j, k, i] = True
    return out


def frame_from(primary, hint):
    a = norm(primary)
    b = np.cross(hint, a)
    if np.linalg.norm(b) < 1e-6:
        b = np.cross([1.0, 0.0, 0.0], a)
    b = norm(b)
    return np.stack([a, b, np.cross(a, b)])


def ellipsoid(grid: Grid, centre, axes, radii) -> np.ndarray:
    centre = np.asarray(centre, dtype=np.float64)
    rmax = max(radii)
    sl = grid.box(centre - rmax - grid.h, centre + rmax + grid.h)
    P = grid.points(sl) - centre
    q = sum(((P @ axes[i]) / radii[i]) ** 2 for i in range(3))
    out = np.zeros(grid.shape, dtype=bool)
    out[sl] = q <= 1.0
    return out


def components(mask: np.ndarray) -> list[int]:
    """Sizes of the face-connected components of a mask, largest first."""
    ny, nz, nx = mask.shape
    flat = mask.ravel()
    seen = np.zeros(flat.shape, dtype=bool)
    sizes = []
    strides = (nz * nx, nx, 1)
    for start in np.flatnonzero(flat):
        if seen[start]:
            continue
        seen[start] = True
        queue = collections.deque([start])
        size = 0
        while queue:
            v = queue.popleft()
            size += 1
            j, rem = divmod(v, nz * nx)
            k, i = divmod(rem, nx)
            for axis, (coord, limit) in enumerate(((j, ny), (k, nz), (i, nx))):
                for step in (-1, 1):
                    if 0 <= coord + step < limit:
                        n = v + step * strides[axis]
                        if flat[n] and not seen[n]:
                            seen[n] = True
                            queue.append(n)
        sizes.append(size)
    return sorted(sizes, reverse=True)


def largest_component(mask: np.ndarray, keep: int = 1) -> np.ndarray:
    """The `keep` largest face-connected components of a mask."""
    ny, nz, nx = mask.shape
    flat = mask.ravel()
    comp = np.zeros(flat.shape, dtype=np.int32)
    strides = (nz * nx, nx, 1)
    sizes = []
    for start in np.flatnonzero(flat):
        if comp[start]:
            continue
        cid = len(sizes) + 1
        comp[start] = cid
        queue = collections.deque([start])
        size = 0
        while queue:
            v = queue.popleft()
            size += 1
            j, rem = divmod(v, nz * nx)
            k, i = divmod(rem, nx)
            for axis, (coord, limit) in enumerate(((j, ny), (k, nz), (i, nx))):
                for step in (-1, 1):
                    if 0 <= coord + step < limit:
                        n = v + step * strides[axis]
                        if flat[n] and not comp[n]:
                            comp[n] = cid
                            queue.append(n)
        sizes.append(size)
    if not sizes:
        return mask.copy()
    best = np.argsort(sizes, kind="stable")[::-1][:keep] + 1
    return np.isin(comp, best).reshape(mask.shape)


def tune(make, target: int, lo: float = 0.2, hi: float = 3.0, iters: int = 26):
    """Scale parameter whose mask has the voxel count nearest `target` (count grows with s)."""
    best = None
    for _ in range(iters):
        s = 0.5 * (lo + hi)
        m = make(s)
        n = int(m.sum())
        if best is None or abs(n - target) < abs(best[1] - target):
            best = (s, n, m)
        if n < target:
            lo = s
        elif n > target:
            hi = s
        else:
            break
    return best


def ranked(candidates: np.ndarray, score: np.ndarray, n: int) -> np.ndarray:
    """The `n` candidate voxels with the lowest score (ties by layout order): a region-filling organ
    that hits its target volume exactly."""
    idx = np.flatnonzero(candidates)
    order = np.argsort(score.ravel()[idx], kind="stable")
    out = np.zeros(candidates.size, dtype=bool)
    out[idx[order[:max(0, n)]]] = True
    return out.reshape(candidates.shape)


# ------------------------------------------------------------------------------------------------
# Inputs: figure, rig, relaxed pose
# ------------------------------------------------------------------------------------------------

class Piece:
    def __init__(self, name, positions, triangles):
        self.name = name
        self.positions = positions
        self.triangles = triangles
        self.tris = positions[triangles]


def load_inputs(root: Path):
    report = json.loads((root / BAKE_REPORT).read_text(encoding="utf-8"))
    outputs = {o["path"]: o["sha256"] for o in report["runtime_outputs"]}
    figure_rel = FIGURE.removeprefix(CREW + "/")
    figure_sha = sha256_file(root / FIGURE)
    if outputs.get(figure_rel) != figure_sha:
        raise SystemExit(f"{FIGURE} does not match its bake receipt ({figure_sha})")
    rig_sha = sha256_file(root / RIG)
    if report["rigged_master"]["sha256"] != rig_sha:
        raise SystemExit(f"{RIG} does not match its bake receipt ({rig_sha})")
    static = Gltf.load(root / FIGURE)
    pieces = []
    for ni in static.mesh_nodes():
        prims = static.primitives(ni)
        if len(prims) != 1:
            raise SystemExit("expected one primitive per crew figure node")
        pieces.append(Piece(static.doc["nodes"][ni]["name"], prims[0]["positions"], prims[0]["triangles"]))
    pose = {p["pose"]: p["bone_rotations_degrees_xyz"] for p in report["poses"]}["relaxed"]
    inputs = {
        "figure": {"path": FIGURE, "sha256": figure_sha},
        "rig": {"path": RIG, "sha256": rig_sha},
        "bake_report": {"path": BAKE_REPORT, "sha256": sha256_file(root / BAKE_REPORT)},
    }
    return pieces, Gltf.load(root / RIG), pose, inputs


def relaxed_rig(rig: Gltf, pose: dict, pieces: list[Piece]):
    """Pose the authoring rig with the bake's relaxed rotations, re-centre it the way the bake did
    (x centre and floor of the posed bounds), and check that skinning reproduces the figure."""
    nodes = rig.doc["nodes"]
    skin = rig.doc["skins"][0]
    joints = skin["joints"]
    ibm = rig.accessor(skin["inverseBindMatrices"]).reshape(-1, 4, 4).transpose(0, 2, 1)
    by_name = {nodes[j]["name"]: j for j in joints}
    override = {}
    for name, degrees in pose.items():
        rotation = np.eye(4)
        rotation[:3, :3] = euler_xyz_matrix(degrees)
        override[by_name[name]] = rig.local_matrix(nodes[by_name[name]]) @ rotation
    world = rig.world_matrices(override)
    skinned = {}
    for ni in rig.mesh_nodes():
        prim = rig.primitives(ni, apply_node=False)[0]
        pos = np.c_[prim["positions"], np.ones(len(prim["positions"]))]
        out = np.zeros((len(pos), 3))
        for k in range(prim["joints"].shape[1]):
            m = np.stack([world[joints[j]] @ ibm[j] for j in prim["joints"][:, k]])
            out += prim["weights"][:, k:k + 1] * np.einsum("nij,nj->ni", m, pos)[:, :3]
        skinned[nodes[ni]["name"]] = (out, prim)
    every = np.concatenate([v[0] for v in skinned.values()])
    offset = np.array([-(every[:, 0].min() + every[:, 0].max()) / 2.0, -every[:, 1].min(), 0.0])
    worst = 0.0
    vertex_joint = {}
    for piece in pieces:
        out, prim = skinned[piece.name.removesuffix("_Static")]
        out = out + offset
        d2 = ((piece.positions[:, None, :] - out[None, :, :]) ** 2).sum(-1)
        nearest = d2.argmin(1)
        worst = max(worst, float(np.sqrt(d2.min(1)).max()))
        w = prim["weights"][nearest]
        vertex_joint[piece.name] = prim["joints"][nearest, w.argmax(1)]
    if worst > 1e-5:
        raise SystemExit(f"relaxed rig does not reproduce the figure (max {worst:.6f} m)")
    node_parent = rig.parents()
    joint_of_node = {n: i for i, n in enumerate(joints)}
    info = []
    for i, j in enumerate(joints):
        m = world[j].copy()
        m[:3, 3] += offset
        info.append({"index": i, "name": nodes[j]["name"], "parent": joint_of_node.get(node_parent[j], -1),
                     "matrix": m, "head": m[:3, 3].copy()})
    return info, worst, vertex_joint


# ------------------------------------------------------------------------------------------------
# The game's own part classifier (crew_physics::classify_relaxed_mesh_point), ported verbatim
# ------------------------------------------------------------------------------------------------

CREW_PARTS = ["Pelvis", "Torso", "Head", "LeftUpperArm", "LeftForearm", "RightUpperArm", "RightForearm",
              "LeftThigh", "LeftShin", "RightThigh", "RightShin"]


def classify_relaxed(x, y):
    x = np.asarray(x)
    y = np.asarray(y)
    out = np.where(y < 0.835, 0, 1)  # Pelvis / Torso
    leg = y < 0.67
    out = np.where(leg & (y >= 0.315), np.where(x < 0.0, 7, 9), out)
    out = np.where(leg & (y < 0.315), np.where(x < 0.0, 8, 10), out)
    out = np.where((x > 0.155) & (y >= 0.39), np.where(y >= 0.79, 5, 6), out)
    out = np.where((x < -0.155) & (y >= 0.39), np.where(y >= 0.79, 3, 4), out)
    out = np.where(y >= 1.275, 2, out)
    return out


# ------------------------------------------------------------------------------------------------
# The builder
# ------------------------------------------------------------------------------------------------

class Builder:
    def __init__(self, pieces, joints, table, voxel_m):
        self.t0 = time.time()
        self.pieces = pieces
        self.joints = joints
        self.J = {j["name"].removesuffix("_N1"): j["head"] for j in joints}
        self.table = table
        self.ID = {row["key"]: row["id"] for row in table["labels"]}
        self.rows = {row["id"]: row for row in table["labels"]}
        h = voxel_m
        allp = np.concatenate([p.positions for p in pieces])
        self.fig_lo, self.fig_hi = allp.min(0), allp.max(0)
        origin = np.floor(self.fig_lo / h + 1e-9) * h - h
        dims = np.ceil((self.fig_hi + h - origin) / h - 1e-9).astype(int)
        self.g = Grid(origin, h, dims)
        self.h = h
        self.notes = []
        self.clipped = {}
        self.tuned = {}
        self.parts = {}

    def elapsed(self):
        return f"{time.time() - self.t0:.1f}s"

    # -- masks and distance fields -------------------------------------------------------------
    def masks(self):
        g = self.g
        P = g.all_points()
        self.piece_inside = {}
        for piece in self.pieces:
            lo, hi = piece.positions.min(0) - g.h, piece.positions.max(0) + g.h
            sel = np.all((P >= lo) & (P <= hi), axis=1)
            w = np.zeros(len(P))
            w[sel] = winding_numbers(P[sel], piece.tris)
            self.piece_inside[piece.name] = (np.abs(w) > 0.5).reshape(g.shape)
        self.inside = np.zeros(g.shape, dtype=bool)
        for m in self.piece_inside.values():
            self.inside |= m
        names = [p.name for p in self.pieces]
        self.body_piece = self.piece_inside[names[0]]
        self.head_piece = self.piece_inside[names[1]]
        # The rig's L_ arm (anatomical left) is the +X piece.
        arm_a, arm_b = self.piece_inside[names[2]], self.piece_inside[names[3]]
        ax = self.g.X[arm_a].mean() if arm_a.any() else 0.0
        self.arm_left, self.arm_right = (arm_a, arm_b) if ax > 0 else (arm_b, arm_a)
        log(f"inside {int(self.inside.sum())} voxels ({self.inside.sum() * g.h ** 3 * 1000:.2f} L) {self.elapsed()}")

    def inside_points(self, P, pieces):
        hit = np.zeros(len(P), dtype=bool)
        for piece in pieces:
            hit |= np.abs(winding_numbers(P, piece.tris)) > 0.5
        return hit

    def exterior_tris(self, pieces_for_test, pieces_to_keep, eps=0.0015, max_depth=5):
        """The outer surface of the union of `pieces_for_test`: the parts of the triangles of
        `pieces_to_keep` with one side outside it. A triangle buried in another piece (or a nested
        inner shell) has both sides inside and is dropped; a triangle that is partly buried (the
        figure's arm meshes pass into the torso) is split recursively at its midpoints, down to
        `max_depth` levels, and only its exterior parts are kept (named: the exterior is resolved
        to 1/32 of a source triangle's edge)."""
        def flags(tris):
            a, b, c = tris[:, 0], tris[:, 1], tris[:, 2]
            n = np.cross(b - a, c - a)
            n = n / np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-18)
            out = []
            for s in ((a + b + c) / 3.0, (4 * a + b + c) / 6.0, (a + 4 * b + c) / 6.0, (a + b + 4 * c) / 6.0):
                out_plus = ~self.inside_points(s + eps * n, pieces_for_test)
                out_minus = ~self.inside_points(s - eps * n, pieces_for_test)
                out.append(out_plus ^ out_minus)
            return np.stack(out, axis=1)

        keep = []
        self.exterior_split = 0
        for piece in pieces_to_keep:
            batch = piece.tris
            for depth in range(max_depth + 1):
                if len(batch) == 0:
                    break
                f = flags(batch)
                full = f.all(axis=1)
                mixed = f.any(axis=1) & ~full
                keep.append(batch[full])
                if depth == max_depth:
                    keep.append(batch[mixed & (f.sum(axis=1) >= 2)])
                    break
                m = batch[mixed]
                self.exterior_split += len(m)
                a, b, c = m[:, 0], m[:, 1], m[:, 2]
                ab, bc, ca = 0.5 * (a + b), 0.5 * (b + c), 0.5 * (c + a)
                batch = np.concatenate([np.stack(t, axis=1) for t in ((a, ab, ca), (ab, b, bc), (ca, bc, c), (ab, bc, ca))])
        return np.concatenate(keep)

    def distances(self):
        g = self.g
        union_tris = self.exterior_tris(self.pieces, self.pieces)
        body = [self.pieces[0]]
        body_tris = self.exterior_tris(body, body)
        self.depth = np.full(g.shape, np.inf)
        self.depth_body = np.full(g.shape, np.inf)
        P = g.all_points()
        flat = self.inside.ravel()
        self.depth.ravel()[flat] = point_triangle_distance(P[flat], union_tris)
        bflat = self.body_piece.ravel()
        self.depth_body.ravel()[bflat] = point_triangle_distance(P[bflat], body_tris)
        self.notes.append(f"outer surface: {len(union_tris)} triangles and sub-triangles of the figure's "
                          f"{sum(len(p.tris) for p in self.pieces)} face the outside; body-piece surface: {len(body_tris)}")
        log(f"distance fields {self.elapsed()}")

    # -- landmarks -----------------------------------------------------------------------------
    def landmarks(self):
        g, J = self.g, self.J
        self.mid = 0.5 * (J["L_Leg"][0] + J["R_Leg"][0])
        col = int(np.argmin(np.abs(g.xs - self.mid)))
        rows = np.flatnonzero(self.inside[:, :, col].any(axis=1))
        legs_apart = rows[(g.ys[rows] > 0.3)]
        self.crotch_y = float(g.ys[legs_apart.min()] - 0.5 * g.h)
        self.H = float(self.fig_hi[1])
        self.eye = J["Eyes"].copy()
        # Profiles of the trunk at the midline, per row (smoothed): front and back surface, half-width.
        ny = g.dims[1]
        near_mid = np.abs(g.xs - self.mid) <= 1.01 * g.h
        zf, zb, hw = np.full(ny, np.nan), np.full(ny, np.nan), np.full(ny, np.nan)
        for j in range(ny):
            m = (self.body_piece[j] | self.head_piece[j])[:, near_mid].any(axis=1)
            if m.any():
                zf[j] = g.zs[m].max() + 0.5 * g.h
                zb[j] = g.zs[m].min() - 0.5 * g.h
            trunk = self.body_piece[j] | self.head_piece[j]
            cols = np.flatnonzero(trunk.any(axis=0))
            if len(cols):
                hw[j] = 0.5 * (g.xs[cols].max() - g.xs[cols].min() + g.h)
        self.zf, self.zb, self.hw = (self._smooth(v) for v in (zf, zb, hw))
        # Vertebral levels (authored proportions, docs): L5/S1 above the crotch, C7/T1 above the
        # shoulder joints, the atlas below the eyes.
        self.y_LS = self.crotch_y + 0.090 * self.H
        sh_y = 0.5 * (J["L_Arm"][1] + J["R_Arm"][1])
        self.y_CT = sh_y + 0.053 * self.H
        self.y_C1 = self.eye[1] - 0.030
        self.y_TL = self.y_LS + 0.389 * (self.y_CT - self.y_LS)
        self.levels = {}
        for names, top, bottom, disc in ((["C%d" % i for i in range(1, 8)], self.y_C1, self.y_CT, 0.22),
                                         (["T%d" % i for i in range(1, 13)], self.y_CT, self.y_TL, 0.18),
                                         (["L%d" % i for i in range(1, 6)], self.y_TL, self.y_LS, 0.22)):
            seg = (top - bottom) / len(names)
            for n, name in enumerate(names):
                y1 = top - n * seg
                self.levels[name] = (y1 - seg, y1, disc * seg)
        # Femoral heads: the rig's hip joints sit at crotch height; the anatomical femoral head is
        # set 0.065 x H above the crotch (named correction), on the rig's hip x and z.
        self.fh_y = self.crotch_y + 0.0425 * self.H
        self.FH = {s: np.array([J[f"{s}_Leg"][0], self.fh_y, J[f"{s}_Leg"][2] + 0.005]) for s in ("L", "R")}
        self.notes.append(f"crotch {self.crotch_y:.3f} m; L5/S1 {self.y_LS:.3f}; T12/L1 {self.y_TL:.3f}; "
                          f"C7/T1 {self.y_CT:.3f}; C1 top {self.y_C1:.3f}; femoral heads {self.fh_y:.3f}")
        log(self.notes[-1])

    def _smooth(self, v, window=5):
        idx = np.arange(len(v))
        ok = ~np.isnan(v)
        filled = np.interp(idx, idx[ok], v[ok])
        k = np.ones(window) / window
        sm = np.convolve(np.pad(filled, window // 2, mode="edge"), k, mode="valid")
        return sm

    def prof(self, arr, y):
        return np.interp(y, self.g.ys, arr)

    def level(self, name, t=0.5):
        lo, hi, _ = self.levels[name]
        return lo + t * (hi - lo)

    def zc(self, y):
        return 0.5 * (self.prof(self.zf, y) + self.prof(self.zb, y))

    def hd(self, y):
        return 0.5 * (self.prof(self.zf, y) - self.prof(self.zb, y))

    def body_point(self, u, y, w):
        """A point from body-normalized coordinates: u = -1..1 from the right to the left side of
        the trunk surface, w = -1..1 from back to front, at height y."""
        return np.array([self.mid + u * self.prof(self.hw, y), y, self.zc(y) + w * self.hd(y)])

    def uw(self):
        g = self.g
        hw = np.interp(g.Y, g.ys, self.hw)
        zf = np.interp(g.Y, g.ys, self.zf)
        zb = np.interp(g.Y, g.ys, self.zb)
        u = (g.X - self.mid) / hw
        w = (g.Z - 0.5 * (zf + zb)) / (0.5 * (zf - zb))
        return u, w

    def spine_z(self, y):
        """Vertebral body centre z: a fraction of the depth from the back surface (authored)."""
        y = np.asarray(y, dtype=np.float64)
        y_hold = self.level("C3")
        yy = np.minimum(y, y_hold)
        f = np.interp(yy, [self.y_LS - 0.11, self.y_LS - 0.02, self.y_LS + 0.02, self.y_TL - 0.02,
                           self.y_TL + 0.02, self.y_CT - 0.02, self.y_CT + 0.02, 10.0],
                      [0.15, 0.31, 0.36, 0.36, 0.32, 0.32, 0.47, 0.47])
        zb, zf = self.prof(self.zb, yy), self.prof(self.zf, yy)
        # never closer to the back than the erector spinae allow: the canal stays two voxels deep
        floor = np.interp(yy, [self.y_LS - 0.05, self.y_LS, self.y_CT, self.y_CT + 0.02], [0.036, 0.050, 0.046, 0.040])
        return zb + np.maximum(f * (zf - zb), floor)

    # -- label painting ------------------------------------------------------------------------
    def paint(self, key, mask, *, where=None, record=None):
        m = mask & self.inside
        if where is not None:
            m &= where
        if record:
            outside = mask & ~self.inside
            self.clipped[record] = self.clipped.get(record, 0) + int(outside.sum())
        self.L[m] = self.ID[key]
        return m

    def build(self, reference_volume_m3: float):
        """Paint every label. Order matters: later structures override earlier ones, so the
        skeleton comes before the organs (which then avoid bone), thin continuous structures
        (vessels, airway, cord) come late, and the closed skin shell is painted last."""
        self.masks()
        # k: this figure's volume over the reference man's; every reference volume is scaled by it.
        self.k = float(self.inside.sum() * self.h ** 3 / reference_volume_m3)
        self.distances()
        self.landmarks()
        g = self.g
        self.L = np.zeros(g.shape, dtype=np.uint8)
        self.L[self.inside] = self.ID["muscle"]
        self.skin = shell6(self.inside)
        self.layer2 = shell6(self.inside & ~self.skin)
        self.u, self.w = self.uw()
        self.bind_bones()
        self.fat()
        self.thorax_and_abdomen_cavities()
        self.skeleton()
        bone = np.isin(self.L, [self.ID[k] for k in ("cortical_bone", "spongiosa", "yellow_marrow", "cartilage")])
        self.thorax_cav &= ~bone
        self.abd_cav &= ~bone
        self.organs_abdomen()
        self.organs_thorax()
        self.head()
        self.vessels_and_airways()
        self.spinal_cord()
        self.seal_brain()
        self.L[self.skin] = self.ID["skin"]
        self.L[~self.inside] = self.ID["exterior"]
        log(f"labels painted {self.elapsed()}")
        return self.L

    def abd_vessel_paths(self):
        """The abdominal aorta and inferior vena cava, in front of the lumbar spine (authored)."""
        Lv, mid = self.level, self.mid
        spz = lambda yy: float(self.spine_z(yy))
        front = lambda yy, r: spz(yy) + float(self.vert_half_depth(yy)) + r + 0.003
        aorta = [np.array([mid + 0.014, Lv("T11"), front(Lv("T11"), 0.0095)]), np.array([mid + 0.012, Lv("T12"), front(Lv("T12"), 0.0095)]),
                 np.array([mid + 0.010, Lv("L2"), front(Lv("L2"), 0.0085)]), np.array([mid + 0.008, Lv("L4"), front(Lv("L4"), 0.008)])]
        ivc = [np.array([mid - 0.022, Lv("T11"), front(Lv("T11"), 0.0095) + 0.006]), np.array([mid - 0.022, Lv("T12"), front(Lv("T12"), 0.0095)]),
               np.array([mid - 0.018, Lv("L2"), front(Lv("L2"), 0.0095)]), np.array([mid - 0.012, Lv("L5"), front(Lv("L5"), 0.0095)])]
        return aorta, ivc

    # -- rig binding ---------------------------------------------------------------------------
    def bone_segments(self):
        J = self.J
        cont = {"ROOT": "Spine_1", "Spine_1": "Spine_2", "Spine_2": "Chest", "Chest": "Neck", "Neck": "Head"}
        for s in ("L", "R"):
            cont.update({f"{s}_Shoulder": f"{s}_Arm", f"{s}_Arm": f"{s}_Elbow", f"{s}_Elbow": f"{s}_Wrist",
                         f"{s}_Wrist": f"{s}_Hand", f"{s}_Hand": f"{s}_Fingers_1", f"{s}_Fingers_1": f"{s}_Fingers_2",
                         f"{s}_Fingers_2": f"{s}_Fingers_3", f"{s}_Thumb_1": f"{s}_Thumb_2", f"{s}_Thumb_2": f"{s}_Thumb_3",
                         f"{s}_Leg": f"{s}_Knee", f"{s}_Knee": f"{s}_Ankle", f"{s}_Ankle": f"{s}_Foot"})
        segs = {}
        for j in self.joints:
            name = j["name"].removesuffix("_N1")
            a = J[name]
            if name in cont:
                b = J[cont[name]]
            elif name == "Head":
                b = a + np.array([0.0, 0.11, 0.0])
            elif name.endswith("Fingers_3") or name.endswith("Thumb_3"):
                parent = name[:-1] + "2"
                b = a + 0.6 * (a - J[parent])
            elif name.endswith("_Foot"):
                ank = J[name.replace("Foot", "Ankle")]
                d = a - ank
                d[1] = 0.0
                b = a + 0.3 * d
            else:  # Eyes, heels: a point bone
                b = a.copy()
            segs[j["index"]] = (name, a, b)
        return segs

    def bind_bones(self):
        """Dominant rig bone per voxel: the nearest bone segment among the bones of the figure
        piece(s) the voxel lies in (named approximation of the skin binding)."""
        g = self.g
        segs = self.bone_segments()
        arm = {s: {i for i, (n, _, _) in segs.items() if n.startswith(f"{s}_") and n.split("_", 1)[1].split("_")[0]
                   in ("Shoulder", "Arm", "Elbow", "Wrist", "Hand", "Fingers", "Thumb")} for s in ("L", "R")}
        head = {i for i, (n, _, _) in segs.items() if n in ("Neck", "Head")}
        body = {i for i, (n, _, _) in segs.items() if n in ("ROOT", "Spine_1", "Spine_2", "Chest", "Neck")
                or n.split("_", 1)[-1] in ("Shoulder", "Leg", "Knee", "Ankle", "Foot", "Heel")}
        self.bone = np.full(g.shape, NO_BONE, dtype=np.uint8)
        P = g.all_points()
        groups = [(self.body_piece, body), (self.head_piece, head), (self.arm_left, arm["L"]), (self.arm_right, arm["R"])]
        best = np.full(len(P), np.inf)
        which = np.full(len(P), NO_BONE, dtype=np.int64)
        for mask, allowed in groups:
            sel = np.flatnonzero(mask.ravel())
            Q = P[sel]
            for i in sorted(allowed):
                _, a, b = segs[i]
                d, _ = segment_distance(Q, a, b)
                better = d < best[sel]
                best[sel] = np.where(better, d, best[sel])
                which[sel] = np.where(better, i, which[sel])
        self.bone.ravel()[:] = np.where(self.inside.ravel(), which, NO_BONE).astype(np.uint8)
        names = {i: n for i, (n, _, _) in segs.items()}
        self.region = np.zeros(g.shape, dtype=np.int8)
        # regions: 1 trunk, 2 head/neck, 3 upper arm, 4 forearm, 5 hand, 6 thigh, 7 shin, 8 foot
        kind = {"ROOT": 1, "Spine_1": 1, "Spine_2": 1, "Chest": 1, "Neck": 2, "Head": 2, "Eyes": 2}
        for i, n in names.items():
            base = n.split("_", 1)[-1]
            if n in kind:
                k = kind[n]
            elif base == "Shoulder":
                k = 1
            elif base == "Arm":
                k = 3
            elif base == "Elbow":
                k = 4
            elif base.split("_")[0] in ("Wrist", "Hand", "Fingers", "Thumb"):
                k = 5
            elif base == "Leg":
                k = 6
            elif base == "Knee":
                k = 7
            else:
                k = 8
            self.region[self.bone == i] = k
        self.segs = segs

    # -- subcutaneous fat ----------------------------------------------------------------------
    def fat(self):
        """Fat: voxels under the skin shell whose centre lies closer to the outer surface than the
        region's authored thickness (trunk front thin, abdomen, buttocks and thighs thicker)."""
        u, w = self.u, self.w
        t = np.zeros(self.g.shape)
        trunk = self.region == 1
        y = self.g.Y
        # chest: almost none in front (pectoral muscle lies under the skin shell), some behind;
        # abdomen and flanks: one voxel; buttocks: thicker (authored, a lean soldier)
        chest = smoothstep(self.y_TL - 0.02, self.y_TL + 0.03, y)
        back = smoothstep(0.0, -0.6, w)
        t_chest = 0.0105 + 0.0045 * back
        t_abdomen = 0.0215 - 0.004 * back + 0.006 * back * smoothstep(self.y_LS + 0.03, self.y_LS - 0.03, y)
        t_trunk = chest * t_chest + (1.0 - chest) * t_abdomen
        t = np.where(trunk, t_trunk, t)
        t = np.where(self.region == 6, 0.0185, t)
        t = np.where(self.region == 3, 0.0155, t)
        t = np.where(self.region == 4, 0.0135, t)
        t = np.where(self.region == 7, 0.0125, t)
        fat = self.inside & ~self.skin & (self.depth < t)
        self.L[fat] = self.ID["subcutaneous_fat"]
        self.subcut = fat

    # -- cavities ------------------------------------------------------------------------------
    def wall_depth(self):
        """Depth (from the trunk's own surface) of the inner edge of the chest/abdominal wall: the rib
        layer starts here. Thicker behind (back muscles) and beside the spine (authored)."""
        u, w = self.u, self.w
        y = self.g.Y
        post = smoothstep(-0.3, -0.7, w)
        para = post * smoothstep(0.55, 0.15, np.abs(u))
        # pectoral muscle over the upper front of the chest; below it the ribs lie under the skin
        pect = smoothstep(0.1, 0.5, w) * smoothstep(self.level("T6"), self.level("T4"), y) * smoothstep(0.85, 0.6, np.abs(u))
        return 0.0105 + 0.009 * pect + 0.009 * post + 0.012 * para

    def thorax_and_abdomen_cavities(self):
        g, h = self.g, self.h
        y = g.Y
        self.t_rib = self.wall_depth()
        self.rib_band = self.body_piece & (self.depth_body >= self.t_rib) & (self.depth_body < self.t_rib + 1.2 * h)
        interior = self.body_piece & (self.depth_body >= self.t_rib + 1.2 * h) & ~self.subcut
        # the abdominal wall: skin, fat and one layer of muscle (rectus, obliques, transversus)
        abd_interior = self.body_piece & (self.depth_body >= 0.0285) & ~self.subcut
        # Diaphragm: two domes and the central tendon, in body-normalized coordinates (authored).
        u, w = self.u, self.w
        y_r = self.level("T8")
        y_l = y_r - 0.013
        y_c = y_r - 0.010
        k = 0.12
        dome_r = y_r - k * ((u + 0.42) ** 2 + ((w - 0.05) / 1.4) ** 2)
        dome_l = y_l - k * ((u - 0.45) ** 2 + ((w - 0.0) / 1.4) ** 2)
        tendon = y_c - k * 1.6 * ((u - 0.05) ** 2 + ((w - 0.25) / 1.2) ** 2)
        floor = np.interp(w, [-1.0, 0.0, 1.0], [self.level("L1", 0.8), self.level("T12", 0.2), self.level("T10", 0.2)])
        self.y_dia = np.maximum(np.maximum(dome_r, dome_l), np.maximum(tendon, floor))
        # The spine and the posterior gutters bound both cavities behind.
        zvb = self.spine_z(y)
        ax = self.vert_half_width(y)
        az = self.vert_half_depth(y)
        dx = np.abs(g.X - self.mid)
        behind_spine = np.where(dx < ax + 0.006, g.Z < zvb + az + 0.004, g.Z < zvb - 0.012)
        top = self.rib_y(1, self.theta()) + 0.018
        self.thorax_cav = interior & (y >= self.y_dia + h) & (y <= top) & ~behind_spine & (y > self.y_TL - 0.03)
        self.diaphragm = interior & (y >= self.y_dia) & (y < self.y_dia + h) & (y < self.y_CT) & ~behind_spine
        # Abdomen and pelvis: below the diaphragm, inside the abdominal wall (the rib layer depth),
        # in front of the spine/psoas/quadratus, above the pelvic floor and inside the pelvic walls.
        y_floor = self.fh_y - 0.045 + 0.45 * dx
        pelvis_hw = np.interp(y, [self.fh_y - 0.05, self.fh_y, self.level("L5"), self.level("L4")], [0.045, 0.07, 0.105, 0.2])
        post_limit = np.where(dx < ax + 0.004, g.Z > zvb + az + 0.004,
                              np.where(dx < 0.10, g.Z > zvb - 0.010, g.Z > self.prof(self.zb, y) + 0.05))
        sacrum_front = np.where(y < self.y_LS, g.Z > self.sacrum_front_z(y) + 0.004, True)
        self.abd_cav = (abd_interior & (y < self.y_dia) & (y > y_floor) & (dx < pelvis_hw) & post_limit & sacrum_front
                        & (y > self.crotch_y + 0.01))
        self.L[self.diaphragm] = self.ID["muscle"]
        log(f"cavities: thorax {int(self.thorax_cav.sum())} abdomen {int(self.abd_cav.sum())} voxels {self.elapsed()}")

    def vert_half_width(self, y):
        return np.interp(y, [self.y_LS, self.y_TL, self.y_CT, self.y_CT + 0.001, self.y_C1],
                         [0.021, 0.018, 0.012, 0.010, 0.009])

    def vert_half_depth(self, y):
        return np.interp(y, [self.y_LS, self.y_TL, self.y_CT, self.y_CT + 0.001, self.y_C1],
                         [0.015, 0.014, 0.010, 0.008, 0.007])

    def sacrum_front_z(self, y):
        t = np.clip((self.y_LS - y) / 0.11, 0.0, 1.0)
        return self.spine_z(y) + np.interp(t, [0.0, 1.0], [0.017, 0.006])

    def theta(self):
        """Angle around the trunk axis in the normalized section: 0 behind, pi in front."""
        return np.arctan2(np.abs(self.u), -self.w)

    def rib_y(self, i, theta):
        """Height of rib i (1..12) at angle theta: from its vertebra behind, falling toward the front
        (authored drop: three to four vertebral levels for the true ribs)."""
        post = self.level(f"T{i}", 0.75)
        drop = {1: 0.034}.get(i, 0.050 + 0.0035 * min(i, 7))
        if i >= 8:
            drop = 0.062
        return post - drop * smoothstep(0.15 * math.pi, math.pi, theta) - 0.006 * smoothstep(0.0, 0.3 * math.pi, theta)

    # -- abdominal organs ----------------------------------------------------------------------
    def target_voxels(self, mass_kg_parts):
        """Voxel count of the reference mass scaled to this figure (k = figure volume / reference)."""
        vol = sum(m / rho for m, rho in mass_kg_parts) * self.k
        return int(round(vol / self.h ** 3))

    def organ_tube(self, name, pts_uyw, radii, allowed, target, extra_scale_radius=True):
        pts = np.array([self.body_point(*p) for p in pts_uyw])
        radii = np.asarray(radii, dtype=np.float64)

        def make(s):
            return largest_component(tube(self.g, pts, radii * s) & allowed)
        s, n, m = tune(make, target, lo=0.3, hi=3.5)
        self.tuned[name] = {"scale": round(s, 4), "voxels": n, "target_voxels": target}
        return m, pts

    def organs_abdomen(self):
        g, h = self.g, self.h
        cav = self.abd_cav.copy()
        taken = np.zeros(g.shape, dtype=bool)
        # Psoas major (muscle) runs inside the posterior abdomen: carve it out first.
        for s, side in (("L", LEFT), ("R", -LEFT)):
            pts = []
            for name, dx, dz in (("T12", 0.022, 0.0), ("L2", 0.032, 0.004), ("L4", 0.045, 0.012), ("L5", 0.052, 0.018)):
                y = self.level(name)
                pts.append([self.mid + side * dx, y, float(self.spine_z(y)) + dz])
            fh = self.FH[s]
            pts.append([fh[0] - side * 0.012, fh[1] + 0.035, fh[2] + 0.018])
            pts.append([fh[0] - side * 0.004, fh[1] - 0.045, fh[2] + 0.004])
            psoas = tube(g, pts, [0.012, 0.018, 0.022, 0.022, 0.02, 0.012]) & self.inside & ~self.skin & ~self.subcut
            self.L[psoas] = self.ID["muscle"]
            cav &= ~psoas
        # The abdominal aorta and vena cava are reserved first so no organ takes their voxels.
        aorta, ivc = self.abd_vessel_paths()
        reserve = tube(g, aorta, 0.011) | tube(g, ivc, 0.011) | raster_line(g, aorta) | raster_line(g, ivc)
        cav &= ~reserve
        self.abd_cav_free = cav
        ID = self.ID
        # Kidneys: retroperitoneal ellipsoids beside the spine, the right one lower (authored).
        kid_target = self.target_voxels([(0.31, 1066)]) // 2
        kidneys = np.zeros(g.shape, dtype=bool)
        for s, side, ylev in (("L", LEFT, self.level("L1", 0.6)), ("R", -LEFT, self.level("L1", 0.1))):
            zvb = float(self.spine_z(ylev))
            centre = np.array([self.mid + side * 0.052, ylev, zvb + 0.004])
            axes = frame_from([-side * 0.22, 1.0, -0.20], [0.0, 0.0, 1.0])

            def make(sc, centre=centre, axes=axes):
                return ellipsoid(g, centre, axes, [0.050 * sc, 0.027 * sc, 0.019 * sc]) & cav & ~kidneys
            sc, n, m = tune(make, kid_target)
            self.tuned[f"kidney_{s}"] = {"scale": round(sc, 4), "voxels": n, "target_voxels": kid_target}
            kidneys |= m
        self.L[kidneys] = ID["kidney"]
        taken |= kidneys
        # Spleen: left, behind and beside the stomach under the left dome (authored).
        sp_target = self.target_voxels([(0.15, 1089)])
        yl = self.level("T11", 0.3)
        centre = self.body_point(0.62, yl, -0.42)
        axes = frame_from([0.25, -0.45, 0.85], [0.0, 1.0, 0.0])

        def make_sp(sc):
            return ellipsoid(g, centre, axes, [0.050 * sc, 0.032 * sc, 0.018 * sc]) & cav & ~taken
        sc, n, spleen = tune(make_sp, sp_target)
        self.tuned["spleen"] = {"scale": round(sc, 4), "voxels": n, "target_voxels": sp_target}
        self.L[spleen] = ID["spleen"]
        taken |= spleen
        # Pancreas: head in the duodenal curve, body across the spine, tail to the spleen.
        Lv = self.level
        pa_target = self.target_voxels([(0.14, 1087)])
        pa_pts = [(-0.22, Lv("L2", 0.6), -0.05), (-0.05, Lv("L1", 0.4), -0.20), (0.18, Lv("L1", 0.7), -0.30),
                  (0.40, Lv("T12", 0.6), -0.38), (0.55, Lv("T12", 0.9), -0.42)]
        pancreas, _ = self.organ_tube("pancreas", pa_pts, [0.016, 0.011, 0.010, 0.009, 0.008], cav & ~taken, pa_target)
        self.L[pancreas] = ID["pancreas"]
        taken |= pancreas
        # Stomach: a J along the left upper quadrant (cardia, fundus, body, antrum, pylorus).
        st_target = self.target_voxels([(0.15, 1088), (0.25, 1040)])
        Lv = self.level
        st_pts = [(0.18, Lv("T11", 0.2), 0.15), (0.34, Lv("T10", 0.6), -0.05), (0.44, Lv("T12", 0.5), 0.18),
                  (0.38, Lv("L1", 0.6), 0.38), (0.18, Lv("L1", 0.1), 0.46), (-0.04, Lv("L1", 0.8), 0.42),
                  (-0.16, Lv("L1", 0.9), 0.25)]
        stomach, self.stomach_path = self.organ_tube("stomach", st_pts, [0.012, 0.032, 0.032, 0.028, 0.022, 0.017, 0.011],
                                                     cav & ~taken, st_target)
        st_wall = shell6(stomach)
        self.L[stomach] = ID["stomach_contents"]
        self.L[st_wall] = ID["stomach_wall"]
        taken |= stomach
        # Colon: caecum, ascending, transverse (sagging), descending, sigmoid, rectum (authored path).
        # Placed before the region-filled organs so its frame stays one piece; the liver and the
        # small intestine then fill around it.
        Lv = self.level
        co_target = self.target_voxels([(0.37, 1088), (0.30, 1040)])
        sac = lambda y: (float(self.sacrum_front_z(y)) - float(self.zc(y))) / float(self.hd(y)) + 0.14
        co_pts = [(-0.55, self.y_LS + 0.010, 0.05), (-0.66, Lv("L4"), -0.18), (-0.64, Lv("L2"), -0.22),
                  (-0.52, Lv("L1", 0.2), -0.05), (-0.25, Lv("L2", 0.1), 0.40), (0.02, Lv("L3", 0.4), 0.50),
                  (0.30, Lv("L3", 0.6), 0.36), (0.64, Lv("L2", 0.6), 0.02), (0.74, Lv("L1", 0.6), -0.28), (0.72, Lv("L3"), -0.32),
                  (0.64, Lv("L4", 0.2), -0.25), (0.50, self.y_LS - 0.005, -0.05), (0.20, self.y_LS - 0.030, 0.10),
                  (0.03, self.y_LS - 0.050, sac(self.y_LS - 0.05)), (0.0, self.fh_y + 0.005, sac(self.fh_y + 0.005)),
                  (0.0, self.fh_y - 0.035, sac(self.fh_y - 0.035) - 0.05)]
        co_r = [0.020, 0.018, 0.016, 0.016, 0.015, 0.015, 0.015, 0.015, 0.015, 0.014, 0.014, 0.013, 0.013, 0.014, 0.015, 0.010]
        colon, self.colon_path = self.organ_tube("colon", co_pts, co_r, cav & ~taken, co_target)
        self.L[colon] = ID["colon_contents"]
        self.L[shell6(colon)] = ID["colon_wall"]
        taken |= colon
        Lv = self.level
        # Liver: the right upper quadrant under the diaphragm, filled from the dome downward and
        # from the right (ranked fill to the reference volume; its lower edge rises to the left).
        # A ranked fill adapts to what the tube organs left: it takes the space under the right dome,
        # its left lobe reaches across the epigastrium, and its right lobe runs down the flank.
        li_target = self.target_voxels([(1.8, 1079)])
        u, w = self.u, self.w
        y = g.Y
        edge = np.interp(u, [-1.0, -0.5, 0.0, 0.3, 0.6], [Lv("L3", 0.5), Lv("L2", 0.5), Lv("L1", 0.5), Lv("T12", 0.5), Lv("T10")])
        cand = cav & ~taken & (u < 0.62) & (y > edge - 0.04)
        score = -(y - edge) + 0.06 * (u + 1.0) - 0.004 * w
        n = li_target
        for _ in range(8):
            liver = largest_component(ranked(cand, score, n))
            if liver.sum() >= li_target or n >= cand.sum():
                break
            n += int(li_target - liver.sum())
        self.tuned["liver"] = {"voxels": int(liver.sum()), "target_voxels": li_target, "method": "ranked fill (one piece)"}
        self.L[liver] = ID["liver"]
        taken |= liver
        # Urinary bladder: behind the pubic symphysis, low in the pelvis.
        bl_target = self.target_voxels([(0.05, 1086), (0.20, 1024)])
        zsym = float(self.prof(self.zf, self.fh_y)) - 0.035
        centre = np.array([self.mid, self.fh_y + 0.012, zsym - 0.030])
        axes = np.eye(3)

        def make_bl(sc):
            return largest_component(ellipsoid(g, centre, axes, [0.034 * sc, 0.030 * sc, 0.028 * sc]) & cav & ~taken)
        sc, n, bladder = tune(make_bl, bl_target)
        self.tuned["bladder"] = {"scale": round(sc, 4), "voxels": n, "target_voxels": bl_target}
        self.L[bladder] = ID["urine"]
        self.L[shell6(bladder)] = ID["bladder_wall"]
        taken |= bladder
        # Duodenum (part of the small intestine): C around the pancreatic head.
        du_pts = [(-0.16, Lv("L1", 0.8), 0.25), (-0.30, Lv("L1", 0.6), 0.05), (-0.32, Lv("L2", 0.6), -0.05),
                  (-0.20, Lv("L3", 0.5), -0.15), (0.05, Lv("L3", 0.6), -0.15), (0.14, Lv("L2", 0.5), -0.10)]
        duo = largest_component(tube(g, np.array([self.body_point(*p) for p in du_pts]), 0.011) & cav & ~taken)
        # Small intestine: the infracolic compartment, ranked from the front and centre.
        si_target = self.target_voxels([(0.65, 1030), (0.35, 1040)]) - int(duo.sum())
        cand = cav & ~taken & ~duo & (y < Lv("L1", 0.5)) & (np.abs(u) < 0.70) & (y > self.fh_y - 0.03)
        dj = self.body_point(*du_pts[-1])
        near_dj = np.sqrt((g.X - dj[0]) ** 2 + (g.Y - dj[1]) ** 2 + (g.Z - dj[2]) ** 2)
        score = -0.6 * w + 0.5 * np.abs(u) + 1.5 * np.abs(y - Lv("L4", 0.5)) - 0.6 * np.clip(1.0 - near_dj / 0.05, 0.0, 1.0)
        # the largest connected part of the ranked fill, grown until it alone reaches the target
        n = max(si_target, 0)
        for _ in range(8):
            si_main = largest_component(ranked(cand, score, n))
            if si_main.sum() >= si_target or n >= cand.sum():
                break
            n += int(si_target - si_main.sum())
        si = si_main | duo
        self.tuned["small_intestine"] = {"voxels": int(si.sum()), "target_voxels": si_target + int(duo.sum()), "method": "ranked fill + duodenum tube"}
        self.L[si] = ID["small_intestine"]
        taken |= si
        self.abd_taken = taken
        rest = cav & ~taken
        self.L[rest] = ID["visceral_fat"]
        log(f"abdominal organs {self.elapsed()}")

    # -- thoracic organs -----------------------------------------------------------------------
    def organs_thorax(self):
        g, h, ID, Lv = self.g, self.h, self.ID, self.level
        cav = self.thorax_cav
        y = g.Y
        # Heart: an ellipsoid on the central tendon, two thirds left of the midline, its long axis
        # from the base (right, back, up) to the apex (left, front, down) (authored).
        he_target = self.target_voxels([(0.33, 1081), (0.51, 1050)])
        yc = Lv("T7", 0.2)
        centre = np.array([self.mid + 0.020, yc, float(self.zc(yc)) + 0.10 * float(self.hd(yc))])
        axes = frame_from([0.62, -0.38, 0.55], [0.0, 1.0, 0.0])
        # the heart stays in front of the vertebral bodies, the oesophagus and the descending aorta
        in_front = g.Z > self.spine_z(y) + self.vert_half_depth(y) + 0.014
        # a flat chest leaves the heart little depth; it may not spread wider than 14 cm or rise
        # above mid-T5, where the great vessels and the carina are (authored bounds)
        in_front &= (np.abs(g.X - (self.mid + 0.015)) < 0.070) & (y < Lv("T5", 0.45))

        def make(sc):
            return ellipsoid(g, centre, axes, [0.056 * sc, 0.045 * sc, 0.032 * sc]) & cav & in_front
        sc, n, heart = tune(make, he_target)
        self.tuned["heart"] = {"scale": round(sc, 4), "voxels": n, "target_voxels": he_target}
        septum_n = norm([0.75, 0.10, -0.65])
        P = np.stack([g.X, g.Y, g.Z], axis=-1) - centre
        septum = heart & (np.abs(P @ septum_n) < 0.5 * h)
        wall = shell6(heart) | septum
        self.L[heart] = ID["heart_blood"]
        self.L[wall & heart] = ID["heart_wall"]
        self.heart = heart
        self.heart_centre = centre
        self.heart_axes = axes
        # Mediastinum: a midline band from the inlet to the diaphragm, wider around the heart.
        x_med = self.mid + np.interp(y, [Lv("T8"), Lv("T4"), Lv("T1")], [0.010, 0.004, 0.0])
        half = np.interp(y, [Lv("T9"), Lv("T5"), Lv("T3"), Lv("T1")], [0.026, 0.022, 0.020, 0.018])
        med = cav & (np.abs(g.X - x_med) < half) & ~heart
        self.mediastinum = med
        self.L[med] = ID["visceral_fat"]
        lungs = largest_component(cav & ~med & ~heart, keep=2)
        self.L[cav & ~med & ~heart & ~lungs] = ID["visceral_fat"]
        self.L[lungs] = ID["lung"]
        self.lungs = lungs
        # the pleura is walled by muscle where the apex meets the fat of the neck root
        # (scalenes and suprapleural membrane, one voxel)
        wall = dilate6(lungs) & (self.L == ID["subcutaneous_fat"])
        self.L[wall] = ID["muscle"]
        log(f"thoracic organs {self.elapsed()}")

    # -- skeleton ------------------------------------------------------------------------------
    def bone_paint(self, name, mask, *, core=None, cortical_only=False, spongy=False, hard=True):
        """Paint a bone: cortical shell and a spongiosa (or marrow) core where the bone is thick
        enough to have one at this voxel size. `spongy`: a cancellous bone whose cortex is thinner
        than a voxel (vertebral bodies, sacrum, epiphyses, tarsals) is all spongiosa (named)."""
        inside = mask & self.inside & ~self.skin
        self.clipped[name] = self.clipped.get(name, 0) + int((mask & (~self.inside | self.skin)).sum())
        if hard:
            self.hard_bone |= inside
        if cortical_only:
            self.L[inside] = self.ID["cortical_bone"]
            return inside
        if spongy:
            self.L[inside] = self.ID["spongiosa"]
            return inside
        shell = shell6(inside)
        self.L[inside] = self.ID["spongiosa"]
        self.L[shell] = self.ID["cortical_bone"]
        if core is not None:
            self.L[core & inside & ~shell] = self.ID["yellow_marrow"]
        return inside

    def long_bone(self, name, a, b, r_shaft, ends=()):
        """Shaft: cortical tube with a yellow-marrow core along its axis; ends: spongiosa with a
        cortical shell."""
        g = self.g
        a, b = np.asarray(a), np.asarray(b)
        shaft = tube(g, [a, b], r_shaft)
        axis_line = raster_line(g, [a + 0.18 * (b - a), a + 0.82 * (b - a)])
        out = shaft.copy()
        epi = np.zeros(g.shape, dtype=bool)
        for (c, axes, radii) in ends:
            epi |= ellipsoid(g, c, axes, radii)
        inside = (shaft | epi) & self.inside & ~self.skin
        self.clipped[name] = int(((shaft | epi) & (~self.inside | self.skin)).sum())
        self.hard_bone |= inside
        # epiphyses: cancellous bone under a cortex thinner than a voxel (named)
        self.L[inside & epi] = self.ID["spongiosa"]
        shaft_only = inside & ~epi
        self.L[shaft_only] = self.ID["cortical_bone"]
        self.L[shaft_only & axis_line] = self.ID["yellow_marrow"]
        del out
        return inside

    def skeleton(self):
        g, h, J, Lv, ID = self.g, self.h, self.J, self.level, self.ID
        self.hard_bone = np.zeros(g.shape, dtype=bool)
        y = g.Y
        dx = g.X - self.mid
        theta = self.theta()
        zvb = self.spine_z(y)
        # ---- vertebral column: bodies (spongiosa + cortical), discs (cartilage), arches,
        # spinous and transverse processes (cortical), the canal left open for cord / CSF.
        column = np.zeros(g.shape, dtype=bool)
        self.canal = np.zeros(g.shape, dtype=bool)
        for name, (lo, hi, disc) in self.levels.items():
            band = (y >= lo) & (y < hi)
            body_band = (y >= lo + 0.5 * disc) & (y < hi - 0.5 * disc)
            yc = 0.5 * (lo + hi)
            ax = float(self.vert_half_width(yc))
            az = float(self.vert_half_depth(yc))
            zc = float(self.spine_z(yc))
            body = band & ((dx / ax) ** 2 + ((g.Z - zc) / az) ** 2 <= 1.0)
            rc = 0.0065 if name.startswith("C") else (0.0055 if name.startswith("T") else 0.0075)
            zcanal = zc - az - rc - 0.002
            rr = np.sqrt(dx ** 2 + (g.Z - zcanal) ** 2)
            canal = band & (rr < rc)
            arch = body_band & (rr >= rc) & (rr < rc + 0.0075) & (g.Z < zc)
            sp_len = 0.030 if name.startswith("T") else (0.026 if name.startswith("L") else 0.020)
            tip_drop = 0.012 if name.startswith("T") else 0.002
            a = np.array([self.mid, yc, zcanal - rc])
            back = float(self.prof(self.zb, yc)) + 0.016
            b = np.array([self.mid, yc - tip_drop, max(back, zcanal - rc - sp_len)])
            spinous = tube(g, [a, b], 0.0055) & body_band
            tp_len = 0.022 if name.startswith("C") else (0.026 if name.startswith("T") else 0.030)
            tps = np.zeros(g.shape, dtype=bool)
            for side in (-1.0, 1.0):
                tps |= tube(g, [[self.mid + side * rc, yc, zcanal + 0.002], [self.mid + side * tp_len, yc, zcanal - 0.004]], 0.005) & body_band
            disc_mask = band & ~body_band & ((dx / ax) ** 2 + ((g.Z - zc) / az) ** 2 <= 1.0)
            self.bone_paint(f"vertebra_{name}", body & body_band, spongy=True)
            self.L[disc_mask & self.inside & ~self.skin] = ID["cartilage"]
            self.bone_paint(f"vertebra_{name}_posterior", (arch | spinous | tps) & ~canal, cortical_only=True)
            column |= body | arch | spinous | tps
            self.canal |= canal
        # ---- sacrum and coccyx: a tapering, backward-curving plate.
        t = np.clip((self.y_LS - y) / 0.11, 0.0, 1.0)
        in_sac = (y < self.y_LS) & (y > self.y_LS - 0.11)
        hw_s = np.interp(t, [0.0, 0.45, 1.0], [0.050, 0.035, 0.010])
        ht_s = np.interp(t, [0.0, 1.0], [0.017, 0.006])
        zs = self.spine_z(y)
        sacrum = in_sac & ((dx / hw_s) ** 2 + ((g.Z - zs) / ht_s) ** 2 <= 1.0)
        self.bone_paint("sacrum", sacrum, spongy=True)
        self.sac_canal = in_sac & (t < 0.6) & (dx ** 2 + (g.Z - (zs - 0.002)) ** 2 < 0.005 ** 2)
        # ---- pelvis: iliac wings (curved plates from the sacroiliac joints round to the anterior
        # superior spines), acetabula round the femoral heads, pubic and ischial rami (authored).
        crest = Lv("L4", 0.6)
        pelvis = np.zeros(g.shape, dtype=bool)
        pelvis_bulk = np.zeros(g.shape, dtype=bool)
        for s, side in (("L", LEFT), ("R", -LEFT)):
            fh = self.FH[s]
            low = fh[1] + 0.022
            for j in range(g.dims[1]):
                yy = g.ys[j]
                if yy < low or yy > crest:
                    continue
                tt = (yy - low) / (crest - low)
                si = np.array([self.mid + side * 0.046, float(self.sacrum_front_z(min(yy, self.y_LS - 0.01))) - 0.004])
                asis = np.array([self.mid + side * 0.112, float(self.prof(self.zf, yy)) - 0.030])
                acet = np.array([fh[0] + side * 0.004, fh[2] + 0.004])
                p0 = si if tt > 0.35 else (si * (tt / 0.35) + np.array([fh[0] - side * 0.010, fh[2] - 0.030]) * (1 - tt / 0.35))
                p2 = asis * tt + acet * (1 - tt)
                p1 = np.array([self.mid + side * (0.105 + 0.03 * tt), 0.5 * (p0[1] + p2[1]) - 0.025 - 0.01 * tt])
                ts = np.linspace(0.0, 1.0, 40)[:, None]
                curve = (1 - ts) ** 2 * p0 + 2 * (1 - ts) * ts * p1 + ts ** 2 * p2
                xz = np.stack([g.X[j], g.Z[j]], axis=-1)
                d = np.sqrt(((xz[..., None, :] - curve[None, None]) ** 2).sum(-1)).min(-1)
                pelvis[j] |= d < 0.0062
            # acetabulum: a cup above and inside the femoral head
            P = np.stack([g.X, g.Y, g.Z], axis=-1) - fh
            r = np.linalg.norm(P, axis=-1)
            opening = norm([side * 0.8, -0.5, 0.3])
            cup = (r > 0.021) & (r < 0.032) & ((P @ opening) < 0.25 * r)
            pelvis_bulk |= cup
            sym = np.array([self.mid + side * 0.008, fh[1] - 0.008, float(self.prof(self.zf, fh[1])) - 0.034])
            it = np.array([fh[0] - side * 0.010, fh[1] - 0.052, fh[2] - 0.026])
            pelvis |= tube(g, [fh + np.array([-side * 0.020, 0.004, 0.022]), sym], 0.0075)
            pelvis |= tube(g, [sym + np.array([0, -0.020, -0.004]), it], 0.0072)
            pelvis_bulk |= tube(g, [fh + np.array([0.0, -0.022, -0.016]), it], 0.0095)
            pelvis_bulk |= ellipsoid(g, it, np.eye(3), [0.012, 0.010, 0.012])
        self.bone_paint("pelvis", pelvis & ~pelvis_bulk & ~column & ~sacrum, cortical_only=True, hard=False)
        self.bone_paint("pelvis", pelvis_bulk & ~column & ~sacrum, spongy=True)
        # ---- ribs, costal cartilage, sternum.
        band = self.rib_band & (y < self.y_CT + 0.01) & (y > Lv("L1", 0.8))
        ribs = np.zeros(g.shape, dtype=bool)
        cart = np.zeros(g.shape, dtype=bool)
        ends = {1: (1.0, 0.86), 2: (1.0, 0.82), 3: (1.0, 0.80), 4: (1.0, 0.78), 5: (1.0, 0.76), 6: (1.0, 0.72),
                7: (1.0, 0.68), 8: (0.86, 0.66), 9: (0.80, 0.64), 10: (0.74, 0.62), 11: (0.56, 0.56), 12: (0.42, 0.42)}
        for i in range(1, 13):
            yr = self.rib_y(i, theta)
            in_rib = band & (np.abs(y - yr) < 0.55 * h) & (theta <= ends[i][0] * math.pi)
            if i <= 2:
                in_rib |= band & (np.abs(y - yr) < 0.80 * h) & (theta <= ends[i][0] * math.pi) & (theta > 0.25 * math.pi)
            cartilage = in_rib & (theta > ends[i][1] * math.pi)
            ribs |= in_rib & ~cartilage
            cart |= cartilage
        y_j = Lv("T2", 0.2)
        y_x = Lv("T10", 0.6)
        st_hw = np.interp(y, [y_x, Lv("T9"), Lv("T5"), Lv("T4"), y_j], [0.007, 0.012, 0.015, 0.020, 0.024])
        sternum = band & (theta > 0.8 * math.pi) & (np.abs(dx) < st_hw) & (y > y_x) & (y < y_j)
        # between the ribs: intercostal muscle (so the pleural cavity is always walled by rib or
        # muscle, even where an arm lies against the chest closer than a voxel)
        cage_rows = (y < self.rib_y(1, theta) + 0.012) & (y > self.rib_y(12, theta) - 0.012)
        self.L[band & cage_rows & self.inside & ~self.skin] = ID["muscle"]
        self.bone_paint("ribs", ribs & ~sternum, cortical_only=True, hard=False)
        self.L[cart & ~sternum & self.inside & ~self.skin] = ID["cartilage"]
        self.bone_paint("sternum", sternum, spongy=True, hard=False)
        # ---- clavicles and scapulae.
        for s, side in (("L", LEFT), ("R", -LEFT)):
            gh = J[f"{s}_Arm"]
            sc_j = np.array([self.mid + side * 0.022, y_j + 0.006, float(self.prof(self.zf, y_j)) - 0.020])
            ac = np.array([gh[0] - side * 0.004, gh[1] + 0.026, gh[2] + 0.004])
            midc = 0.5 * (sc_j + ac) + np.array([0.0, -0.004, 0.010])
            self.bone_paint(f"clavicle_{s}", tube(g, [sc_j, midc, ac], 0.0065) | raster_line(g, [sc_j, midc, ac]), cortical_only=True, hard=False)
            # scapula: a plate on the back of the ribs, in the muscle just outside the rib layer
            top = np.array([self.mid + side * 0.060, Lv("T2"), 0.0])
            bottom = np.array([self.mid + side * 0.072, Lv("T7", 0.3), 0.0])
            glen = np.array([gh[0] - side * 0.014, gh[1] - 0.004, 0.0])

            def inside_tri(px, py, a=top, b=bottom, c=glen):
                d1 =(b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0])
                d2 = (c[0] - b[0]) * (py - b[1]) - (c[1] - b[1]) * (px - b[0])
                d3 = (a[0] - c[0]) * (py - c[1]) - (a[1] - c[1]) * (px - c[0])
                neg = (d1 < 0) | (d2 < 0) | (d3 < 0)
                pos = (d1 > 0) | (d2 > 0) | (d3 > 0)
                return ~(neg & pos)
            plate = (self.body_piece & (self.depth_body >= self.t_rib - 1.05 * h) & (self.depth_body < self.t_rib)
                     & (self.w < -0.35) & inside_tri(g.X, g.Y))
            spine_sc = tube(g, [[self.mid + side * 0.066, Lv("T3", 0.3), float(self.prof(self.zb, Lv("T3"))) + 0.022],
                                [ac[0], ac[1] - 0.004, ac[2] - 0.012]], 0.0062)
            glenoid = ellipsoid(g, gh + np.array([-side * 0.026, 0.0, 0.0]), np.eye(3), [0.008, 0.016, 0.013])
            self.bone_paint(f"scapula_{s}", plate | spine_sc | glenoid, cortical_only=True, hard=False)
        # ---- limbs.
        for s, side in (("L", LEFT), ("R", -LEFT)):
            gh, el, wr, ha = J[f"{s}_Arm"], J[f"{s}_Elbow"], J[f"{s}_Wrist"], J[f"{s}_Hand"]
            axis = norm(el - gh)
            head = (gh, frame_from(axis, [0, 0, 1]), [0.019, 0.019, 0.019])
            condyles = (el - 0.012 * axis, frame_from([side * 1.0, 0, 0], [0, 1, 0]), [0.024, 0.012, 0.012])
            self.long_bone(f"humerus_{s}", gh + 0.012 * axis, el - 0.020 * axis, 0.0095, ends=(head, condyles))
            # radius (thumb side) and ulna, side by side in the plane of the thumb
            fa = norm(wr - el)
            thumb = J[f"{s}_Thumb_1"] - wr
            o = norm(thumb - (thumb @ fa) * fa) * 0.0075
            self.bone_paint(f"radius_{s}", tube(g, [el + 0.012 * fa + 0.6 * o, wr - 0.004 * fa + o], 0.0058)
                            | raster_line(g, [el + 0.02 * fa + 0.6 * o, wr - 0.01 * fa + o]), cortical_only=True, hard=False)
            self.bone_paint(f"ulna_{s}", tube(g, [el - 0.010 * fa - o, wr - 0.010 * fa - 0.6 * o], 0.0058)
                            | raster_line(g, [el - 0.004 * fa - o, wr - 0.014 * fa - 0.6 * o]), cortical_only=True, hard=False)
            # hand: carpals, metacarpals, phalanges along the rig's finger and thumb chains
            carpals = ellipsoid(g, 0.5 * (wr + ha), frame_from(ha - wr, thumb), [0.012, 0.010, 0.006])
            hand = np.zeros(g.shape, dtype=bool)
            f1, f2, f3 = J[f"{s}_Fingers_1"], J[f"{s}_Fingers_2"], J[f"{s}_Fingers_3"]
            across = norm(np.cross(f2 - f1, np.cross(thumb, f2 - f1)))
            for off in (-0.012, 0.0, 0.012):
                o2 = across * off
                hand |= raster_line(g, [ha + 0.5 * o2, f1 + o2, f2 + o2, f3 + o2])
            t1, t2, t3 = J[f"{s}_Thumb_1"], J[f"{s}_Thumb_2"], J[f"{s}_Thumb_3"]
            hand |= raster_line(g, [wr + 0.3 * (ha - wr), t1, t2, t3])
            self.bone_paint(f"hand_{s}", hand & ~carpals, cortical_only=True, hard=False)
            self.bone_paint(f"hand_{s}", carpals, spongy=True)
            # femur: head (raised to the anatomical hip), neck to the greater trochanter, shaft, condyles
            fh, kn, an = self.FH[s], J[f"{s}_Knee"], J[f"{s}_Ankle"]
            gt = fh + np.array([side * 0.040, -0.026, -0.004])
            leg = norm(kn - gt)
            head = (fh, np.eye(3), [0.020, 0.020, 0.020])
            neck = (0.5 * (fh + gt), frame_from(gt - fh, [0, 0, 1]), [0.026, 0.013, 0.013])
            troch = (gt, np.eye(3), [0.013, 0.016, 0.013])
            cond = (kn + np.array([0, 0.016, -0.004]), np.eye(3), [0.032, 0.020, 0.024])
            self.long_bone(f"femur_{s}", gt - 0.010 * leg, kn + 0.030 * leg * -1.0, 0.0115,
                           ends=(head, neck, troch, cond))
            # patella in front of the knee
            row = g.row(kn[1] + 0.018)
            cols = np.abs(g.xs - kn[0]) < 0.02
            zfront = g.zs[(self.inside[row][:, cols]).any(axis=1)].max() + 0.5 * h
            self.bone_paint(f"patella_{s}", ellipsoid(g, [kn[0], kn[1] + 0.022, zfront - 0.016], np.eye(3), [0.018, 0.020, 0.008]),
                            cortical_only=True, hard=False)
            # tibia (medial, front) and fibula (lateral, back)
            shin = norm(an - kn)
            medial = np.array([-side, 0.0, 0.0])
            plateau = (kn + np.array([0, -0.020, 0]) + 0.004 * medial, np.eye(3), [0.032, 0.016, 0.024])
            malleolus = (an + np.array([0, 0.012, 0]) + 0.010 * medial, np.eye(3), [0.016, 0.018, 0.016])
            self.long_bone(f"tibia_{s}", kn + 0.030 * shin + 0.005 * medial + np.array([0, 0, 0.006]),
                           an - 0.020 * shin + 0.008 * medial, 0.0105, ends=(plateau, malleolus))
            lat = -medial
            self.bone_paint(f"fibula_{s}", tube(g, [kn + 0.040 * shin + 0.020 * lat + np.array([0, 0, -0.010]),
                                                     an + np.array([0, 0.004, -0.008]) + 0.020 * lat], 0.0058)
                            | raster_line(g, [kn + 0.040 * shin + 0.020 * lat + np.array([0, 0, -0.010]),
                                              an + np.array([0, 0.006, -0.008]) + 0.020 * lat]), cortical_only=True, hard=False)
            # foot: talus, calcaneus, metatarsals and toes (lifted above the boot sole)
            ft, he = J[f"{s}_Foot"], J[f"{s}_Heel"]
            talus = ellipsoid(g, an + np.array([0, -0.014, 0.004]), np.eye(3), [0.016, 0.012, 0.020])
            calc = ellipsoid(g, np.array([he[0], an[1] - 0.046, he[2] + 0.020]), frame_from([0, 0.3, 1.0], [0, 1, 0]),
                             [0.032, 0.016, 0.014])
            foot = np.zeros(g.shape, dtype=bool)
            fwd = norm(np.array([ft[0] - an[0], 0.0, ft[2] - an[2]]))
            fl = norm(np.cross([0, 1, 0], fwd))
            for off in (-0.014, 0.0, 0.014):
                start = an + np.array([0, -0.040, 0]) + 0.030 * fwd + off * fl
                ball = ft + np.array([0, 0.016, 0]) + off * fl
                foot |= raster_line(g, [start, ball, ball + 0.030 * fwd + np.array([0, -0.004, 0])])
            self.bone_paint(f"foot_{s}", foot & ~(talus | calc), cortical_only=True, hard=False)
            self.bone_paint(f"foot_{s}", talus | calc, spongy=True)
        self.column = column
        log(f"skeleton {self.elapsed()}")

    # -- head ----------------------------------------------------------------------------------
    def skull_base(self, z):
        e = self.eye
        return np.interp(z - e[2], [-0.25, -0.12, -0.08, -0.05, -0.03, -0.005],
                         [-0.038, -0.038, -0.030, -0.012, 0.004, 0.014]) + e[1]

    def head(self):
        g, h, ID = self.g, self.h, self.ID
        e = self.eye
        y, z = g.Y, g.Z
        dx = g.X - self.mid
        chin_y = self.level("C3", 0.9) - 0.002
        y_sb = self.skull_base(z)
        cranial = self.inside & (y >= y_sb) & (y > self.y_C1 - 0.01)
        face = self.inside & (y < y_sb) & (y > chin_y) & (z > float(self.spine_z(self.y_C1)) + 0.012)
        # skull vault: the voxel layer under the scalp (the owner's offset shell)
        vault = cranial & self.layer2
        self.bone_paint("skull_vault", vault, cortical_only=True)
        # skull base: a plate along the cranial floor, open at the foramen magnum
        foramen = (np.abs(dx) < 0.011) & (np.abs(z - float(self.spine_z(self.y_C1))) < 0.013)
        base = self.inside & ~self.skin & (np.abs(y - y_sb) <= 0.5 * h) & (y > self.y_C1 - 0.02) & ~foramen \
            & (z > float(self.prof(self.zb, e[1])) + 0.005)
        self.bone_paint("skull_base", base, cortical_only=True)
        # brain: the cranial cavity inside the vault and above the base
        brain = cranial & ~self.skin & ~self.layer2 & (y > y_sb + 0.5 * h)
        self.L[brain] = ID["brain"]
        self.brain = brain
        # CSF: the lateral ventricles and the fourth ventricle (rough), inside the brain
        cc = np.array([self.mid, e[1] + 0.035, e[2] - 0.075])
        csf = np.zeros(g.shape, dtype=bool)
        for side in (-1.0, 1.0):
            csf |= ellipsoid(g, cc + np.array([side * 0.011, 0.0, 0.0]), frame_from([0, 0.2, 1.0], [0, 1, 0]), [0.028, 0.009, 0.006])
        csf |= ellipsoid(g, np.array([self.mid, e[1] - 0.012, e[2] - 0.115]), np.eye(3), [0.006, 0.008, 0.006])
        self.L[csf & brain] = ID["csf"]
        # facial skeleton: the layer under the face skin, not under the jaw (submandibular floor)
        z_chin = float(self.prof(self.zf, chin_y + 0.01))
        under_jaw = (y < chin_y + 0.016) & (z < z_chin - 0.026)
        self.bone_paint("facial_skeleton", face & self.layer2 & ~under_jaw, cortical_only=True)
        # teeth: one dental arch (named: teeth as cortical bone)
        ya = e[1] - 0.060
        zf = float(self.prof(self.zf, ya))
        arch = [[self.mid + sx * ax_, ya, zf - dz] for sx, ax_, dz in
                ((-1, 0.022, 0.050), (-1, 0.017, 0.026), (-1, 0.008, 0.014), (0, 0.0, 0.012), (1, 0.008, 0.014),
                 (1, 0.017, 0.026), (1, 0.022, 0.050))]
        self.bone_paint("teeth", raster_line(g, arch), cortical_only=True)
        # eyes: spheres on the rig's eye plane, sized to the reference volume
        ey_target = self.target_voxels([(0.015, 1005)]) // 2
        eyes = np.zeros(g.shape, dtype=bool)
        for side in (-1.0, 1.0):
            c = np.array([self.mid + side * 0.029, e[1], e[2]])

            def make(sc, c=c):
                return ellipsoid(g, c, np.eye(3), [0.0112 * sc] * 3) & self.inside & ~self.skin
            sc, n, m = tune(make, ey_target, lo=0.5, hi=2.0)
            self.tuned["eye_L" if side > 0 else "eye_R"] = {"scale": round(sc, 4), "voxels": n, "target_voxels": ey_target}
            eyes |= m
        self.L[eyes] = ID["eye"]
        # air: nasal cavity, maxillary sinuses, nasopharynx
        zface = float(self.prof(self.zf, e[1] - 0.025))
        nasal = self.inside & ~self.skin & (np.abs(dx) <= 0.006) & (y > e[1] - 0.040) & (y < e[1] - 0.008) \
            & (z < zface - 0.014) & (z > zface - 0.066)
        sinus = np.zeros(g.shape, dtype=bool)
        for side in (-1.0, 1.0):
            sinus |= ellipsoid(g, [self.mid + side * 0.026, e[1] - 0.030, zface - 0.036], np.eye(3), [0.012, 0.012, 0.015])
        self.L[(nasal | sinus) & self.inside & ~self.skin & ~eyes] = ID["airway"]
        self.head_air = (nasal | sinus) & self.inside & ~self.skin & ~eyes
        log(f"head {self.elapsed()}")

    # -- vessels, airway tubes, oesophagus ----------------------------------------------------
    def vessel(self, name, pts, r, key="vessel_blood", where=None):
        """A vessel trunk: a tube of its radius plus a face-connected centre line, so a trunk
        thinner than a voxel stays continuous. It never replaces the long bones, spine or skull,
        the eyes, brain, heart, airway or oesophagus (it may groove a rib or clavicle)."""
        g = self.g
        pts = np.asarray(pts, dtype=np.float64)
        shape = tube(g, pts, r) | raster_line(g, pts)
        m = shape & self.inside & ~self.skin
        if where is not None:
            m &= where
        protected = self.hard_bone | np.isin(self.L, [self.ID[k] for k in (
            "eye", "brain", "heart_wall", "heart_blood", "airway", "oesophagus")])
        self.clipped[name] = int((shape & ~self.inside).sum())
        blocked = m & protected
        m &= ~protected
        self.L[m] = self.ID[key]
        sizes = components(m)
        keys = {r["id"]: r["key"] for r in self.table["labels"]}
        by = collections.Counter(("hard bone" if self.hard_bone.ravel()[i] else keys[int(self.L.ravel()[i])])
                                 for i in np.flatnonzero(blocked.ravel()))
        shallow = shape & (~self.inside | self.skin)
        self.vessel_report[name] = {"voxels": int(m.sum()), "pieces": len(sizes), "blocked_voxels": int(blocked.sum()),
                                    "skin_or_outside_voxels": int(shallow.sum()),
                                    "skin_or_outside_y_m": sorted({round(float(v), 3) for v in self.g.Y[shallow]}),
                                    "blocked_by": dict(sorted(by.items())),
                                    "blocked_y_m": sorted({round(float(v), 3) for v in self.g.Y[blocked]})}
        return m

    def vessels_and_airways(self):
        g, J, Lv, ID = self.g, self.J, self.level, self.ID
        self.vessel_report = {}
        H = self.heart_centre
        ax = self.heart_axes
        zfront = lambda yy: float(self.prof(self.zf, yy))
        spz = lambda yy: float(self.spine_z(yy))
        mid = self.mid
        e = self.eye
        # ---- airway: larynx and trachea (air lumen, cartilage wall), main bronchi, pharynx
        larynx = np.array([mid, Lv("C4"), spz(Lv("C4")) + 0.034])
        carina = np.array([mid + 0.002, Lv("T5", 0.3), spz(Lv("T5")) + 0.030])
        t_upper = np.array([mid, Lv("C7"), spz(Lv("C7")) + 0.032])
        t_mid = np.array([mid, Lv("T2"), 0.5 * (zfront(Lv("T2")) - 0.030 + spz(Lv("T2")) + 0.030)])
        trachea_pts = [larynx, t_upper, t_mid, carina]
        wall = tube(g, trachea_pts, 0.0112) & self.inside & ~self.skin
        self.L[wall & ~self.hard_bone] = ID["cartilage"]
        lumen = (tube(g, trachea_pts, 0.0065) | raster_line(g, trachea_pts)) & self.inside
        bro_r = [carina, carina + np.array([-0.022, -0.020, -0.006]), np.array([mid - 0.055, Lv("T6", 0.6), spz(Lv("T6")) + 0.022])]
        bro_l = [carina, carina + np.array([0.024, -0.012, 0.0]), np.array([mid + 0.060, Lv("T6", 0.3), spz(Lv("T6")) + 0.030])]
        lumen |= (raster_line(g, bro_r) | raster_line(g, bro_l)) & (self.lungs | self.mediastinum | wall)
        zface = float(self.prof(self.zf, e[1] - 0.025))
        naso = np.array([mid, e[1] - 0.030, zface - 0.058])
        choana = np.array([mid, e[1] - 0.034, spz(self.y_C1) + 0.022])
        oro = np.array([mid, Lv("C2"), spz(Lv("C2")) + 0.018])
        lumen |= (tube(g, [naso, choana, oro, larynx], 0.0062) | raster_line(g, [naso, choana, oro, larynx])) & self.inside & ~self.skin
        self.L[lumen & ~self.skin] = ID["airway"]
        self.airway = lumen & ~self.skin
        # ---- oesophagus: behind the trachea and to the right of the descending aorta, crossing in
        # front of it to the left to pass the diaphragm and reach the cardia
        cardia = self.stomach_path[0]
        oe = [np.array([mid, Lv("C6"), spz(Lv("C6")) + 0.016]), np.array([mid - 0.002, Lv("T2"), spz(Lv("T2")) + 0.018]),
              np.array([mid - 0.006, Lv("T6"), spz(Lv("T6")) + 0.019]), np.array([mid - 0.002, Lv("T8"), spz(Lv("T8")) + 0.021]),
              np.array([mid + 0.006, Lv("T10", 0.3), spz(Lv("T10")) + 0.042]), cardia]
        oshape = tube(g, oe, 0.0068) | raster_line(g, oe)
        om = oshape & self.inside & ~self.skin & ~self.airway & ~self.hard_bone
        om &= ~np.isin(self.L, [ID[k] for k in ("cartilage", "stomach_wall", "stomach_contents", "heart_wall", "heart_blood")])
        om = largest_component(om)
        self.L[om] = ID["oesophagus"]
        # ---- great vessels (blood; walls unresolved): aorta, venae cavae, pulmonary trunk, neck and limb trunks
        base = H + np.array([-0.004, 0.020, -0.004])
        # the trachea's depth at a height (for routing around it)
        tp = np.array(trachea_pts)
        tz = lambda yy: float(np.interp(yy, tp[::-1, 1], tp[::-1, 2]))
        # the arch rises in front of the trachea and passes back to its left, over the left bronchus
        arch_top = np.array([mid + 0.004, Lv("T3", 0.5), tz(Lv("T3", 0.5)) + 0.026])
        arch_left = np.array([mid + 0.028, Lv("T3", 0.8), tz(Lv("T3", 0.8)) + 0.004])
        arch_back = np.array([mid + 0.028, Lv("T4", 0.6), spz(Lv("T4")) + 0.014])
        aorta_abd, ivc_abd = self.abd_vessel_paths()
        aorta = [base, np.array([mid - 0.002, Lv("T4", 0.3), tz(Lv("T4", 0.3)) + 0.030]), arch_top, arch_left, arch_back,
                 np.array([mid + 0.028, Lv("T6"), spz(Lv("T6")) + 0.008]), np.array([mid + 0.024, Lv("T9"), spz(Lv("T9")) + 0.012])] + aorta_abd
        self.vessel("aorta", aorta, [0.0115, 0.0115, 0.011, 0.0105, 0.0100, 0.0090, 0.0090, 0.0085, 0.0090, 0.0085, 0.008])
        ra = H + np.array([-0.030, 0.000, -0.012])
        ivc = [ra, np.array([mid - 0.022, Lv("T9"), spz(Lv("T9")) + 0.030])] + ivc_abd
        self.vessel("ivc", ivc, 0.0095)
        svc = [ra + np.array([0.004, 0.014, 0.0]), np.array([mid - 0.024, Lv("T3"), float(self.zc(Lv("T3"))) + 0.006]),
               np.array([mid - 0.022, Lv("T1", 0.3), float(self.zc(Lv("T1"))) + 0.006])]
        self.vessel("svc", svc, 0.0085)
        rv = H + 0.8 * 0.03 * ax[0] + np.array([0.0, 0.012, 0.010])
        # the pulmonary trunk divides in front of the carina
        pt_bif = np.array([mid + 0.010, Lv("T5", 0.85), carina[2] + 0.022])
        self.vessel("pulmonary_trunk", [rv, pt_bif], 0.010)
        self.vessel("pulmonary_r", [pt_bif, np.array([mid - 0.050, Lv("T6"), spz(Lv("T6")) + 0.040])], 0.0075)
        self.vessel("pulmonary_l", [pt_bif, np.array([mid + 0.052, Lv("T6", 0.2), spz(Lv("T6")) + 0.038])], 0.0075)
        # ---- neck: carotids and jugulars to the skull base
        skull = lambda side, dxx: np.array([mid + side * dxx, self.y_C1 - 0.004, spz(self.y_C1) + 0.012])
        for side, s in ((LEFT, "L"), (-LEFT, "R")):
            # left: subclavian and carotid from the back of the arch, beside the trachea; right: the
            # brachiocephalic trunk rises in front of the trachea and divides behind the clavicle
            if s == "L":
                origin = arch_left + np.array([0.0, 0.010, -0.004])
                beside = np.array([mid + 0.030, Lv("T1", 0.6), spz(Lv("T1")) + 0.028])
            else:
                origin = arch_top + np.array([-0.012, 0.006, 0.010])
                beside = np.array([mid - 0.026, Lv("T1", 0.6), spz(Lv("T1")) + 0.044])
            neck_low = np.array([mid + side * 0.019, Lv("C7"), spz(Lv("C7")) + 0.024])
            neck_hi = np.array([mid + side * 0.019, Lv("C3"), spz(Lv("C3")) + 0.018])
            self.vessel(f"carotid_{s}", [origin, beside, neck_low, neck_hi, skull(side, 0.019)], 0.0045)
            jl = np.array([mid + side * 0.024, Lv("T1", 0.5), spz(Lv("T1")) + 0.040])
            cross = [np.array([mid, Lv("T1", 0.4), tz(Lv("T1", 0.4)) + 0.024])] if s == "L" else []
            self.vessel(f"jugular_{s}", [svc[-1]] + cross + [jl, np.array([mid + side * 0.022, Lv("C5"), spz(Lv("C5")) + 0.020]),
                                         np.array([mid + side * 0.021, Lv("C3"), spz(Lv("C3")) + 0.014]),
                                         skull(side, 0.024)], 0.0045)
            # arm: subclavian, axillary, brachial, forearm trunk (radial and ulnar as one)
            gh, el, wr = J[f"{s}_Arm"], J[f"{s}_Elbow"], J[f"{s}_Wrist"]
            axis = norm(el - gh)
            medial = np.array([-side, 0.0, 0.0])
            axilla = gh + np.array([0, -0.026, 0.008]) + 0.020 * medial
            under_clav = np.array([mid + side * 0.075, Lv("T2", 0.4), float(self.zc(Lv("T2"))) + 0.010])
            # (the forearm trunk stops a quarter above the wrist: there the arteries are shallower
            # than the skin voxel)
            low = np.array([beside[0], Lv("T2", 0.5), beside[2]])
            self.vessel(f"arm_artery_{s}", [origin, low, under_clav, axilla,
                                            el - 0.03 * axis + 0.018 * medial + np.array([0, 0, 0.014]),
                                            el + 0.010 * medial + np.array([0, 0, 0.006]), wr + 0.25 * (el - wr)], 0.0042)
            # leg: iliac, femoral, popliteal, posterior tibial (artery and vein each one trunk)
            fh, kn, an = self.FH[s], J[f"{s}_Knee"], J[f"{s}_Ankle"]
            bif = aorta[-1]
            brim = np.array([mid + side * 0.050, self.y_LS - 0.010, float(self.zc(self.y_LS)) - 0.004])
            ing = np.array([fh[0] - side * 0.012, fh[1] + 0.016, zfront(fh[1] + 0.016) - 0.030])
            midthigh = 0.5 * (fh + kn) + np.array([-side * 0.020, 0, 0.014])
            hiatus = fh + 0.70 * (kn - fh) + np.array([-side * 0.016, 0, -0.012])
            pop_hi = kn + np.array([0, 0.045, -0.036])
            pop_lo = kn + np.array([0, -0.035, -0.034])
            ptib = 0.5 * (kn + an) + np.array([-side * 0.006, 0, -0.022])
            # (the trunk stops above the ankle, where the vessels pass shallower than a voxel)
            ank = an + 0.18 * (kn - an) + np.array([-side * 0.010, 0.0, -0.016])
            self.vessel(f"leg_artery_{s}", [bif, brim, ing, midthigh, hiatus, pop_hi, pop_lo, ptib, ank],
                        [0.0065, 0.006, 0.0055, 0.005, 0.005, 0.0045, 0.0045, 0.0035, 0.003])
            vbif = ivc[-1]
            self.vessel(f"leg_vein_{s}", [vbif, brim + np.array([-side * 0.014, -0.004, 0.002]), ing + np.array([-side * 0.010, 0, -0.004]),
                                          midthigh + np.array([-side * 0.008, 0, -0.006])], 0.006)
        log(f"vessels and airways {self.elapsed()}")

    def seal_brain(self):
        """Close any gap in the skull: a brain voxel beside anything but brain, CSF, bone or the
        spinal cord becomes skull (cortical bone)."""
        ID = self.ID
        brain = self.L == ID["brain"]
        ok = np.isin(self.L, [ID[k] for k in ("brain", "csf", "cortical_bone", "spongiosa", "spinal_cord")])
        leak = brain & dilate6(~ok)
        self.notes.append(f"skull sealed at {int(leak.sum())} brain voxels")
        self.L[leak] = ID["cortical_bone"]

    def spinal_cord(self):
        g, ID = self.g, self.ID
        mid = self.mid
        pts = []
        for name in [f"C{i}" for i in range(1, 8)] + [f"T{i}" for i in range(1, 13)] + ["L1", "L2"]:
            lo, hi, _ = self.levels[name]
            yc = 0.5 * (lo + hi)
            az = float(self.vert_half_depth(yc))
            rc = 0.0065 if name.startswith("C") else (0.0055 if name.startswith("T") else 0.0075)
            pts.append([mid, yc, float(self.spine_z(yc)) - az - rc - 0.002])
        top = np.array([mid, self.y_C1 + 0.012, pts[0][2] + 0.004])
        pts = [top] + pts
        cord_line = raster_line(g, np.array(pts[:-1]))
        lumbar = raster_line(g, np.array(pts[-2:]))
        self.L[(cord_line | self.canal & (g.Y > self.level("L1", 0.5))) & self.inside & ~self.skin] = ID["spinal_cord"]
        csf = (self.canal | self.sac_canal) & (g.Y <= self.level("L1", 0.5)) & self.inside & ~self.skin
        self.L[(csf | lumbar & (g.Y <= self.level("L1", 0.5))) & self.inside] = ID["csf"]
        # where the figure's back is too thin for the canal to sit two voxels deep, the canal voxel
        # next to the skin becomes lamina (cortical bone)
        open_canal = np.isin(self.L, [ID["csf"], ID["spinal_cord"]]) & dilate6(self.skin) & (g.Y < self.y_C1)
        self.L[open_canal] = ID["cortical_bone"]

    # -- receipt -------------------------------------------------------------------------------
    def summary(self, labels):
        return summarize(labels, self.table, self.h, self.k)


def summarize(labels: np.ndarray, table: dict, h: float, k: float) -> dict:
    rows = {r["id"]: r for r in table["labels"]}
    ID = {r["key"]: r["id"] for r in table["labels"]}
    counts = np.bincount(labels.ravel(), minlength=256)
    v = h ** 3
    per = []
    for i, r in sorted(rows.items()):
        n = int(counts[i])
        per.append({"id": i, "key": r["key"], "voxels": n, "volume_l": round(n * v * 1000, 4),
                    "mass_kg": round(n * v * r["density_kg_m3"], 4) if r["key"] != "exterior" else 0.0,
                    "blood_l": round(n * v * 1000 * r["blood_volume_fraction"], 4) if r["key"] != "exterior" else 0.0})
    mass = {p["key"]: p["mass_kg"] for p in per}
    groups = [
        ("brain", ["brain"], 1.45), ("eyes (both)", ["eye"], 0.015), ("spinal cord", ["spinal_cord"], 0.03),
        ("lungs (with blood)", ["lung"], 1.2), ("heart wall", ["heart_wall"], 0.33), ("heart contents (blood)", ["heart_blood"], 0.51),
        ("heart, wall + contents", ["heart_wall", "heart_blood"], 0.84), ("liver", ["liver"], 1.8), ("kidneys (both)", ["kidney"], 0.31),
        ("spleen", ["spleen"], 0.15), ("pancreas", ["pancreas"], 0.14), ("stomach wall", ["stomach_wall"], 0.15),
        ("stomach contents", ["stomach_contents"], 0.25), ("stomach, wall + contents", ["stomach_wall", "stomach_contents"], 0.40),
        ("small intestine, wall + contents", ["small_intestine"], 1.0), ("colon wall", ["colon_wall"], 0.37),
        ("colon contents", ["colon_contents"], 0.30), ("colon, wall + contents", ["colon_wall", "colon_contents"], 0.67),
        ("urinary bladder wall", ["bladder_wall"], 0.05), ("urine", ["urine"], 0.20), ("bladder, wall + urine", ["bladder_wall", "urine"], 0.25),
        ("oesophagus", ["oesophagus"], 0.04), ("skin", ["skin"], 3.3),
        ("adipose, subcutaneous + visceral (vs separable adipose)", ["subcutaneous_fat", "visceral_fat"], 14.5),
        ("skeletal muscle", ["muscle"], 29.0), ("cortical bone", ["cortical_bone"], 4.4),
        ("spongiosa (trabecular bone + red marrow)", ["spongiosa"], 2.27), ("yellow marrow", ["yellow_marrow"], 2.48),
        ("cartilage", ["cartilage"], 1.1),
        ("skeleton (bone, marrow, cartilage)", ["cortical_bone", "spongiosa", "yellow_marrow", "cartilage"], 10.25),
        ("whole body", [r["key"] for r in table["labels"] if r["key"] != "exterior"], 73.0),
    ]
    organs = []
    for name, keys, ref in groups:
        m = sum(mass[k] for k in keys)
        organs.append({"organ": name, "labels": keys, "voxel_mass_kg": round(m, 4), "icrp89_male_kg": ref,
                       "reference_scaled_kg": round(ref * k, 4), "ratio_to_scaled": round(m / (ref * k), 3) if ref else None})
    blood = sum(p["blood_l"] for p in per)
    return {"labels": per, "organs": organs,
            "blood_l": {"total": round(blood, 3), "explicit_blood_voxels": round(sum(p["blood_l"] for p in per if p["key"] in ("heart_blood", "vessel_blood")), 3),
                        "reference_scaled": round(5.3 * k, 3), "crew_blood_system_authored": 6.0}}


def checks(labels: np.ndarray, table: dict, bone: np.ndarray) -> dict:
    ID = {r["key"]: r["id"] for r in table["labels"]}
    ext = labels == ID["exterior"]
    skin = labels == ID["skin"]
    touching_ext = dilate6(ext, pad=True)
    touching_skin = dilate6(skin)
    out = {}
    out["non_skin_voxels_touching_exterior"] = int((touching_ext & ~ext & ~skin).sum())
    deep = ["brain", "lung", "heart_wall", "heart_blood", "liver", "kidney", "spleen", "pancreas", "stomach_wall",
            "stomach_contents", "small_intestine", "colon_wall", "colon_contents", "bladder_wall", "urine", "spinal_cord",
            "csf", "visceral_fat", "oesophagus"]
    out["organ_voxels_touching_skin"] = {k: int((touching_skin & (labels == ID[k])).sum()) for k in deep}
    bones = ["cortical_bone", "spongiosa", "yellow_marrow"]
    out["bone_voxels_touching_skin"] = int((touching_skin & np.isin(labels, [ID[k] for k in bones])).sum())
    comp = {}
    for k in ["brain", "lung", "heart_wall", "liver", "kidney", "spleen", "pancreas", "stomach_wall", "small_intestine",
              "colon_wall", "bladder_wall", "eye", "spinal_cord", "oesophagus", "airway", "vessel_blood", "csf"]:
        sizes = components(labels == ID[k])
        comp[k] = {"count": len(sizes), "largest": sizes[:4]}
    for name, keys in (("heart (wall + blood)", ["heart_wall", "heart_blood"]), ("stomach (wall + contents)", ["stomach_wall", "stomach_contents"]),
                       ("colon (wall + contents)", ["colon_wall", "colon_contents"]), ("bladder (wall + urine)", ["bladder_wall", "urine"])):
        sizes = components(np.isin(labels, [ID[k] for k in keys]))
        comp[name] = {"count": len(sizes), "largest": sizes[:4]}
    out["components"] = comp
    brain = labels == ID["brain"]
    ok = np.isin(labels, [ID[k] for k in ("brain", "csf", "cortical_bone", "spongiosa", "spinal_cord")])
    out["brain_voxels_beside_non_cranial_tissue"] = int((brain & dilate6(~ok)).sum())
    lung = labels == ID["lung"]
    out["lung_voxels_beside_skin_or_subcutaneous_fat"] = int((lung & dilate6(np.isin(labels, [ID["skin"], ID["subcutaneous_fat"]]))).sum())
    out["voxels_without_bone_binding"] = int(((labels != ID["exterior"]) & (bone == NO_BONE)).sum())
    return out


def build_header(b: Builder, table_sha: str, inputs: dict, rig_err: float, labels_meta: list) -> dict:
    joints = []
    segs = b.segs
    for j in b.joints:
        name, a, c = segs[j["index"]]
        midp = 0.5 * (a + c)
        joints.append({"index": j["index"], "name": j["name"], "parent": j["parent"],
                       "head_m": [round(float(v), 6) for v in j["head"]],
                       "matrix_relaxed": [round(float(v), 8) for v in j["matrix"].T.ravel()],
                       "crew_part_by_game_classifier": CREW_PARTS[int(classify_relaxed(midp[0], midp[1]))]})
    return {
        "format": kvox.FORMAT, "version": kvox.VERSION,
        "dims": b.g.dims, "voxel_size_m": b.h,
        "origin_m": [round(float(v), 9) for v in b.g.origin],
        "layout": kvox.LAYOUT,
        "frame": {
            "name": "relaxed crew figure frame (the frame crew_tissue voxelises and CrewBodyPart::rest_center uses)",
            "units": "metres", "up": "+Y", "forward": "+Z (the face)", "left": "+X is the figure's anatomical left (the rig's L_ bones)",
            "floor": "Y = 0 (boot soles)",
            "game_part_naming": "crew_physics::classify_relaxed_mesh_point names the -X arm and leg 'Left'; anatomically that is the figure's right",
            "source_mesh": inputs["figure"],
        },
        "channels": [
            {"name": "label", "type": "u8", "meaning": "tissue label id (catalog/crew_anatomy_tissues.v1.json)"},
            {"name": "bone", "type": "u8", "meaning": "dominant rig joint: index into rig.joints; 255 outside the body",
             "method": "nearest bone segment of the rig in the relaxed pose, restricted to the bones of the figure piece(s) holding the voxel (named approximation of the skin binding)"},
        ],
        "labels": labels_meta,
        "tissue_table": {"path": "catalog/crew_anatomy_tissues.v1.json", "sha256": table_sha},
        "rig": {"source": inputs["rig"], "pose": "relaxed", "pose_source": inputs["bake_report"],
                "skinning_check_max_error_m": rig_err,
                "matrix_note": "matrix_relaxed is each joint's world matrix in this frame (column-major, includes the rig's uniform 0.0089 cm-to-m scale). A voxel at p binds as p_local = inverse(matrix_relaxed) * p; a posed body maps it with that joint's posed world matrix.",
                "joints": joints},
        "provenance": {"generator": "tools/anatomy/build_anatomy.py", "generator_version": GENERATOR_VERSION,
                       "figure_licence": "CC-BY-4.0, 'Free Tiger And Crew Kit' by AdrielCZ (see the asset's SOURCE.md)",
                       "anatomy": "procedural, authored in this generator; no third-party anatomy model was used"},
        "edits": [],
    }


def compare_receipt(receipt: dict, fresh: dict) -> list[str]:
    problems = []
    for key in ("label_sha256", "bone_sha256", "dims", "voxel_size_m", "origin_m"):
        if receipt["volume"].get(key) != fresh["volume"].get(key):
            problems.append(f"{key}: receipt {receipt['volume'].get(key)} != rebuilt {fresh['volume'].get(key)}")
    return problems


def capture_edits(base_labels: np.ndarray, edited_path: Path, base_sha: str, out: Path, table: dict):
    header, channels = kvox.read(edited_path)
    edited = channels["label"]
    if edited.shape != base_labels.shape:
        raise SystemExit(f"edited volume dims {header['dims']} differ from the base")
    changed = np.flatnonzero(edited.ravel() != base_labels.ravel())
    keys = {r["id"]: r["key"] for r in table["labels"]}
    pairs = collections.Counter((keys.get(int(base_labels.ravel()[i]), str(base_labels.ravel()[i])),
                                 keys.get(int(edited.ravel()[i]), str(edited.ravel()[i]))) for i in changed)
    doc = {"schema": "kessel.crew-anatomy-edits", "version": 1,
           "base_label_sha256": base_sha, "dims": header["dims"],
           "index_order": kvox.LAYOUT,
           "summary": [{"from": a, "to": b, "voxels": n} for (a, b), n in sorted(pairs.items())],
           "edits": [[int(i), int(edited.ravel()[i])] for i in changed]}
    out.write_text(json.dumps(doc, indent=None, separators=(",", ":")) + "\n", encoding="utf-8")
    return doc


def apply_edits(labels: np.ndarray, path: Path, base_sha: str) -> dict:
    doc = json.loads(path.read_text(encoding="utf-8"))
    flat = labels.ravel()
    for i, v in doc["edits"]:
        flat[i] = v
    return {"path": str(path.relative_to(REPO)) if path.is_relative_to(REPO) else str(path), "voxels": len(doc["edits"]),
            "base_matches": doc["base_label_sha256"] == base_sha}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--asset-root", help="directory holding assets/ (default: $KESSEL_ASSET_ROOT, else the repo)")
    ap.add_argument("--voxel-mm", type=float, default=10.0)
    ap.add_argument("--out", help=f"volume path (default: <asset root>/{DEFAULT_OUT})")
    ap.add_argument("--receipt", default=str(RECEIPT), help="tracked receipt JSON (checksums, masses, checks)")
    ap.add_argument("--no-edits", action="store_true", help="do not apply the tracked edit layer")
    ap.add_argument("--verify", action="store_true", help="rebuild and compare with the receipt; write nothing")
    ap.add_argument("--capture-edits", metavar="EDITED_KVOX", help="diff an edited volume against the procedural base and store it as the tracked edit layer")
    ap.add_argument("--report", metavar="KVOX", help="print masses and checks of an existing volume (e.g. one edited by hand)")
    args = ap.parse_args(argv)
    table = json.loads(TABLE.read_text(encoding="utf-8"))
    table_sha = sha256_file(TABLE)
    if args.report:
        header, channels = kvox.read(args.report)
        k = header.get("figure", {}).get("k_volume_ratio", 1.0)
        out = {"summary": summarize(channels["label"], table, header["voxel_size_m"], k),
               "checks": checks(channels["label"], table, channels.get("bone", np.zeros_like(channels["label"])))}
        print(json.dumps(out, indent=1))
        return 0
    root = Path(args.asset_root or os.environ.get("KESSEL_ASSET_ROOT") or REPO)
    pieces, rig, pose, inputs = load_inputs(root)
    joints, rig_err, vertex_joint = relaxed_rig(rig, pose, pieces)
    log(f"relaxed rig reproduces the figure to {rig_err:.2e} m")
    b = Builder(pieces, joints, table, args.voxel_mm / 1000.0)
    labels = b.build(table["reference_body"]["volume_l"] / 1000.0)
    base_sha = kvox.payload_sha256(labels)
    if args.capture_edits:
        doc = capture_edits(labels, Path(args.capture_edits), base_sha, EDITS, table)
        log(f"captured {len(doc['edits'])} edited voxels into {EDITS.relative_to(REPO)}: {doc['summary']}")
        return 0
    edits = None
    if EDITS.exists() and not args.no_edits:
        edits = apply_edits(labels, EDITS, base_sha)
        log(f"applied edit layer: {edits}")
    labels_meta = [{"id": r["id"], "key": r["key"], "name": r["name"], "colour": r["colour"]} for r in table["labels"]]
    header = build_header(b, table_sha, inputs, rig_err, labels_meta)
    header["figure"] = {"inside_voxels": int(b.inside.sum()), "volume_l": round(float(b.inside.sum() * b.h ** 3 * 1000), 3),
                        "reference_body_volume_l": table["reference_body"]["volume_l"], "k_volume_ratio": round(b.k, 5),
                        "linear_scale_to_reference": round(b.k ** (-1.0 / 3.0), 5)}
    header["landmarks"] = {"midline_x_m": round(b.mid, 5), "crotch_y_m": round(b.crotch_y, 4),
                           "vertebral_levels_m": {k2: [round(v[0], 4), round(v[1], 4)] for k2, v in b.levels.items()},
                           "femoral_heads_m": {s: [round(float(x), 4) for x in v] for s, v in b.FH.items()}}
    header["base_label_sha256"] = base_sha
    if edits:
        header["edits"].append({"tool": "build_anatomy.py edit layer", **edits})
    data = kvox.encode(header, {"label": labels, "bone": b.bone})
    summary = b.summary(labels)
    chk = checks(labels, table, b.bone)
    # binding agreement with the figure's own skin weights, at the skin voxels
    agree = binding_agreement(b, vertex_joint)
    receipt = {
        "schema": "kessel.crew-anatomy-volume-receipt", "version": 1,
        "volume": {"path": (args.out or DEFAULT_OUT), "bytes": len(data), "file_sha256": sha256_bytes(data),
                   "dims": b.g.dims, "voxel_size_m": b.h, "origin_m": header["origin_m"],
                   "label_sha256": kvox.payload_sha256(labels), "bone_sha256": kvox.payload_sha256(b.bone),
                   "base_label_sha256": base_sha},
        "inputs": {**inputs, "tissue_table": {"path": "catalog/crew_anatomy_tissues.v1.json", "sha256": table_sha},
                   "generator": {"path": "tools/anatomy/build_anatomy.py", "sha256": sha256_file(Path(__file__))}},
        "rig_check": {"max_skinning_error_m": rig_err},
        "figure": header["figure"],
        "landmarks": header["landmarks"],
        "tuning": b.tuned,
        "vessels": b.vessel_report,
        "summary": summary,
        "checks": {**chk, "shape_voxels_clipped_by_the_body_or_skin": {k2: v for k2, v in sorted(b.clipped.items()) if v},
                   "bone_binding_vs_skin_weights": agree},
        "notes": b.notes,
        "edit_layer": edits,
    }
    if args.verify:
        old = json.loads(Path(args.receipt).read_text(encoding="utf-8"))
        problems = compare_receipt(old, receipt)
        if problems:
            for p in problems:
                log("MISMATCH", p)
            return 1
        log("rebuild matches the receipt")
        return 0
    out = Path(args.out) if args.out else root / DEFAULT_OUT
    out.parent.mkdir(parents=True, exist_ok=True)
    tmp = out.with_name(out.name + ".partial")
    tmp.write_bytes(data)
    tmp.replace(out)
    Path(args.receipt).write_text(json.dumps(receipt, indent=1) + "\n", encoding="utf-8")
    log(f"wrote {out} ({len(data)} bytes) and {args.receipt} {b.elapsed()}")
    for o in summary["organs"]:
        log(f"  {o['organ']:58s} {o['voxel_mass_kg']:8.3f} kg  ref {o['icrp89_male_kg']:6.3f} x k = {o['reference_scaled_kg']:7.3f}  ratio {o['ratio_to_scaled']}")
    return 0


def binding_agreement(b: Builder, vertex_joint: dict) -> dict:
    """At skin voxels: how often the voxel's bound bone is the dominant skin-weight joint of the
    nearest figure vertex."""
    g = b.g
    verts = np.concatenate([p.positions for p in b.pieces])
    vj = np.concatenate([vertex_joint[p.name] for p in b.pieces])
    sel = np.flatnonzero(b.skin.ravel())
    P = g.all_points()[sel]
    nearest = np.empty(len(P), dtype=np.int64)
    for s in range(0, len(P), 2048):
        d2 = ((P[s:s + 2048, None, :] - verts[None]) ** 2).sum(-1)
        nearest[s:s + 2048] = d2.argmin(1)
    same = (b.bone.ravel()[sel] == vj[nearest])
    return {"skin_voxels": int(len(sel)), "same_dominant_joint": round(float(same.mean()), 4)}


if __name__ == "__main__":
    sys.exit(main())
