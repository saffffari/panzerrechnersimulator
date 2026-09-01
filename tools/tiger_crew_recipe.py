#!/usr/bin/env python3
"""Offline contracts shared by the AdrielCZ Panzer crew baker and verifier."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path, PurePosixPath
import struct
from typing import Any, Iterable


ARCHIVE_SHA256 = "ecc46d04c5016f07b54838e65b56705486917f2ccda2fce5708900e406183483"
SOURCE_FBX_SHA256 = "0b17a804879a485744de15b51c3294b49a398a91728a214859df4dc87d63c3a2"
TARGET_HEIGHT_M = 1.53
IDENTITIES = tuple(f"face{index:02d}" for index in range(1, 6))
POSES = ("standing", "seated", "relaxed")

SOURCE_MESH_TOPOLOGY = {
    "PanzerTruppe_Body_N1": {"vertices": 110, "polygons": 110, "triangles": 204},
    "PanzerTruppe_L_Arm_N1": {"vertices": 66, "polygons": 65, "triangles": 122},
    "PanzerTruppe_R_Arm_N1": {"vertices": 66, "polygons": 65, "triangles": 122},
    "PanzerTruppe_Head_N1": {"vertices": 64, "polygons": 58, "triangles": 116},
}
CHARACTER_VERTICES = 306
CHARACTER_TRIANGLES = 564
BINOCULAR_VERTICES = 96
BINOCULAR_TRIANGLES = 144

ARMATURE_BONES = (
    "ROOT_N1",
    "Spine_1_N1",
    "Spine_2_N1",
    "Chest_N1",
    "L_Shoulder_N1",
    "L_Arm_N1",
    "L_Elbow_N1",
    "L_Wrist_N1",
    "L_Hand_N1",
    "L_Fingers_1_N1",
    "L_Fingers_2_N1",
    "L_Fingers_3_N1",
    "L_Thumb_1_N1",
    "L_Thumb_2_N1",
    "L_Thumb_3_N1",
    "R_Shoulder_N1",
    "R_Arm_N1",
    "R_Elbow_N1",
    "R_Wrist_N1",
    "R_Hand_N1",
    "R_Fingers_1_N1",
    "R_Fingers_2_N1",
    "R_Fingers_3_N1",
    "R_Thumb_1_N1",
    "R_Thumb_2_N1",
    "R_Thumb_3_N1",
    "Neck_N1",
    "Head_N1",
    "Eyes_N1",
    "R_Leg_N1",
    "R_Knee_N1",
    "R_Ankle_N1",
    "R_Foot_N1",
    "R_Heel_N1",
    "L_Leg_N1",
    "L_Knee_N1",
    "L_Ankle_N1",
    "L_Foot_N1",
    "L_Heel_N1",
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def stable_json(value: Any) -> str:
    return json.dumps(value, indent=2, sort_keys=True) + "\n"


def safe_relative_path(value: str) -> PurePosixPath:
    path = PurePosixPath(value)
    if path.is_absolute() or not path.parts or any(part in {"", ".", ".."} for part in path.parts):
        raise ValueError(f"unsafe logical path: {value!r}")
    return path


def file_record(path: Path, *, logical_path: str) -> dict[str, Any]:
    safe_relative_path(logical_path)
    if not path.is_file():
        raise ValueError(f"receipt target is not a file: {path}")
    return {
        "path": logical_path,
        "bytes": path.stat().st_size,
        "sha256": sha256_file(path),
    }


def verify_file_record(root: Path, record: dict[str, Any]) -> Path:
    relative = safe_relative_path(str(record["path"]))
    path = root.joinpath(*relative.parts)
    if not path.is_file():
        raise ValueError(f"receipt file is missing: {relative}")
    actual_size = path.stat().st_size
    actual_hash = sha256_file(path)
    if actual_size != int(record["bytes"]) or actual_hash != str(record["sha256"]):
        raise ValueError(
            f"stale receipt for {relative}: expected {record['bytes']} bytes/{record['sha256']}, "
            f"got {actual_size} bytes/{actual_hash}"
        )
    return path


def read_glb_document(path: Path) -> dict[str, Any]:
    data = path.read_bytes()
    if len(data) < 20:
        raise ValueError(f"truncated GLB: {path}")
    magic, version, declared_length = struct.unpack_from("<4sII", data, 0)
    if magic != b"glTF" or version != 2 or declared_length != len(data):
        raise ValueError(f"invalid GLB header: {path}")
    offset = 12
    json_chunk = None
    while offset < len(data):
        if offset + 8 > len(data):
            raise ValueError(f"truncated GLB chunk header: {path}")
        length, chunk_type = struct.unpack_from("<II", data, offset)
        offset += 8
        end = offset + length
        if end > len(data):
            raise ValueError(f"truncated GLB chunk body: {path}")
        if chunk_type == 0x4E4F534A:
            if json_chunk is not None:
                raise ValueError(f"multiple JSON chunks in GLB: {path}")
            json_chunk = data[offset:end]
        offset = end
    if offset != len(data) or json_chunk is None:
        raise ValueError(f"GLB has no valid JSON chunk: {path}")
    return json.loads(json_chunk.rstrip(b" \t\r\n\0").decode("utf-8"))


def _primitives(document: dict[str, Any]) -> list[dict[str, Any]]:
    return [primitive for mesh in document.get("meshes", []) for primitive in mesh.get("primitives", [])]


def _triangle_count(document: dict[str, Any]) -> int:
    accessors = document.get("accessors", [])
    result = 0
    for primitive in _primitives(document):
        if int(primitive.get("mode", 4)) != 4:
            raise ValueError("runtime GLB contains a non-triangle primitive")
        accessor_index = primitive.get("indices", primitive.get("attributes", {}).get("POSITION"))
        if accessor_index is None or not (0 <= int(accessor_index) < len(accessors)):
            raise ValueError("runtime GLB primitive has no valid index/position accessor")
        count = int(accessors[int(accessor_index)].get("count", 0))
        if count <= 0 or count % 3:
            raise ValueError("runtime GLB triangle accessor count is invalid")
        result += count // 3
    return result


def _bounds(document: dict[str, Any]) -> tuple[list[float], list[float]]:
    accessors = document.get("accessors", [])
    minima: list[list[float]] = []
    maxima: list[list[float]] = []
    for primitive in _primitives(document):
        index = primitive.get("attributes", {}).get("POSITION")
        if index is None or not (0 <= int(index) < len(accessors)):
            raise ValueError("runtime GLB primitive has no valid POSITION accessor")
        accessor = accessors[int(index)]
        minimum = accessor.get("min")
        maximum = accessor.get("max")
        if not isinstance(minimum, list) or not isinstance(maximum, list) or len(minimum) != 3 or len(maximum) != 3:
            raise ValueError("runtime GLB POSITION accessor has no three-axis bounds")
        minima.append([float(value) for value in minimum])
        maxima.append([float(value) for value in maximum])
    if not minima:
        raise ValueError("runtime GLB contains no mesh primitives")
    return (
        [min(values[axis] for values in minima) for axis in range(3)],
        [max(values[axis] for values in maxima) for axis in range(3)],
    )


def _round_vector(values: Iterable[float]) -> list[float]:
    return [round(float(value), 6) for value in values]


def _validate_one_embedded_base_color(document: dict[str, Any]) -> None:
    materials = document.get("materials", [])
    if len(materials) != 1:
        raise ValueError(f"runtime GLB must contain exactly one material, found {len(materials)}")
    primitives = _primitives(document)
    if not primitives or any(primitive.get("material") != 0 for primitive in primitives):
        raise ValueError("every runtime primitive must reference the sole material at index zero")
    pbr = materials[0].get("pbrMetallicRoughness", {})
    texture_info = pbr.get("baseColorTexture")
    if not isinstance(texture_info, dict):
        raise ValueError("sole runtime material has no base-color texture")
    textures = document.get("textures", [])
    texture_index = int(texture_info.get("index", -1))
    if not (0 <= texture_index < len(textures)):
        raise ValueError("runtime base-color texture index is invalid")
    images = document.get("images", [])
    image_index = int(textures[texture_index].get("source", -1))
    if not (0 <= image_index < len(images)):
        raise ValueError("runtime base-color image index is invalid")
    image = images[image_index]
    if "bufferView" not in image or "uri" in image:
        raise ValueError("runtime base-color image must be embedded in the GLB")


def _validate_no_bundled_tiger(document: dict[str, Any]) -> None:
    forbidden = [
        str(node.get("name", ""))
        for node in document.get("nodes", [])
        if "tiger_h1" in str(node.get("name", "")).casefold()
    ]
    if forbidden:
        raise ValueError(f"crew GLB leaked bundled Tiger nodes: {forbidden}")


def validate_static_runtime_glb(path: Path, *, expected_triangles: int) -> dict[str, Any]:
    document = read_glb_document(path)
    _validate_one_embedded_base_color(document)
    _validate_no_bundled_tiger(document)
    if document.get("skins"):
        raise ValueError(f"static runtime GLB unexpectedly contains skins: {path}")
    if document.get("animations"):
        raise ValueError(f"static runtime GLB unexpectedly contains animations: {path}")
    required = {"POSITION", "NORMAL", "TEXCOORD_0"}
    for primitive in _primitives(document):
        if not required.issubset(primitive.get("attributes", {})):
            raise ValueError(f"runtime GLB primitive lacks position/normal/UV attributes: {path}")
    triangles = _triangle_count(document)
    if triangles != expected_triangles:
        raise ValueError(f"runtime GLB triangle count changed: expected {expected_triangles}, got {triangles}")
    minimum, maximum = _bounds(document)
    return {
        "triangles": triangles,
        "primitive_count": len(_primitives(document)),
        "material_count": len(document.get("materials", [])),
        "image_count": len(document.get("images", [])),
        "bounds_min_m": _round_vector(minimum),
        "bounds_max_m": _round_vector(maximum),
    }


def validate_rigged_master_glb(path: Path) -> dict[str, Any]:
    document = read_glb_document(path)
    _validate_one_embedded_base_color(document)
    _validate_no_bundled_tiger(document)
    if document.get("animations"):
        raise ValueError("rigged authoring master must not imply source animation")
    skins = document.get("skins", [])
    if not skins or not all(skin.get("joints") for skin in skins):
        raise ValueError("rigged authoring master has no populated skin")
    attributes = [primitive.get("attributes", {}) for primitive in _primitives(document)]
    if not attributes or not all({"JOINTS_0", "WEIGHTS_0"}.issubset(item) for item in attributes):
        raise ValueError("rigged authoring master does not retain skin weights on every primitive")
    triangles = _triangle_count(document)
    if triangles != CHARACTER_TRIANGLES:
        raise ValueError(
            f"rigged authoring master triangle count changed: expected {CHARACTER_TRIANGLES}, got {triangles}"
        )
    minimum, maximum = _bounds(document)
    return {
        "triangles": triangles,
        "skin_count": len(skins),
        "joint_count": len({joint for skin in skins for joint in skin.get("joints", [])}),
        "bounds_min_m": _round_vector(minimum),
        "bounds_max_m": _round_vector(maximum),
    }


def verify_bake_receipt(
    asset_root: Path,
    report: dict[str, Any],
    *,
    extraction_root: Path | None = None,
) -> dict[str, Any]:
    asset_root = asset_root.resolve()
    if report.get("schema") != "kessel.tiger-crew-bake" or report.get("version") != 1:
        raise ValueError("unsupported Tiger crew bake receipt")
    rigged_path = verify_file_record(asset_root, report["rigged_master"])
    manifest_path = verify_file_record(asset_root, report["runtime_manifest"])
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("schema") != "kessel.tiger-crew-runtime-manifest" or manifest.get("version") != 1:
        raise ValueError("unsupported Tiger crew runtime manifest")
    if tuple(manifest.get("identities", {}).keys()) != IDENTITIES:
        raise ValueError("runtime manifest identity order/set changed")

    expected_paths: set[str] = set()
    for identity in IDENTITIES:
        poses = manifest["identities"][identity].get("poses", {})
        if set(poses) != set(POSES) or len(poses) != len(POSES):
            raise ValueError(f"runtime manifest poses changed for {identity}")
        expected_paths.update(str(path) for path in poses.values())
    binocular_path = str(manifest.get("props", {}).get("binoculars", ""))
    if not binocular_path:
        raise ValueError("runtime manifest has no physical binocular prop")
    expected_paths.add(binocular_path)
    expected_paths.update(str(path) for path in manifest.get("first_person_binocular_frames", {}).values())

    output_records = report.get("runtime_outputs", [])
    actual_paths = {str(record.get("path")) for record in output_records}
    if actual_paths != expected_paths or len(output_records) != len(expected_paths):
        raise ValueError("runtime output receipt set does not match the manifest")

    verified_outputs = []
    for record in sorted(output_records, key=lambda item: str(item["path"])):
        path = verify_file_record(asset_root, record)
        role = record.get("role")
        expected_triangles = BINOCULAR_TRIANGLES if role == "physical_binoculars" else CHARACTER_TRIANGLES
        if role == "first_person_binocular_frame":
            expected_triangles = 2 * 122 + BINOCULAR_TRIANGLES
        summary = validate_static_runtime_glb(path, expected_triangles=expected_triangles)
        if summary["bounds_min_m"] != record.get("bounds_min_m") or summary["bounds_max_m"] != record.get("bounds_max_m"):
            raise ValueError(f"stale bounds receipt for {record['path']}")
        verified_outputs.append({"path": record["path"], **summary})

    rigged_summary = validate_rigged_master_glb(rigged_path)
    if extraction_root is not None:
        extraction_root = extraction_root.resolve()
        for record in report.get("sources", {}).get("extracted_files", []):
            verify_file_record(extraction_root, record)
        receipt_record = report.get("sources", {}).get("extraction_receipt")
        if receipt_record:
            verify_file_record(extraction_root, receipt_record)

    baked_glbs = {
        path.relative_to(asset_root).as_posix()
        for directory in (asset_root / "baked", asset_root / "authoring")
        if directory.is_dir()
        for path in directory.glob("*.glb")
    }
    expected_glbs = expected_paths | {str(report["rigged_master"]["path"])}
    if baked_glbs != expected_glbs:
        raise ValueError(
            f"unexpected or missing GLBs: expected {sorted(expected_glbs)}, got {sorted(baked_glbs)}"
        )
    return {
        "schema": "kessel.tiger-crew-bake-verification",
        "version": 1,
        "runtime_output_count": len(verified_outputs),
        "identity_count": len(IDENTITIES),
        "pose_count_per_identity": len(POSES),
        "rigged_master": rigged_summary,
        "source_bytes_reverified": extraction_root is not None,
    }
