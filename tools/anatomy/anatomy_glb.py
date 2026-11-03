"""Minimal, dependency-free (numpy only) glTF 2.0 binary reader for the crew anatomy tools.

Reads only data: the JSON chunk, the BIN chunk and accessors. It never evaluates anything found in
a file, so it is safe to run (with `python3 -I`) on asset files of unconfirmed origin.

Supports what the crew figure and its authoring rig use: node TRS/matrix hierarchies, indexed
triangle primitives, POSITION / JOINTS_0 / WEIGHTS_0 attributes and one skin.
"""

from __future__ import annotations

import json
import struct
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np

GLB_MAGIC = 0x46546C67
CHUNK_JSON = 0x4E4F534A
CHUNK_BIN = 0x004E4942
COMPONENT = {5120: np.int8, 5121: np.uint8, 5122: np.int16, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}
WIDTH = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT2": 4, "MAT3": 9, "MAT4": 16}


@dataclass
class Gltf:
    doc: dict
    bin: bytes
    path: str = ""
    _world: dict = field(default_factory=dict)

    @classmethod
    def load(cls, path: str | Path) -> "Gltf":
        data = Path(path).read_bytes()
        magic, version, length = struct.unpack_from("<III", data, 0)
        if magic != GLB_MAGIC or version != 2 or length > len(data):
            raise ValueError(f"{path}: not a glTF 2.0 binary")
        offset = 12
        doc = None
        blob = b""
        while offset + 8 <= length:
            chunk_length, chunk_type = struct.unpack_from("<II", data, offset)
            body = data[offset + 8: offset + 8 + chunk_length]
            if chunk_type == CHUNK_JSON:
                doc = json.loads(body.decode("utf-8"))
            elif chunk_type == CHUNK_BIN:
                blob = body
            offset += 8 + chunk_length
        if doc is None:
            raise ValueError(f"{path}: no JSON chunk")
        return cls(doc=doc, bin=blob, path=str(path))

    def accessor(self, index: int) -> np.ndarray:
        acc = self.doc["accessors"][index]
        dtype = np.dtype(COMPONENT[acc["componentType"]])
        width = WIDTH[acc["type"]]
        count = acc["count"]
        view = self.doc["bufferViews"][acc["bufferView"]]
        if view.get("buffer", 0) != 0:
            raise ValueError("only the GLB's own buffer is supported")
        start = view.get("byteOffset", 0) + acc.get("byteOffset", 0)
        stride = view.get("byteStride", 0) or dtype.itemsize * width
        if stride == dtype.itemsize * width:
            out = np.frombuffer(self.bin, dtype=dtype, count=count * width, offset=start).reshape(count, width)
        else:
            rows = [np.frombuffer(self.bin, dtype=dtype, count=width, offset=start + i * stride) for i in range(count)]
            out = np.stack(rows)
        out = out.astype(np.float64) if dtype == np.float32 else out.copy()
        if acc.get("normalized") and dtype != np.float32:
            out = out.astype(np.float64) / float(np.iinfo(dtype).max)
        return out[:, 0] if width == 1 else out

    # -- node hierarchy -------------------------------------------------------------------------
    @staticmethod
    def local_matrix(node: dict) -> np.ndarray:
        if "matrix" in node:
            return np.array(node["matrix"], dtype=np.float64).reshape(4, 4).T
        t = np.array(node.get("translation", [0.0, 0.0, 0.0]), dtype=np.float64)
        q = np.array(node.get("rotation", [0.0, 0.0, 0.0, 1.0]), dtype=np.float64)
        s = np.array(node.get("scale", [1.0, 1.0, 1.0]), dtype=np.float64)
        m = np.eye(4)
        m[:3, :3] = quat_matrix(q) * s
        m[:3, 3] = t
        return m

    def parents(self) -> list[int]:
        parent = [-1] * len(self.doc.get("nodes", []))
        for i, node in enumerate(self.doc.get("nodes", [])):
            for c in node.get("children", []):
                parent[c] = i
        return parent

    def world_matrices(self, local_override: dict[int, np.ndarray] | None = None) -> list[np.ndarray]:
        """World matrix of every node; `local_override` replaces chosen nodes' local matrices."""
        nodes = self.doc.get("nodes", [])
        parent = self.parents()
        world: list[np.ndarray | None] = [None] * len(nodes)

        def solve(i: int) -> np.ndarray:
            if world[i] is None:
                local = local_override[i] if local_override and i in local_override else self.local_matrix(nodes[i])
                world[i] = local if parent[i] < 0 else solve(parent[i]) @ local
            return world[i]

        return [solve(i) for i in range(len(nodes))]

    # -- meshes ---------------------------------------------------------------------------------
    def mesh_nodes(self) -> list[int]:
        return [i for i, n in enumerate(self.doc.get("nodes", [])) if "mesh" in n]

    def primitives(self, node_index: int, *, apply_node: bool = True) -> list[dict]:
        """Each triangle primitive of a mesh node: positions (world unless apply_node is False),
        triangle indices and, where present, skin joints/weights."""
        node = self.doc["nodes"][node_index]
        world = self.world_matrices()[node_index] if apply_node else np.eye(4)
        out = []
        for p in self.doc["meshes"][node["mesh"]]["primitives"]:
            if p.get("mode", 4) != 4:
                continue
            pos = self.accessor(p["attributes"]["POSITION"])
            pos = (np.c_[pos, np.ones(len(pos))] @ world.T)[:, :3]
            idx = self.accessor(p["indices"]).astype(np.int64).reshape(-1, 3) if "indices" in p else np.arange(len(pos)).reshape(-1, 3)
            item = {"positions": pos, "triangles": idx, "material": p.get("material")}
            # Every influence set (JOINTS_0/WEIGHTS_0, JOINTS_1/WEIGHTS_1, ...): the authoring rig
            # exports all influences, and its hands carry more than four.
            sets = []
            n = 0
            while f"JOINTS_{n}" in p["attributes"] and f"WEIGHTS_{n}" in p["attributes"]:
                sets.append((self.accessor(p["attributes"][f"JOINTS_{n}"]).astype(np.int64),
                             self.accessor(p["attributes"][f"WEIGHTS_{n}"]).astype(np.float64)))
                n += 1
            if sets:
                item["joints"] = np.concatenate([s[0] for s in sets], axis=1)
                item["weights"] = np.concatenate([s[1] for s in sets], axis=1)
            out.append(item)
        return out


def quat_matrix(q: np.ndarray) -> np.ndarray:
    x, y, z, w = q / max(np.linalg.norm(q), 1e-300)
    return np.array([
        [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
        [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
        [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
    ])


def euler_xyz_matrix(degrees: list[float] | tuple[float, float, float]) -> np.ndarray:
    """Blender's 'XYZ' Euler rotation (X applied first, then Y, then Z), as a 3x3 matrix."""
    ax, ay, az = np.radians(np.asarray(degrees, dtype=np.float64))
    rx = np.array([[1, 0, 0], [0, np.cos(ax), -np.sin(ax)], [0, np.sin(ax), np.cos(ax)]])
    ry = np.array([[np.cos(ay), 0, np.sin(ay)], [0, 1, 0], [-np.sin(ay), 0, np.cos(ay)]])
    rz = np.array([[np.cos(az), -np.sin(az), 0], [np.sin(az), np.cos(az), 0], [0, 0, 1]])
    return rz @ ry @ rx
