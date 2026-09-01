#!/usr/bin/env python3
"""Offline authority for the reviewed Vinrax skeleton character bake.

The module intentionally has no Blender dependency.  It gates the exact downloaded archive,
extracts only the two audited nested source members, verifies generated GLB structure, and checks
the stable bake receipt used by the runtime integration work.
"""

from __future__ import annotations

from collections import Counter
from contextlib import contextmanager
import hashlib
from io import BytesIO
import json
from pathlib import Path, PurePosixPath
import struct
import tempfile
from typing import Any, Iterable, Iterator, Sequence
from zipfile import ZipFile


TITLE = "Skeleton Character PSX"
CREATOR = "Vinrax"
SKETCHFAB_UID = "ece576bbed4b4364911c7596d828a558"
SOURCE_URL = (
    "https://sketchfab.com/3d-models/"
    "skeleton-character-psx-ece576bbed4b4364911c7596d828a558"
)
LICENSE_SPDX = "CC-BY-4.0"

ARCHIVE_SHA256 = "65825015009305fcc65db9eb524720306497c03ffadfdb2f1d0f733cb116e40d"
NESTED_ARCHIVE_SHA256 = "75499259c01a2e4d08332c3cc1a1cea4d9ab1f33bea86515c5bf71d8fafb720e"
SOURCE_FBX_SHA256 = "f30b1f5e28faf589ed57b55470433af5d0444a0537a948024fa3e868a692d454"
SOURCE_TEXTURE_SHA256 = "79b97748d0d1721bd5ef9367d63b6f74f06323b07b5060531636e8df5fb6148b"
ARCHIVE_FILENAME = "skeleton-character-psx.zip"
ARCHIVE_BYTES = 342_010
NESTED_ARCHIVE_BYTES = 194_192
SOURCE_FBX_BYTES = 115_676
SOURCE_TEXTURE_BYTES = 133_426

OUTER_MEMBERS = (
    "source/skeleton_psx_fbx.zip",
    "textures/internal_ground_ao_texture.jpeg",
    "textures/skeleton_d.png",
)
NESTED_MEMBERS = ("skeleton.fbx", "skeleton_d.png")

TARGET_HEIGHT_M = 1.53
SOURCE_VERTICES = 855
SOURCE_EDGES = 1812
SOURCE_POLYGONS = 1006
SOURCE_TRIANGLES = 1596
SOURCE_LOOPS = 3608
TEXTURE_SIZE_PX = (256, 256)

ARMATURE_BONES = (
    "hips",
    "spine",
    "chest",
    "neck",
    "head",
    "jaw",
    "jaw_end",
    "shoulder.L",
    "upper_arm.L",
    "forearm.L",
    "hand.L",
    "hand.L_end",
    "shoulder.R",
    "upper_arm.R",
    "forearm.R",
    "hand.R",
    "hand.R_end",
    "thigh.L",
    "shin.L",
    "foot.L",
    "toe.L",
    "toe.L_end",
    "thigh.R",
    "shin.R",
    "foot.R",
    "toe.R",
    "toe.R_end",
)

BODY_PARTS = (
    "Pelvis",
    "Torso",
    "Head",
    "LeftUpperArm",
    "LeftForearm",
    "RightUpperArm",
    "RightForearm",
    "LeftThigh",
    "LeftShin",
    "RightThigh",
    "RightShin",
)

# Snapshot of src/crew_physics.rs.  These are comparison targets, not claims that the downloaded
# skeleton is registered exactly to the clothed crew mesh or its conservative collision boxes.
BODY_PART_TARGET_CENTERS_M = {
    "Pelvis": (0.0, 0.735, 0.0),
    "Torso": (0.0, 1.045, 0.0),
    "Head": (0.0, 1.405, 0.0),
    "LeftUpperArm": (-0.218, 1.000, 0.0),
    "LeftForearm": (-0.220, 0.635, 0.008),
    "RightUpperArm": (0.218, 1.000, 0.0),
    "RightForearm": (0.220, 0.635, 0.008),
    "LeftThigh": (-0.105, 0.520, 0.0),
    "LeftShin": (-0.100, 0.205, 0.004),
    "RightThigh": (0.105, 0.520, 0.0),
    "RightShin": (0.100, 0.205, 0.004),
}

RIGGED_RELATIVE = "authoring/skeleton_character_psx_rigged.glb"
STATIC_RELATIVE = "baked/skeleton_character_psx_relaxed_static.glb"
ALIGNMENT_RELATIVE = "baked/anatomy_alignment.v1.json"
REPORT_RELATIVE = "bake.report.json"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


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
    relative = safe_relative_path(str(record.get("path", "")))
    path = root.joinpath(*relative.parts)
    if not path.is_file():
        raise ValueError(f"receipt file is missing: {relative}")
    actual = file_record(path, logical_path=relative.as_posix())
    for key in ("path", "bytes", "sha256"):
        if actual[key] != record.get(key):
            raise ValueError(f"stale receipt for {relative}: expected {record}, got {actual}")
    return path


def expected_tool_paths() -> dict[str, str]:
    return {
        "baker": "tools/bake_skeleton_character.py",
        "shared_recipe": "tools/skeleton_character_recipe.py",
        "offline_verifier": "tools/verify_skeleton_character_bake.py",
    }


def toolchain_receipt() -> dict[str, dict[str, Any]]:
    repository_root = Path(__file__).resolve().parent.parent
    return {
        role: file_record(
            repository_root / logical_path,
            logical_path=logical_path,
        )
        for role, logical_path in expected_tool_paths().items()
    }


def verify_toolchain_receipt(records: object) -> dict[str, dict[str, Any]]:
    expected_paths = expected_tool_paths()
    if not isinstance(records, dict) or set(records) != set(expected_paths):
        raise ValueError(
            "skeleton tool receipt set changed: "
            f"expected {sorted(expected_paths)}, "
            f"got {sorted(records) if isinstance(records, dict) else type(records).__name__}"
        )
    repository_root = Path(__file__).resolve().parent.parent
    verified: dict[str, dict[str, Any]] = {}
    for role, logical_path in expected_paths.items():
        record = records.get(role)
        if not isinstance(record, dict) or record.get("path") != logical_path:
            raise ValueError(f"skeleton {role} tool path changed")
        verify_file_record(repository_root, record)
        verified[role] = record
    return verified


def expected_private_source_receipt() -> dict[str, Any]:
    return {
        "archive": {
            "filename": ARCHIVE_FILENAME,
            "bytes": ARCHIVE_BYTES,
            "sha256": ARCHIVE_SHA256,
        },
        "nested_archive": {
            "member": "source/skeleton_psx_fbx.zip",
            "bytes": NESTED_ARCHIVE_BYTES,
            "sha256": NESTED_ARCHIVE_SHA256,
        },
        "reviewed_members": [
            {
                "path": "skeleton.fbx",
                "bytes": SOURCE_FBX_BYTES,
                "sha256": SOURCE_FBX_SHA256,
            },
            {
                "path": "skeleton_d.png",
                "bytes": SOURCE_TEXTURE_BYTES,
                "sha256": SOURCE_TEXTURE_SHA256,
            },
        ],
    }


def _validate_member_set(actual: Sequence[str], expected: Sequence[str], label: str) -> None:
    if Counter(actual) != Counter(expected) or len(actual) != len(expected):
        raise ValueError(f"{label} member set changed: expected {list(expected)}, got {list(actual)}")
    for name in actual:
        safe_relative_path(name)


@contextmanager
def extract_reviewed_source(archive_path: Path) -> Iterator[tuple[Path, Path, dict[str, Any]]]:
    """Yield the reviewed nested FBX and texture from a private temporary directory."""

    archive_path = archive_path.expanduser().resolve()
    if not archive_path.is_file():
        raise ValueError(f"skeleton archive does not exist: {archive_path}")
    archive_hash = sha256_file(archive_path)
    if archive_hash != ARCHIVE_SHA256:
        raise ValueError(
            f"unreviewed skeleton archive: expected {ARCHIVE_SHA256}, got {archive_hash}"
        )
    with ZipFile(archive_path) as outer:
        _validate_member_set(outer.namelist(), OUTER_MEMBERS, "outer skeleton archive")
        nested_bytes = outer.read("source/skeleton_psx_fbx.zip")
    if sha256_bytes(nested_bytes) != NESTED_ARCHIVE_SHA256:
        raise ValueError("nested skeleton FBX archive hash changed")
    with ZipFile(BytesIO(nested_bytes)) as nested:
        _validate_member_set(nested.namelist(), NESTED_MEMBERS, "nested skeleton archive")
        fbx_bytes = nested.read("skeleton.fbx")
        texture_bytes = nested.read("skeleton_d.png")
    if sha256_bytes(fbx_bytes) != SOURCE_FBX_SHA256:
        raise ValueError("reviewed skeleton FBX hash changed")
    if sha256_bytes(texture_bytes) != SOURCE_TEXTURE_SHA256:
        raise ValueError("reviewed skeleton texture hash changed")

    with tempfile.TemporaryDirectory(prefix="kessel-reviewed-skeleton-") as temporary:
        root = Path(temporary)
        fbx_path = root / "skeleton.fbx"
        texture_path = root / "skeleton_d.png"
        fbx_path.write_bytes(fbx_bytes)
        texture_path.write_bytes(texture_bytes)
        yield fbx_path, texture_path, {
            "archive": {
                "filename": archive_path.name,
                "bytes": archive_path.stat().st_size,
                "sha256": archive_hash,
            },
            "nested_archive": {
                "member": "source/skeleton_psx_fbx.zip",
                "bytes": len(nested_bytes),
                "sha256": NESTED_ARCHIVE_SHA256,
            },
            "reviewed_members": [
                {
                    "path": "skeleton.fbx",
                    "bytes": len(fbx_bytes),
                    "sha256": SOURCE_FBX_SHA256,
                },
                {
                    "path": "skeleton_d.png",
                    "bytes": len(texture_bytes),
                    "sha256": SOURCE_TEXTURE_SHA256,
                },
            ],
        }


def read_glb_document(path: Path) -> dict[str, Any]:
    data = path.read_bytes()
    if len(data) < 20:
        raise ValueError(f"truncated GLB: {path}")
    magic, version, declared_length = struct.unpack_from("<4sII", data, 0)
    if magic != b"glTF" or version != 2 or declared_length != len(data):
        raise ValueError(f"invalid GLB 2.0 header: {path}")
    offset = 12
    chunks: list[tuple[int, bytes]] = []
    while offset < len(data):
        if offset + 8 > len(data):
            raise ValueError("truncated GLB chunk header")
        length, kind = struct.unpack_from("<II", data, offset)
        offset += 8
        end = offset + length
        if end > len(data):
            raise ValueError("truncated GLB chunk body")
        chunks.append((kind, data[offset:end]))
        offset = end
    if offset != len(data) or not chunks or chunks[0][0] != 0x4E4F534A:
        raise ValueError("GLB has no leading JSON chunk")
    if len(chunks) != 2 or chunks[1][0] != 0x004E4942:
        raise ValueError("skeleton GLB must contain exactly embedded JSON and BIN chunks")
    return json.loads(chunks[0][1].rstrip(b" \t\r\n\0").decode("utf-8"))


def _primitives(document: dict[str, Any]) -> list[dict[str, Any]]:
    return [primitive for mesh in document.get("meshes", []) for primitive in mesh.get("primitives", [])]


def _triangle_count(document: dict[str, Any]) -> int:
    total = 0
    accessors = document.get("accessors", [])
    for primitive in _primitives(document):
        if int(primitive.get("mode", 4)) != 4:
            raise ValueError("skeleton GLB contains a non-triangle primitive")
        index = primitive.get("indices")
        if index is None or not 0 <= int(index) < len(accessors):
            raise ValueError("skeleton GLB primitive has no valid index accessor")
        count = int(accessors[int(index)].get("count", 0))
        if count <= 0 or count % 3:
            raise ValueError("skeleton GLB index count is not a positive triangle count")
        total += count // 3
    return total


def _bounds(document: dict[str, Any]) -> tuple[list[float], list[float]]:
    accessors = document.get("accessors", [])
    minima = []
    maxima = []
    for primitive in _primitives(document):
        position = primitive.get("attributes", {}).get("POSITION")
        if position is None or not 0 <= int(position) < len(accessors):
            raise ValueError("skeleton GLB primitive has no valid POSITION accessor")
        accessor = accessors[int(position)]
        minimum, maximum = accessor.get("min"), accessor.get("max")
        if not isinstance(minimum, list) or not isinstance(maximum, list):
            raise ValueError("skeleton GLB POSITION accessor has no bounds")
        if len(minimum) != 3 or len(maximum) != 3:
            raise ValueError("skeleton GLB POSITION bounds are not three-dimensional")
        minima.append([float(value) for value in minimum])
        maxima.append([float(value) for value in maximum])
    if not minima:
        raise ValueError("skeleton GLB contains no mesh primitive")
    return (
        [min(values[axis] for values in minima) for axis in range(3)],
        [max(values[axis] for values in maxima) for axis in range(3)],
    )


def _round_vector(values: Iterable[float]) -> list[float]:
    return [round(float(value), 6) for value in values]


def validate_glb(path: Path, *, rigged: bool) -> dict[str, Any]:
    document = read_glb_document(path)
    primitives = _primitives(document)
    if len(document.get("meshes", [])) != 1 or len(primitives) != 1:
        raise ValueError("skeleton GLB must contain one mesh and one primitive")
    if _triangle_count(document) != SOURCE_TRIANGLES:
        raise ValueError("skeleton GLB did not preserve the reviewed 1,596 triangles")
    primitive = primitives[0]
    required = {"POSITION", "NORMAL", "TEXCOORD_0"}
    if not required.issubset(primitive.get("attributes", {})):
        raise ValueError("skeleton GLB primitive lacks position, normal, or UV attributes")
    materials = document.get("materials", [])
    if len(materials) != 1 or primitive.get("material") != 0:
        raise ValueError("skeleton GLB must use exactly one material")
    pbr = materials[0].get("pbrMetallicRoughness", {})
    texture_info = pbr.get("baseColorTexture")
    if not isinstance(texture_info, dict):
        raise ValueError("skeleton GLB material has no base-color texture")
    textures = document.get("textures", [])
    texture_index = int(texture_info.get("index", -1))
    if not 0 <= texture_index < len(textures):
        raise ValueError("skeleton GLB base-color texture index is invalid")
    image_index = int(textures[texture_index].get("source", -1))
    images = document.get("images", [])
    if len(images) != 1 or not 0 <= image_index < len(images):
        raise ValueError("skeleton GLB must contain one base-color image")
    if "uri" in images[image_index] or "bufferView" not in images[image_index]:
        raise ValueError("skeleton GLB base-color image is not embedded")
    for buffer in document.get("buffers", []):
        if "uri" in buffer:
            raise ValueError("skeleton GLB contains an external buffer")
    if document.get("animations"):
        raise ValueError("reviewed source has no animation; output must not imply one")

    attributes = primitive.get("attributes", {})
    skins = document.get("skins", [])
    if rigged:
        if len(skins) != 1 or len(skins[0].get("joints", [])) != len(ARMATURE_BONES):
            raise ValueError("rigged skeleton master did not retain its 27-joint native skin")
        if not {"JOINTS_0", "WEIGHTS_0"}.issubset(attributes):
            raise ValueError("rigged skeleton master did not retain skin weights")
    elif skins or "JOINTS_0" in attributes or "WEIGHTS_0" in attributes:
        raise ValueError("static skeleton runtime GLB unexpectedly contains skinning data")

    minimum, maximum = _bounds(document)
    height = maximum[1] - minimum[1]
    if abs(minimum[1]) > 2.0e-5 or abs(height - TARGET_HEIGHT_M) > 2.0e-4:
        raise ValueError(
            f"skeleton output is not grounded at glTF Y=0 and {TARGET_HEIGHT_M:.2f} m tall: "
            f"min={minimum}, max={maximum}"
        )
    return {
        "triangles": SOURCE_TRIANGLES,
        "primitive_count": 1,
        "material_count": 1,
        "embedded_image_count": 1,
        "skin_count": len(skins),
        "joint_count": len(skins[0].get("joints", [])) if skins else 0,
        "bounds_min_m": _round_vector(minimum),
        "bounds_max_m": _round_vector(maximum),
        "dimensions_m": _round_vector(maximum[axis] - minimum[axis] for axis in range(3)),
    }


def classify_relaxed_point(point: Sequence[float]) -> str:
    """Python snapshot of ``CrewBodyPart::classify_relaxed_mesh_point``."""

    x, y, _z = (float(value) for value in point)
    if y >= 1.275:
        return "Head"
    if x < -0.155 and y >= 0.39:
        return "LeftUpperArm" if y >= 0.79 else "LeftForearm"
    if x > 0.155 and y >= 0.39:
        return "RightUpperArm" if y >= 0.79 else "RightForearm"
    if y < 0.67:
        if y >= 0.315:
            return "LeftThigh" if x < 0.0 else "RightThigh"
        return "LeftShin" if x < 0.0 else "RightShin"
    return "Pelvis" if y < 0.835 else "Torso"


def verify_bake_receipt(asset_root: Path, report: dict[str, Any]) -> dict[str, Any]:
    asset_root = asset_root.expanduser().resolve()
    if report.get("schema") != "kessel.skeleton-character-bake" or report.get("version") != 1:
        raise ValueError("unsupported skeleton bake receipt")
    source = report.get("source", {})
    expected_source = {
        "title": TITLE,
        "creator": CREATOR,
        "sketchfab_uid": SKETCHFAB_UID,
        "source_url": SOURCE_URL,
        "license": LICENSE_SPDX,
        "archive_sha256": ARCHIVE_SHA256,
    }
    for key, expected in expected_source.items():
        if source.get(key) != expected:
            raise ValueError(f"skeleton source provenance changed at {key}")
    if source.get("private_source_receipt") != expected_private_source_receipt():
        raise ValueError("skeleton private source file hashes do not match reviewed inputs")
    toolchain = report.get("toolchain", {})
    verified_tools = verify_toolchain_receipt(toolchain.get("files"))

    rigged_path = verify_file_record(asset_root, report["rigged_master"])
    static_path = verify_file_record(asset_root, report["runtime_static"])
    alignment_path = verify_file_record(asset_root, report["alignment_manifest"])
    rigged_summary = validate_glb(rigged_path, rigged=True)
    static_summary = validate_glb(static_path, rigged=False)
    for summary, record in ((rigged_summary, report["rigged_master"]), (static_summary, report["runtime_static"])):
        if summary["bounds_min_m"] != record.get("bounds_min_m"):
            raise ValueError(f"stale minimum bounds receipt for {record['path']}")
        if summary["bounds_max_m"] != record.get("bounds_max_m"):
            raise ValueError(f"stale maximum bounds receipt for {record['path']}")

    alignment = json.loads(alignment_path.read_text(encoding="utf-8"))
    if alignment.get("schema") != "kessel.skeleton-anatomy-alignment" or alignment.get("version") != 1:
        raise ValueError("unsupported skeleton anatomy alignment manifest")
    partition = alignment.get("triangle_centroid_partition", {})
    counts = partition.get("counts", {})
    if tuple(sorted(counts)) != tuple(sorted(BODY_PARTS)):
        raise ValueError("skeleton anatomy manifest does not cover all eleven body parts")
    if any(int(counts[name]) <= 0 for name in BODY_PARTS):
        raise ValueError("skeleton anatomy manifest has an empty body-part partition")
    if sum(int(counts[name]) for name in BODY_PARTS) != SOURCE_TRIANGLES:
        raise ValueError("skeleton anatomy partitions do not conserve all source triangles")
    if alignment.get("registration_claim") != "measured-approximate-not-exact":
        raise ValueError("skeleton anatomy manifest overstates registration fidelity")

    actual_glbs = {
        path.relative_to(asset_root).as_posix()
        for directory in (asset_root / "authoring", asset_root / "baked")
        if directory.is_dir()
        for path in directory.glob("*.glb")
    }
    expected_glbs = {RIGGED_RELATIVE, STATIC_RELATIVE}
    if actual_glbs != expected_glbs:
        raise ValueError(
            f"unexpected or missing skeleton GLBs: expected {sorted(expected_glbs)}, "
            f"got {sorted(actual_glbs)}"
        )
    return {
        "schema": "kessel.skeleton-character-bake-verification",
        "version": 1,
        "status": "PASS",
        "rigged_master": rigged_summary,
        "runtime_static": static_summary,
        "partition_triangles": {name: int(counts[name]) for name in BODY_PARTS},
        "maximum_landmark_error_m": alignment.get("landmark_alignment", {}).get(
            "maximum_error_m"
        ),
        "verified_tool_files": sorted(verified_tools),
    }
