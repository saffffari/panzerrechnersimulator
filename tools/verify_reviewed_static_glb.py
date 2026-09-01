#!/usr/bin/env python3
"""Offline structural verifier for the reviewed Enigma and bridge runtime GLBs."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import struct
from typing import Any

import reviewed_static_asset_recipe as recipe


GLB_MAGIC = b"glTF"
GLB_VERSION = 2
JSON_CHUNK_TYPE = 0x4E4F534A
BIN_CHUNK_TYPE = 0x004E4942


def parse_glb(path: Path) -> tuple[dict[str, Any], bytes]:
    data = path.read_bytes()
    if len(data) < 20:
        raise ValueError(f"GLB is too short: {path}")
    magic, version, declared_length = struct.unpack_from("<4sII", data, 0)
    if magic != GLB_MAGIC or version != GLB_VERSION:
        raise ValueError(f"not a GLB 2.0 file: {path}")
    if declared_length != len(data):
        raise ValueError(
            f"GLB length mismatch: header={declared_length}, bytes={len(data)}"
        )
    offset = 12
    chunks: list[tuple[int, bytes]] = []
    while offset < len(data):
        if offset + 8 > len(data):
            raise ValueError("truncated GLB chunk header")
        chunk_length, chunk_type = struct.unpack_from("<II", data, offset)
        offset += 8
        end = offset + chunk_length
        if end > len(data):
            raise ValueError("truncated GLB chunk payload")
        chunks.append((chunk_type, data[offset:end]))
        offset = end
    if not chunks or chunks[0][0] != JSON_CHUNK_TYPE:
        raise ValueError("GLB does not begin with a JSON chunk")
    if len(chunks) != 2 or chunks[1][0] != BIN_CHUNK_TYPE:
        raise ValueError("reviewed runtime GLB must contain exactly JSON and BIN chunks")
    document = json.loads(chunks[0][1].decode("utf-8").rstrip(" \t\r\n\0"))
    return document, chunks[1][1]


def primitive_triangle_count(document: dict[str, Any], primitive: dict[str, Any]) -> int:
    if primitive.get("mode", 4) != 4:
        raise ValueError("runtime primitive is not TRIANGLES")
    if "indices" in primitive:
        index_count = int(document["accessors"][primitive["indices"]]["count"])
        if index_count % 3 != 0:
            raise ValueError(f"triangle index count is not divisible by three: {index_count}")
        return index_count // 3
    position_accessor = primitive.get("attributes", {}).get("POSITION")
    if position_accessor is None:
        raise ValueError("runtime primitive has no POSITION accessor")
    vertex_count = int(document["accessors"][position_accessor]["count"])
    if vertex_count % 3 != 0:
        raise ValueError("non-indexed triangle vertex count is not divisible by three")
    return vertex_count // 3


def vector_close(left: list[float], right: list[float], tolerance: float = 2.0e-5) -> bool:
    return len(left) == len(right) and all(
        abs(float(a) - float(b)) <= tolerance for a, b in zip(left, right)
    )


def verify(asset_key: str, glb_path: Path, report_path: Path) -> dict[str, Any]:
    asset = recipe.ASSETS[asset_key]
    if glb_path.name != asset.output_filename:
        raise ValueError(
            f"{asset_key} GLB must be named {asset.output_filename!r}, got {glb_path.name!r}"
        )
    if report_path.name != asset.report_filename:
        raise ValueError(
            f"{asset_key} report must be named {asset.report_filename!r}, "
            f"got {report_path.name!r}"
        )
    document, binary_chunk = parse_glb(glb_path)
    report = json.loads(report_path.read_text(encoding="utf-8"))

    if report.get("schema") != "kessel.reviewed-static-asset-bake.v1":
        raise ValueError("unexpected reviewed-static report schema")
    if report.get("recipe_version") != recipe.RECIPE_VERSION:
        raise ValueError("bake report recipe version does not match verifier")
    expected_asset = {
        "key": asset.key,
        "title": asset.title,
        "creator": asset.creator,
        "sketchfab_uid": asset.sketchfab_uid,
        "source_url": asset.source_url,
        "license": "CC-BY-4.0",
    }
    if report.get("asset") != expected_asset:
        raise ValueError("bake report asset provenance does not match reviewed source")
    if report.get("source_archive") != recipe.source_archive_record(asset):
        raise ValueError("bake report original archive receipt does not match reviewed source")
    reported_input_hashes = report.get("inputs", {}).get("sha256")
    if reported_input_hashes != recipe.expected_input_sha256(asset):
        raise ValueError("bake report extracted source hashes do not match reviewed inputs")
    verified_tools = recipe.verify_toolchain_receipt(asset, report.get("toolchain"))
    output = report.get("output", {})
    if output.get("filename") != glb_path.name:
        raise ValueError("bake report output filename does not match GLB")
    if output.get("bytes") != glb_path.stat().st_size:
        raise ValueError("bake report output byte count does not match GLB")
    if output.get("sha256") != recipe.sha256(glb_path):
        raise ValueError("bake report output SHA-256 does not match GLB")

    buffers = document.get("buffers", [])
    if len(buffers) != 1 or "uri" in buffers[0]:
        raise ValueError("runtime GLB must contain one embedded buffer and no external buffer URI")
    declared_buffer_length = int(buffers[0].get("byteLength", -1))
    if declared_buffer_length < 0 or declared_buffer_length > len(binary_chunk):
        raise ValueError("embedded BIN chunk does not satisfy the declared buffer length")
    for image in document.get("images", []):
        if "uri" in image:
            raise ValueError("runtime GLB contains an external image URI")
        if "bufferView" not in image or image.get("mimeType") != "image/png":
            raise ValueError("runtime GLB images must be embedded PNG buffer views")
    expected_images = 3 if asset_key == "enigma" else 1
    if len(document.get("images", [])) != expected_images:
        raise ValueError(
            f"{asset_key} must embed {expected_images} atlas image(s), "
            f"found {len(document.get('images', []))}"
        )
    for forbidden in ("animations", "skins", "cameras"):
        if document.get(forbidden):
            raise ValueError(f"static runtime GLB unexpectedly contains {forbidden}")

    materials = document.get("materials", [])
    if len(materials) != 1:
        raise ValueError(f"runtime GLB must contain exactly one material, found {len(materials)}")
    pbr = materials[0].get("pbrMetallicRoughness", {})
    if "baseColorTexture" not in pbr:
        raise ValueError("runtime material has no embedded base-color atlas")
    if asset_key == "enigma":
        if "metallicRoughnessTexture" not in pbr:
            raise ValueError("Enigma runtime material has no metallic-roughness atlas")
        if "normalTexture" not in materials[0]:
            raise ValueError("Enigma runtime material has no normal atlas")

    meshes = document.get("meshes", [])
    if len(meshes) != 1:
        raise ValueError(f"runtime GLB must contain one mesh, found {len(meshes)}")
    primitives = meshes[0].get("primitives", [])
    if len(primitives) != 1:
        raise ValueError(
            f"one-material runtime GLB must contain one primitive, found {len(primitives)}"
        )
    primitive = primitives[0]
    if primitive.get("material") != 0:
        raise ValueError("runtime primitive does not reference the sole material")
    position_accessor_index = primitive.get("attributes", {}).get("POSITION")
    if position_accessor_index is None:
        raise ValueError("runtime primitive has no POSITION accessor")
    position_accessor = document["accessors"][position_accessor_index]
    bounds_min = [float(value) for value in position_accessor.get("min", [])]
    bounds_max = [float(value) for value in position_accessor.get("max", [])]
    if len(bounds_min) != 3 or len(bounds_max) != 3:
        raise ValueError("POSITION accessor does not declare three-dimensional bounds")
    report_bounds = report.get("runtime_geometry", {}).get("bounds_m", {})
    if not vector_close(bounds_min, report_bounds.get("min", [])):
        raise ValueError(f"GLB minimum bounds disagree with report: {bounds_min}")
    if not vector_close(bounds_max, report_bounds.get("max", [])):
        raise ValueError(f"GLB maximum bounds disagree with report: {bounds_max}")

    triangles = primitive_triangle_count(document, primitive)
    reported_triangles = report.get("runtime_geometry", {}).get("triangles")
    if triangles != reported_triangles:
        raise ValueError(
            f"GLB triangle count {triangles} disagrees with report {reported_triangles}"
        )
    dimensions = [bounds_max[index] - bounds_min[index] for index in range(3)]
    if not vector_close(
        dimensions, report.get("runtime_geometry", {}).get("dimensions_m", [])
    ):
        raise ValueError("GLB dimensions disagree with bake report")

    if asset_key == "enigma":
        if not 10_000 <= triangles <= 25_000:
            raise ValueError(f"Enigma triangle count outside runtime target: {triangles}")
        if abs(bounds_min[1]) > 2.0e-5:
            raise ValueError(f"Enigma does not stand on glTF Y=0: {bounds_min[1]}")
        if not (
            0.40 <= dimensions[0] <= 0.46
            and 0.50 <= dimensions[1] <= 0.58
            and 0.27 <= dimensions[2] <= 0.31
        ):
            raise ValueError(f"Enigma dimensions are not physically plausible: {dimensions}")
    else:
        expected = [19.978382, 3.508359, 4.065169]
        if triangles != 7_384:
            raise ValueError(f"bridge did not preserve its 7,384 triangles: {triangles}")
        if not vector_close(dimensions, expected, tolerance=4.0e-5):
            raise ValueError(f"bridge dimensions changed: {dimensions}")
        deck = report.get("runtime_geometry", {}).get("gltf_deck", {})
        if deck != {"axis": "Y", "value_m": 0.0}:
            raise ValueError("bridge report does not declare its driveable deck at glTF Y=0")
        if report.get("runtime_geometry", {}).get("gltf_long_axis") != "X":
            raise ValueError("bridge report does not declare long axis X")
        if not bounds_min[1] < 0.0 < bounds_max[1]:
            raise ValueError("bridge origin is not on an interior deck-height plane")

    return {
        "asset": asset_key,
        "glb": str(glb_path),
        "sha256": recipe.sha256(glb_path),
        "bytes": glb_path.stat().st_size,
        "triangles": triangles,
        "position_accessor_vertices": int(position_accessor["count"]),
        "dimensions_m": [round(value, 6) for value in dimensions],
        "embedded_images": len(document.get("images", [])),
        "materials": len(materials),
        "external_uris": 0,
        "verified_tool_files": sorted(verified_tools),
        "source_archive_sha256": asset.archive_sha256,
        "status": "PASS",
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("asset", choices=sorted(recipe.ASSETS))
    parser.add_argument("glb", type=Path)
    parser.add_argument("report", type=Path)
    arguments = parser.parse_args()
    summary = verify(
        arguments.asset,
        arguments.glb.expanduser().resolve(),
        arguments.report.expanduser().resolve(),
    )
    print(json.dumps(summary, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
