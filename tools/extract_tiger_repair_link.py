#!/usr/bin/env python3
"""Copy one actual Tiger track shoe into a small, centered runtime GLB.

No Blender rebuild, asset search or decimation. All source triangles and normals survive;
the node's uniform authored scale is baked and its bounds center becomes the link pivot.
"""
import argparse
import hashlib
import json
import struct
from pathlib import Path


def extract(source: Path, output: Path):
    raw = source.read_bytes()
    assert raw[:4] == b"glTF"
    json_length = struct.unpack_from("<I", raw, 12)[0]
    doc = json.loads(raw[20:20 + json_length])
    bin_header = 20 + json_length
    bin_length, kind = struct.unpack_from("<II", raw, bin_header)
    assert kind == 0x004E4942
    source_bin = raw[bin_header + 8:bin_header + 8 + bin_length]
    node = next(n for n in doc["nodes"] if n.get("name") == "Hull_0628")
    assert "rotation" not in node and "matrix" not in node
    scale = node["scale"]
    assert max(scale) == min(scale) and min(scale) > 0
    mesh = doc["meshes"][node["mesh"]]
    assert len(mesh["primitives"]) == 1
    primitive = mesh["primitives"][0]
    position = doc["accessors"][primitive["attributes"]["POSITION"]]
    center = [(a + b) * 0.5 for a, b in zip(position["min"], position["max"])]
    output_bin = bytearray()
    accessors, views = [], []

    def copy_accessor(index, transform_positions=False):
        accessor = doc["accessors"][index]
        view = doc["bufferViews"][accessor["bufferView"]]
        assert "sparse" not in accessor
        width = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[accessor["type"]]
        size = {5121: 1, 5123: 2, 5125: 4, 5126: 4}[accessor["componentType"]] * width
        stride = view.get("byteStride", size)
        start = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
        data = bytearray()
        for i in range(accessor["count"]):
            row = source_bin[start + i * stride:start + i * stride + size]
            if transform_positions:
                assert accessor["componentType"] == 5126 and width == 3
                row = struct.pack("<3f", *[(x - c) * s for x, c, s in
                    zip(struct.unpack("<3f", row), center, scale)])
            data.extend(row)
        output_bin.extend(b"\0" * (-len(output_bin) % 4))
        views.append({"buffer": 0, "byteOffset": len(output_bin), "byteLength": len(data)})
        output_bin.extend(data)
        result = {k: accessor[k] for k in ("componentType", "count", "type")}
        result["bufferView"] = len(views) - 1
        if "normalized" in accessor:
            result["normalized"] = accessor["normalized"]
        if transform_positions:
            for key in ("min", "max"):
                result[key] = [(x - c) * s for x, c, s in zip(accessor[key], center, scale)]
        accessors.append(result)
        return len(accessors) - 1

    attributes = {name: copy_accessor(index, name == "POSITION")
                  for name, index in primitive["attributes"].items()}
    indices = copy_accessor(primitive["indices"])
    # Same authored game track finish as the rigid hull's TRACK_STEEL group.
    linear = lambda v: ((v / 255.0 + 0.055) / 1.055) ** 2.4
    material = {"name": "Tiger repair track steel", "doubleSided": True,
                "pbrMetallicRoughness": {"baseColorFactor": [linear(58), linear(58), linear(56), 1.0],
                                         "metallicFactor": 0.48, "roughnessFactor": 0.90}}
    output_doc = {"asset": {"version": "2.0", "generator": "extract_tiger_repair_link.py"},
        "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"name": "Tiger track shoe Hull_0628", "mesh": 0}],
        "meshes": [{"primitives": [{"attributes": attributes, "indices": indices, "material": 0}]}],
        "materials": [material], "accessors": accessors, "bufferViews": views,
        "buffers": [{"byteLength": len(output_bin)}]}
    json_bytes = json.dumps(output_doc, separators=(",", ":")).encode()
    json_bytes += b" " * (-len(json_bytes) % 4)
    output_bin.extend(b"\0" * (-len(output_bin) % 4))
    result = struct.pack("<III", 0x46546C67, 2, 28 + len(json_bytes) + len(output_bin))
    result += struct.pack("<II", len(json_bytes), 0x4E4F534A) + json_bytes
    result += struct.pack("<II", len(output_bin), 0x004E4942) + output_bin
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(result)
    print(json.dumps({"source": str(source), "source_node": node["name"],
        "source_sha256": hashlib.sha256(raw).hexdigest(), "output": str(output),
        "output_sha256": hashlib.sha256(result).hexdigest(),
        "triangles": accessors[indices]["count"] // 3,
        "bounds": [accessors[attributes["POSITION"]][k] for k in ("min", "max")]}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    extract(args.source, args.output)
