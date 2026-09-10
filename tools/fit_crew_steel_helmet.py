#!/usr/bin/env python3
"""Extract the preserved CC0 steel helmet and register it to the current crew head.

CPU-only; no Blender session, original mesh, uniform atlas or rig is modified. Run with
--check to reproduce the derivative/receipt in memory and compare the committed outputs.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path
import struct

import numpy as np


ROOT = Path(__file__).resolve().parents[1]
OLD = "assets/flavor/Characters/german-ww2-soldier-cc0/baked/german_ww2_soldier_relaxed_static.glb"
CREW = "assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/baked"
OUTPUT = "assets/flavor/Characters/german-ww2-soldier-cc0/baked/german_steel_helmet_panzertruppe_relaxed_static.glb"
RECEIPT = OUTPUT.removesuffix(".glb") + ".report.json"
CROWN_CLEARANCE_M = 0.020


def read_glb(path: str) -> tuple[dict, bytes, bytes]:
    raw = (ROOT / path).read_bytes()
    magic, version, length = struct.unpack_from("<4sII", raw)
    assert (magic, version, length) == (b"glTF", 2, len(raw))
    json_size, json_type = struct.unpack_from("<II", raw, 12)
    assert json_type == 0x4E4F534A
    doc = json.loads(raw[20:20 + json_size])
    bin_size, bin_type = struct.unpack_from("<II", raw, 20 + json_size)
    assert bin_type == 0x004E4942
    binary = raw[28 + json_size:28 + json_size + bin_size]
    return doc, binary, raw


def primitive(doc: dict, name: str) -> dict:
    node = next(node for node in doc["nodes"] if node.get("name") == name)
    # These reviewed static bakes put evaluated positions in mesh coordinates.
    assert not any(key in node for key in ("matrix", "translation", "rotation", "scale", "skin"))
    prims = doc["meshes"][node["mesh"]]["primitives"]
    assert len(prims) == 1
    return prims[0]


def accessor(doc: dict, binary: bytes, index: int) -> np.ndarray:
    acc = doc["accessors"][index]
    view = doc["bufferViews"][acc["bufferView"]]
    dtype = {5123: "<u2", 5125: "<u4", 5126: "<f4"}[acc["componentType"]]
    components = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[acc["type"]]
    size = np.dtype(dtype).itemsize
    return np.ndarray(
        (acc["count"], components), dtype, buffer=binary,
        offset=view.get("byteOffset", 0) + acc.get("byteOffset", 0),
        strides=(view.get("byteStride", components * size), size),
    ).copy()


def head_points(pose: str, identity: int = 1) -> tuple[np.ndarray, str, bytes]:
    path = f"{CREW}/german_panzertruppe_face{identity:02d}_{pose}_static.glb"
    doc, binary, raw = read_glb(path)
    prim = primitive(doc, "PanzerTruppe_Head_N1_Static")
    return accessor(doc, binary, prim["attributes"]["POSITION"]).astype(float), path, raw


def generate() -> tuple[bytes, bytes]:
    source, source_bin, source_raw = read_glb(OLD)
    helmet = primitive(source, "Helmet_Static")
    source_positions = accessor(source, source_bin, helmet["attributes"]["POSITION"])
    assert source_positions.shape == (229, 3)
    head, head_path, head_raw = head_points("relaxed")
    helmet_min, helmet_max = source_positions.min(0), source_positions.max(0)
    head_min, head_max = head.min(0), head.max(0)
    # Retain the reference helmet's dimensions and silhouette. Center it laterally/depth-wise,
    # with 20 mm above the current scalp. This is a visual fit, not collision/armor geometry.
    offset = (head_min + head_max - helmet_min - helmet_max) * 0.5
    offset[1] = head_max[1] + CROWN_CLEARANCE_M - helmet_max[1]
    fitted = (source_positions + offset).astype("<f4")
    assert np.all((fitted.max(0) - fitted.min(0))[[0, 2]] > (head_max - head_min)[[0, 2]])

    out = {"asset": {"version": "2.0", "generator": "Kessel fit_crew_steel_helmet.py"},
           "scene": 0, "scenes": [{"nodes": [0]}],
           "nodes": [{"mesh": 0, "name": "CC0_Steel_Helmet_Panzertruppe_Relaxed"}],
           "meshes": [{"primitives": []}], "accessors": [], "bufferViews": []}
    payload = bytearray()

    def add_view(blob: bytes, target: int | None = None) -> int:
        payload.extend(b"\0" * (-len(payload) % 4))
        view = {"buffer": 0, "byteOffset": len(payload), "byteLength": len(blob)}
        if target is not None:
            view["target"] = target
        out["bufferViews"].append(view)
        payload.extend(blob)
        return len(out["bufferViews"]) - 1

    def add_accessor(index: int, positions: np.ndarray | None = None) -> int:
        original = source["accessors"][index]
        data = accessor(source, source_bin, index) if positions is None else positions
        acc = {"bufferView": add_view(data.tobytes()), "componentType": original["componentType"],
               "count": original["count"], "type": original["type"]}
        if positions is not None:
            acc.update(min=data.min(0).tolist(), max=data.max(0).tolist())
        out["accessors"].append(acc)
        return len(out["accessors"]) - 1

    attrs = {name: add_accessor(index, fitted if name == "POSITION" else None)
             for name, index in helmet["attributes"].items()}
    out["meshes"][0]["primitives"].append({"attributes": attrs,
        "indices": add_accessor(helmet["indices"]), "material": 0})
    material = copy.deepcopy(source["materials"][helmet["material"]])
    pbr = material["pbrMetallicRoughness"]
    texture = source["textures"][pbr["baseColorTexture"]["index"]]
    img = source["images"][texture["source"]]
    view = source["bufferViews"][img["bufferView"]]
    image_bytes = source_bin[view.get("byteOffset", 0):view.get("byteOffset", 0) + view["byteLength"]]
    pbr["baseColorTexture"]["index"] = 0
    # Painted steel uses the same matte factors as the old reference renderer.
    pbr["metallicFactor"], pbr["roughnessFactor"] = 0.02, 0.88
    out["materials"] = [material]
    out["images"] = [{"bufferView": add_view(image_bytes), "mimeType": img["mimeType"],
                      "name": "Preserved_CC0_Soldier_Atlas"}]
    out["textures"] = [{"source": 0}]
    if "sampler" in texture:
        out["samplers"] = [source["samplers"][texture["sampler"]]]
        out["textures"][0]["sampler"] = 0
    out["buffers"] = [{"byteLength": len(payload)}]
    text = json.dumps(out, separators=(",", ":"), sort_keys=True).encode()
    text += b" " * (-len(text) % 4)
    payload.extend(b"\0" * (-len(payload) % 4))
    glb = (struct.pack("<4sII", b"glTF", 2, 28 + len(text) + len(payload))
           + struct.pack("<II", len(text), 0x4E4F534A) + text
           + struct.pack("<II", len(payload), 0x004E4942) + payload)

    poses = {}
    for pose in ("seated", "standing"):
        target, _, _ = head_points(pose)
        ca, cb = head.mean(0), target.mean(0)
        u, _, vt = np.linalg.svd((head - ca).T @ (target - cb))
        rotation = vt.T @ u.T
        assert np.linalg.det(rotation) > 0.99999
        translation = cb - rotation @ ca
        error = np.linalg.norm(head @ rotation.T + translation - target, axis=1).max()
        assert error < 1e-6, "helmet needs a new pose attachment, not an approximate seat offset"
        for identity in range(1, 6):
            other, _, _ = head_points(pose, identity)
            assert np.array_equal(other, target), "face identities must share the attachment geometry"
        poses[pose] = {"rotation_rows": rotation.tolist(), "translation_m": translation.tolist(),
                       "maximum_head_registration_error_m": float(error)}
    receipt = {"schema": "kessel.crew-helmet-fit", "version": 1, "license": "CC0-1.0",
        "source": OLD, "source_sha256": hashlib.sha256(source_raw).hexdigest(),
        "source_node": "Helmet_Static", "target": head_path,
        "target_sha256": hashlib.sha256(head_raw).hexdigest(), "output": OUTPUT,
        "output_sha256": hashlib.sha256(glb).hexdigest(), "vertices": len(fitted),
        "triangles": source["accessors"][helmet["indices"]]["count"] // 3,
        "atlas_png_sha256": hashlib.sha256(image_bytes).hexdigest(),
        "uniform_scale": 1.0, "translation_m": offset.tolist(),
        "crown_clearance_m": CROWN_CLEARANCE_M,
        "head_bounds_m": {"min": head_min.tolist(), "max": head_max.tolist()},
        "helmet_bounds_m": {"min": fitted.min(0).tolist(), "max": fitted.max(0).tolist()},
        "live_pose_from_relaxed": poses,
        "limitations": ["Visual source-derived fit; no helmet armor or independent helmet physics.",
                         "Helmet follows a present detached head; it is hidden when that head is pulverized.",
                         "Current crew meshes, five face atlases, uniform and rig are unchanged."]}
    return glb, (json.dumps(receipt, indent=2, sort_keys=True) + "\n").encode()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    glb, receipt = generate()
    for path, data in ((OUTPUT, glb), (RECEIPT, receipt)):
        if args.check:
            assert (ROOT / path).read_bytes() == data, f"stale derivative: {path}"
        else:
            (ROOT / path).write_bytes(data)
    print("crew steel helmet: source triangles/UVs/atlas preserved; all five live head poses registered")


if __name__ == "__main__":
    main()
