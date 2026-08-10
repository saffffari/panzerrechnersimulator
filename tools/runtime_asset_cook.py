#!/usr/bin/env python3
"""Deterministic, offline GLB catalog validation and model cooking for Kessel.

The source library remains untouched. ``bootstrap`` records the audited runtime model closure in
the tracked semantic catalog, ``validate`` verifies that lock against source bytes and geometry,
and ``cook`` copies only those canonical GLBs into a fresh flat model tree with reproducible
manifests. This is not a runnable game-package cook: terrain, audio, fonts, gameplay data and the
engine's source-path resolver remain separate. No command scans archives, downloads data, or
rewrites a source asset.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import struct
import sys
from typing import Any, Iterable


CATALOG_SCHEMA = "kessel.asset-catalog"
CATALOG_VERSION = 1
RUNTIME_SCHEMA = "kessel.runtime-assets"
RUNTIME_VERSION = 1
DEFAULT_CATALOG = Path("catalog/runtime_assets.v1.json")

ID_RE = re.compile(r"^[a-z0-9]+(?:[._-][a-z0-9]+)*$")
FORBIDDEN_RUNTIME_EXTENSIONS = {
    ".zip",
    ".rar",
    ".7z",
    ".fbx",
    ".obj",
    ".dae",
    ".mtl",
    ".tif",
    ".tiff",
    ".blend",
    ".blend1",
}
PREVIEW_TOKENS = {"preview", "previews", "_previews", "thumb", "thumbs", ".thumbs"}
ALLOWED_COOK_EXTENSIONS = {".glb", ".json", ".tsv", ".sha256"}

PIZZA_PREFIXES = {
    "retro_fps": (
        "assets/pizza/_extracted/Modular Retro FPS Kit v1.5/"
        "Modular Retro FPS Kit/Models/other-formats/GLB/"
    ),
    "mega1": (
        "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/"
        "Models/GLB (recommended)/"
    ),
    "mega2": (
        "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/"
        "Models/GLB (recommended)/"
    ),
    "nature": (
        "assets/pizza/_extracted/PSX Nature v1.7.3/"
        "PSX Nature (Tree branches separated)/Models/GLB (recommended)/"
    ),
}

# GLBs loaded by the normal game world in addition to each act manifest. Keeping this audited list
# explicit makes changes reviewable and prevents the authoring-only 4.6 GB Pizza library from
# leaking into runtime output.
LIVE_GAME_PIZZA = {
    "mega1": [
        "Items & Weapons/ammo_box_1.glb",
        "Items & Weapons/cigs_packet_1.glb",
        "Large Props/metal_barrel_mp_1.glb",
        "Large Props/metal_barrel_mp_2.glb",
        "Large Props/supply_crate_1.glb",
        "Large Props/wooden_crate_1.glb",
        "Large Props/wooden_crate_3.glb",
        "Modular Structures/arc_1_wall_1.glb",
        "Modular Structures/doorway_1.glb",
        "Modular Structures/pillar_1.glb",
        "Modular Structures/wall_1.glb",
        "Modular Structures/wall_1_tunnel_hole_1.glb",
        "Modular Structures/wall_1_tunnel_hole_2.glb",
    ],
    "mega2": [
        "Buildings/garage_hl_1.glb",
        "Buildings/warehouse_mx_1.glb",
        "Debris & Misc/brick_mx_1.glb",
        "Debris & Misc/bricks_stacked_mx_1.glb",
        "Debris & Misc/bricks_stacked_mx_2.glb",
        "Debris & Misc/bricks_stacked_mx_3.glb",
        "Debris & Misc/bricks_stacked_mx_4.glb",
        "Debris & Misc/debris_bricks_mx_1.glb",
        "Debris & Misc/debris_bricks_mx_2.glb",
        "Large Props & Machinery/chimney_a_1.glb",
        "Large Props & Machinery/shipping_container_mx_1.glb",
        "Modular Props/barricade_a_1.glb",
        "Modular Props/barricade_a_2.glb",
        "Modular Props/barricade_b_1.glb",
        "Modular Props/barricade_b_3.glb",
        "Modular Props/barricade_b_4.glb",
        "Modular Structures/doorway_hr_1.glb",
        "Modular Structures/floor_ceiling_hr_1.glb",
        "Modular Structures/pillar_hr_1_broken.glb",
        "Modular Structures/pillar_hr_2.glb",
        "Modular Structures/roof_bx_1.glb",
        "Modular Structures/roof_hr_3_middle_angle.glb",
        "Modular Structures/roof_mh_1.glb",
        "Modular Structures/roof_mh_2.glb",
        "Modular Structures/roof_mh_4_a_1.glb",
        "Modular Structures/wall_hr_1.glb",
        "Modular Structures/wall_hr_1_hole_1.glb",
        "Modular Structures/wall_hr_1_m1a_1.glb",
        "Modular Structures/window_hr_1.glb",
        "Props/toolbox_mx_1.glb",
        "Weapons & Tools/hammer_mx_1.glb",
    ],
    "nature": ["stone_3.glb", "stone_5.glb", "tree_log_1.glb"],
}

NON_PIZZA_RUNTIME_GLOBS = [
    "assets/88mm/baked/*.glb",
    "assets/flavor/**/*.glb",
    "assets/tanks/T34-76/baked/*.glb",
    "assets/tanks/tiger/baked/*.glb",
    "assets/tanks/tiger/baked/half/*.glb",
    "assets/tanks/wrecks/*.glb",
]

LICENSES = {
    "kessel-provenance-unrecorded": {
        "name": "Kessel local project asset",
        "status": "provenance-not-recorded-in-repository",
        "redistribution": "review-required",
    },
    "pizzadoggy-local-purchase": {
        "name": "Pizza Doggy local purchased asset",
        "status": "verified-for-use-in-a-game-do-not-redistribute-standalone",
        "redistribution": "game-integration-only",
        "source_reference": "docs/THIRD_PARTY_ASSETS.md",
    },
}


class CatalogError(RuntimeError):
    """A deterministic validation failure with a user-facing message."""


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n").encode("utf-8")


def sha256_bytes(blob: bytes) -> str:
    return hashlib.sha256(blob).hexdigest()


def safe_repo_path(raw: str, field: str = "path") -> PurePosixPath:
    if "\\" in raw:
        raise CatalogError(f"{field} must use '/' separators: {raw!r}")
    path = PurePosixPath(raw)
    if path.is_absolute() or not path.parts or any(part in {"", ".", ".."} for part in path.parts):
        raise CatalogError(f"{field} must be a normalized repository-relative path: {raw!r}")
    return path


def path_has_preview_token(path: PurePosixPath) -> bool:
    for part in path.parts:
        stem = PurePosixPath(part).stem.casefold()
        pieces = {piece for piece in re.split(r"[^a-z0-9_.]+", stem) if piece}
        if stem in PREVIEW_TOKENS or pieces & PREVIEW_TOKENS:
            return True
        if stem.startswith("preview_") or stem.startswith("thumb_"):
            return True
    return False


def reject_noncanonical_source(path: PurePosixPath) -> None:
    suffix = path.suffix.casefold()
    blender_backup = suffix.startswith(".blend") and suffix.removeprefix(".blend").isdigit()
    if suffix in FORBIDDEN_RUNTIME_EXTENSIONS or blender_backup:
        raise CatalogError(f"forbidden raw runtime input type {suffix}: {path}")
    if suffix != ".glb":
        raise CatalogError(f"runtime inputs must be canonical .glb files: {path}")
    if path_has_preview_token(path):
        raise CatalogError(f"preview/thumbnail asset is forbidden in runtime output: {path}")


def decode_uri(uri: str) -> bytes | None:
    if not uri.startswith("data:"):
        return None
    try:
        header, encoded = uri.split(",", 1)
    except ValueError as error:
        raise CatalogError("malformed data URI in GLB") from error
    if ";base64" not in header:
        raise CatalogError("only base64 data URIs are supported in canonical GLBs")
    try:
        return base64.b64decode(encoded, validate=True)
    except ValueError as error:
        raise CatalogError("invalid base64 data URI in GLB") from error


def matrix_identity() -> list[list[float]]:
    return [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]


def matrix_mul(a: list[list[float]], b: list[list[float]]) -> list[list[float]]:
    return [[sum(a[r][k] * b[k][c] for k in range(4)) for c in range(4)] for r in range(4)]


def node_matrix(node: dict[str, Any]) -> list[list[float]]:
    if "matrix" in node:
        values = node["matrix"]
        if not isinstance(values, list) or len(values) != 16:
            raise CatalogError("node.matrix must contain 16 numbers")
        return [[float(values[c * 4 + r]) for c in range(4)] for r in range(4)]

    tx, ty, tz = (float(v) for v in node.get("translation", [0.0, 0.0, 0.0]))
    sx, sy, sz = (float(v) for v in node.get("scale", [1.0, 1.0, 1.0]))
    x, y, z, w = (float(v) for v in node.get("rotation", [0.0, 0.0, 0.0, 1.0]))
    length = math.sqrt(x * x + y * y + z * z + w * w)
    if length == 0.0:
        raise CatalogError("node rotation quaternion has zero length")
    x, y, z, w = x / length, y / length, z / length, w / length
    xx, yy, zz = x * x, y * y, z * z
    xy, xz, yz = x * y, x * z, y * z
    wx, wy, wz = w * x, w * y, w * z
    # glTF local transform is T * R * S.
    return [
        [(1.0 - 2.0 * (yy + zz)) * sx, (2.0 * (xy - wz)) * sy, (2.0 * (xz + wy)) * sz, tx],
        [(2.0 * (xy + wz)) * sx, (1.0 - 2.0 * (xx + zz)) * sy, (2.0 * (yz - wx)) * sz, ty],
        [(2.0 * (xz - wy)) * sx, (2.0 * (yz + wx)) * sy, (1.0 - 2.0 * (xx + yy)) * sz, tz],
        [0.0, 0.0, 0.0, 1.0],
    ]


def transform_point(matrix: list[list[float]], point: Iterable[float]) -> list[float]:
    x, y, z = point
    out = [
        matrix[r][0] * x + matrix[r][1] * y + matrix[r][2] * z + matrix[r][3]
        for r in range(3)
    ]
    if not all(math.isfinite(v) for v in out):
        raise CatalogError("GLB transform produced a non-finite vertex")
    return out


COMPONENTS: dict[int, tuple[str, int, bool]] = {
    5120: ("b", 1, True),
    5121: ("B", 1, False),
    5122: ("h", 2, True),
    5123: ("H", 2, False),
    5125: ("I", 4, False),
    5126: ("f", 4, True),
}


def component_to_float(value: int | float, component_type: int, normalized: bool) -> float:
    if component_type == 5126 or not normalized:
        return float(value)
    _, size, signed = COMPONENTS[component_type]
    bits = size * 8
    if signed:
        return max(float(value) / float((1 << (bits - 1)) - 1), -1.0)
    return float(value) / float((1 << bits) - 1)


def read_accessor_vec3(
    document: dict[str, Any], buffers: list[bytes], accessor_index: int
) -> list[list[float]]:
    try:
        accessor = document["accessors"][accessor_index]
    except (KeyError, IndexError, TypeError) as error:
        raise CatalogError(f"invalid POSITION accessor {accessor_index}") from error
    if accessor.get("type") != "VEC3":
        raise CatalogError("POSITION accessor must be VEC3")
    if "sparse" in accessor:
        raise CatalogError("sparse POSITION accessors are not supported by the runtime cook")
    component_type = accessor.get("componentType")
    if component_type not in COMPONENTS:
        raise CatalogError(f"unsupported POSITION componentType {component_type!r}")
    try:
        view = document["bufferViews"][accessor["bufferView"]]
        blob = buffers[view.get("buffer", 0)]
    except (KeyError, IndexError, TypeError) as error:
        raise CatalogError("POSITION accessor references an invalid bufferView/buffer") from error
    code, width, _signed = COMPONENTS[component_type]
    item_width = width * 3
    stride = int(view.get("byteStride", item_width))
    if stride < item_width:
        raise CatalogError("POSITION byteStride is smaller than the element")
    offset = int(view.get("byteOffset", 0)) + int(accessor.get("byteOffset", 0))
    count = int(accessor.get("count", -1))
    if count < 1:
        raise CatalogError("POSITION accessor must contain at least one vertex")
    normalized = bool(accessor.get("normalized", False))
    unpack = struct.Struct("<" + code * 3)
    result: list[list[float]] = []
    for index in range(count):
        start = offset + index * stride
        if start < 0 or start + item_width > len(blob):
            raise CatalogError("POSITION accessor exceeds its buffer")
        raw = unpack.unpack_from(blob, start)
        result.append(
            [component_to_float(v, component_type, normalized) for v in raw]
        )
    return result


def parse_glb(blob: bytes, source: str) -> dict[str, Any]:
    if len(blob) < 20:
        raise CatalogError(f"{source}: truncated GLB header")
    magic, version, declared_length = struct.unpack_from("<4sII", blob, 0)
    if magic != b"glTF" or version != 2:
        raise CatalogError(f"{source}: expected a GLB 2.0 header")
    if declared_length != len(blob):
        raise CatalogError(
            f"{source}: GLB header length {declared_length} does not match {len(blob)} bytes"
        )
    chunks: list[tuple[int, bytes]] = []
    cursor = 12
    while cursor < len(blob):
        if cursor + 8 > len(blob):
            raise CatalogError(f"{source}: truncated GLB chunk header")
        size, kind = struct.unpack_from("<II", blob, cursor)
        cursor += 8
        end = cursor + size
        if end > len(blob):
            raise CatalogError(f"{source}: GLB chunk exceeds the file")
        chunks.append((kind, blob[cursor:end]))
        cursor = end
    json_chunks = [payload for kind, payload in chunks if kind == 0x4E4F534A]
    bin_chunks = [payload for kind, payload in chunks if kind == 0x004E4942]
    if len(json_chunks) != 1:
        raise CatalogError(f"{source}: GLB must contain exactly one JSON chunk")
    try:
        document = json.loads(json_chunks[0].rstrip(b"\x00 \t\r\n"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise CatalogError(f"{source}: invalid GLB JSON chunk") from error
    if str(document.get("asset", {}).get("version")) != "2.0":
        raise CatalogError(f"{source}: GLB JSON does not declare glTF 2.0")

    dependencies: list[str] = []
    buffers: list[bytes] = []
    embedded_bin = bin_chunks[0] if bin_chunks else b""
    for index, buffer in enumerate(document.get("buffers", [])):
        uri = buffer.get("uri")
        if uri is None:
            if index != 0 or not bin_chunks:
                raise CatalogError(f"{source}: buffer {index} has no embedded BIN chunk")
            data = embedded_bin
        else:
            data = decode_uri(str(uri))
            if data is None:
                dependencies.append(str(uri))
                data = b""
        expected = int(buffer.get("byteLength", -1))
        if expected < 0 or (data and len(data) < expected):
            raise CatalogError(f"{source}: buffer {index} is shorter than byteLength")
        buffers.append(data)
    for image in document.get("images", []):
        if "uri" in image and decode_uri(str(image["uri"])) is None:
            dependencies.append(str(image["uri"]))
    dependencies = sorted(set(dependencies))
    if dependencies:
        return {"document": document, "dependencies": dependencies, "bounds": None}

    scenes = document.get("scenes", [])
    scene_index = int(document.get("scene", 0)) if scenes else -1
    if scene_index < 0 or scene_index >= len(scenes):
        raise CatalogError(f"{source}: GLB has no valid default scene")
    nodes = document.get("nodes", [])
    meshes = document.get("meshes", [])
    minimum = [math.inf, math.inf, math.inf]
    maximum = [-math.inf, -math.inf, -math.inf]
    vertex_count = 0

    def walk(node_index: int, parent: list[list[float]], ancestors: set[int]) -> None:
        nonlocal vertex_count
        if node_index in ancestors:
            raise CatalogError(f"{source}: cycle in GLB node graph")
        if node_index < 0 or node_index >= len(nodes):
            raise CatalogError(f"{source}: invalid scene node index {node_index}")
        node = nodes[node_index]
        world = matrix_mul(parent, node_matrix(node))
        if "mesh" in node:
            mesh_index = int(node["mesh"])
            if mesh_index < 0 or mesh_index >= len(meshes):
                raise CatalogError(f"{source}: invalid mesh index {mesh_index}")
            for primitive in meshes[mesh_index].get("primitives", []):
                position_index = primitive.get("attributes", {}).get("POSITION")
                if position_index is None:
                    continue
                for point in read_accessor_vec3(document, buffers, int(position_index)):
                    transformed = transform_point(world, point)
                    for axis in range(3):
                        minimum[axis] = min(minimum[axis], transformed[axis])
                        maximum[axis] = max(maximum[axis], transformed[axis])
                    vertex_count += 1
        next_ancestors = ancestors | {node_index}
        for child in node.get("children", []):
            walk(int(child), world, next_ancestors)

    for root in scenes[scene_index].get("nodes", []):
        walk(int(root), matrix_identity(), set())
    if vertex_count == 0:
        raise CatalogError(f"{source}: default scene contains no POSITION geometry")
    return {
        "document": document,
        "dependencies": dependencies,
        "bounds": {"min": minimum, "max": maximum, "vertices": vertex_count},
    }


def rounded_vector(values: Iterable[float]) -> list[float]:
    result = []
    for value in values:
        rounded = round(float(value), 6)
        result.append(0.0 if rounded == 0.0 else rounded)
    return result


def slug(value: str) -> str:
    clean = re.sub(r"[^a-z0-9]+", "_", value.casefold()).strip("_")
    if not clean:
        raise CatalogError(f"cannot derive stable id from {value!r}")
    return clean


def pizza_family(path: str) -> tuple[str, str] | None:
    for family, prefix in PIZZA_PREFIXES.items():
        if path.startswith(prefix):
            return family, path[len(prefix) :]
    return None


def stable_id(path: str) -> str:
    family = pizza_family(path)
    source = PurePosixPath(path)
    if family:
        pack, relative = family
        rel = PurePosixPath(relative)
        category = slug(rel.parts[0]) if len(rel.parts) > 1 else "asset"
        return f"pizza.{pack}.{category}.{slug(rel.stem)}"
    parts = source.parts
    if path.startswith("assets/tanks/tiger/baked/half/"):
        return f"tank.tiger.half.{slug(source.stem.removeprefix('tiger_'))}"
    if path.startswith("assets/tanks/tiger/baked/"):
        return f"tank.tiger.full.{slug(source.stem.removeprefix('tiger_'))}"
    if path.startswith("assets/tanks/T34-76/baked/"):
        return f"tank.t34_76.{slug(source.stem.removeprefix('t34_'))}"
    if path.startswith("assets/tanks/wrecks/"):
        return f"wreck.{slug(source.stem)}"
    if path.startswith("assets/88mm/baked/"):
        return f"ordnance.88mm.{slug(source.stem.removeprefix('shell_'))}"
    if "deadman" in path.casefold():
        return "flavor.deadman.prone"
    if source.name == "BrickBuilding.glb":
        return "flavor.building.brick"
    if source.name == "IndustrialBuilding.glb":
        return "flavor.building.industrial"
    if source.name == "luger.glb":
        return "flavor.weapon.luger"
    if source.name == "extinguisher.glb":
        return "flavor.equipment.extinguisher"
    return "runtime." + ".".join(slug(part) for part in parts[1:-1] + (source.stem,))


def semantic_policy(path: str) -> dict[str, Any]:
    lower = path.casefold()
    family = pizza_family(path)
    tags: set[str] = {"glb", "runtime"}
    source_family = "kessel-local"
    license_id = "kessel-provenance-unrecorded"
    if family:
        source_family = "pizzadoggy-" + family[0]
        license_id = "pizzadoggy-local-purchase"
        tags.update({"pizza", family[0]})

    if any(token in lower for token in ("wall", "doorway", "window", "floor", "roof", "pillar")):
        tags.update({"environment", "structure", "modular"})
        collision, destruction, distance = "static_mesh", "masonry", 1400.0
    elif any(token in lower for token in ("building", "warehouse", "garage", "shed")):
        tags.update({"environment", "structure", "building"})
        collision, destruction, distance = "static_mesh", "building", 1800.0
    elif any(token in lower for token in ("tree", "pine")):
        tags.update({"environment", "vegetation", "tree"})
        collision, destruction, distance = "trunk", "foliage", 900.0
    elif any(token in lower for token in ("grass", "fern")):
        tags.update({"environment", "vegetation", "ground-cover"})
        collision, destruction, distance = "none", "none", 350.0
    elif any(token in lower for token in ("stone", "brick", "rubble", "debris", "stump", "log")):
        tags.update({"environment", "debris"})
        collision, destruction, distance = "simple_bounds", "debris", 500.0
    elif any(token in lower for token in ("tiger", "t34-76")) and "/wrecks/" not in lower:
        tags.update({"vehicle", "tank", "rig-part"})
        collision, destruction, distance = "vehicle_part", "vehicle_module", 2400.0
    elif "/wrecks/" in lower:
        tags.update({"vehicle", "wreck"})
        collision, destruction, distance = "vehicle_hull", "wreck", 1600.0
    elif "/88mm/" in lower:
        tags.update({"ordnance", "projectile"})
        collision, destruction, distance = "projectile", "none", 180.0
    elif "deadman" in lower:
        tags.update({"character", "corpse", "set-dressing"})
        collision, destruction, distance = "none", "none", 450.0
    elif "luger" in lower:
        tags.update({"weapon", "sidearm"})
        collision, destruction, distance = "held_item", "none", 160.0
    elif "extinguisher" in lower:
        tags.update({"equipment", "fire-suppression"})
        collision, destruction, distance = "held_item", "prop", 160.0
    elif "furniture" in lower:
        tags.update({"interior", "furniture", "prop"})
        collision, destruction, distance = "simple_bounds", "prop", 350.0
    elif any(token in lower for token in ("crate", "barrel", "container", "barricade", "fence")):
        tags.update({"environment", "prop", "cover"})
        collision, destruction, distance = "simple_bounds", "prop", 750.0
    else:
        tags.update({"environment", "prop"})
        collision, destruction, distance = "simple_bounds", "prop", 600.0

    if "/half/" in lower:
        tags.add("half-resolution")
    if "baked" in lower:
        tags.add("baked")
    return {
        "tags": sorted(tags),
        "source_family": source_family,
        "license": license_id,
        "collision_class": collision,
        "destruction_class": destruction,
        "render_distance": distance,
    }


def candidate_sources(repo_root: Path) -> tuple[dict[str, set[str]], dict[str, int]]:
    required_by: dict[str, set[str]] = {}

    def add(path: str, consumer: str) -> None:
        safe_repo_path(path, "source")
        if not path.casefold().endswith(".glb"):
            return
        required_by.setdefault(path, set()).add(consumer)

    manifest_union: set[str] = set()
    for act_id in (1, 2, 3):
        manifest_path = repo_root / f"assets/worldgen/act{act_id}/manifest.v1.json"
        try:
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise CatalogError(f"cannot read canonical act manifest {manifest_path}: {error}") from error
        for raw in manifest.get("assets", []):
            if not isinstance(raw, str):
                raise CatalogError(f"{manifest_path}: every assets entry must be a string")
            if raw.casefold().endswith(".glb"):
                add(raw, f"act{act_id}.manifest")
                manifest_union.add(raw)

        # The worldbuilder's overlay is authored runtime state, not disposable local scratch.
        # Include added assets so a model placed in the editor cannot disappear from a later cook.
        edits_path = manifest_path.with_name("manifest_edits.json")
        if edits_path.is_file():
            try:
                edits = json.loads(edits_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as error:
                raise CatalogError(f"cannot read manifest edits {edits_path}: {error}") from error
            ops = edits.get("ops")
            if not isinstance(ops, list):
                raise CatalogError(f"{edits_path}: ops must be an array")
            for index, op in enumerate(ops):
                if not isinstance(op, dict):
                    raise CatalogError(f"{edits_path}: ops[{index}] must be an object")
                if op.get("op") != "add":
                    continue
                raw = op.get("asset")
                if not isinstance(raw, str):
                    raise CatalogError(f"{edits_path}: ops[{index}].asset must be a string")
                if raw.casefold().endswith(".glb"):
                    add(raw, f"act{act_id}.manifest-edits")
                    manifest_union.add(raw)

    for family, relatives in LIVE_GAME_PIZZA.items():
        prefix = PIZZA_PREFIXES[family]
        for relative in relatives:
            add(prefix + relative, "game.static-world")

    for pattern in NON_PIZZA_RUNTIME_GLOBS:
        for source in sorted(repo_root.glob(pattern)):
            if source.is_file():
                add(source.relative_to(repo_root).as_posix(), "runtime.non-pizza")

    counts = {
        "campaign_manifest_unique_glbs": len(manifest_union),
        "campaign_manifest_pizza_glbs": sum(p.startswith("assets/pizza/") for p in manifest_union),
        "pizza_glbs": sum(p.startswith("assets/pizza/") for p in required_by),
        "non_pizza_glbs": sum(not p.startswith("assets/pizza/") for p in required_by),
        "total_glbs": len(required_by),
    }
    return required_by, counts


def catalog_entry(repo_root: Path, source: str, required_by: Iterable[str]) -> dict[str, Any]:
    rel = safe_repo_path(source, "source")
    reject_noncanonical_source(rel)
    path = repo_root.joinpath(*rel.parts)
    if not path.is_file():
        raise CatalogError(f"required runtime source is missing: {source}")
    if path.is_symlink():
        raise CatalogError(f"runtime source may not be a symlink: {source}")
    blob = path.read_bytes()
    parsed = parse_glb(blob, source)
    dependencies = parsed["dependencies"]
    if dependencies:
        raise CatalogError(f"{source}: canonical GLB has external dependencies: {dependencies}")
    raw_bounds = parsed["bounds"]
    minimum = raw_bounds["min"]
    maximum = raw_bounds["max"]
    dimensions = [maximum[i] - minimum[i] for i in range(3)]
    center = [(minimum[i] + maximum[i]) * 0.5 for i in range(3)]
    policy = semantic_policy(source)
    asset_id = stable_id(source)
    return {
        "bounds_m": {
            "max": rounded_vector(maximum),
            "min": rounded_vector(minimum),
        },
        "canonical": {
            "byte_preserving": True,
            "format": "glb-2.0",
            "runtime_path": f"models/{asset_id}.glb",
        },
        "collision_class": policy["collision_class"],
        "destruction_class": policy["destruction_class"],
        "dimensions_m": rounded_vector(dimensions),
        "expected": {
            "bytes": len(blob),
            "sha256": sha256_bytes(blob),
            "vertices": int(raw_bounds["vertices"]),
        },
        "external_dependencies": dependencies,
        "id": asset_id,
        "license": policy["license"],
        "origin": {
            "normalization_hook": "identity",
            "policy": "source-authored",
            "source_origin_m": [0.0, 0.0, 0.0],
            "to_bounds_center_m": rounded_vector(center),
            "height_above_bounds_min_m": round(max(0.0, -minimum[1]), 6),
        },
        "render": {
            "lod_hook": {
                "fallback": "cull",
                "group": asset_id,
                "levels": [{"asset_id": asset_id, "distance_m": 0.0}],
            },
            "range_m": [0.0, policy["render_distance"]],
        },
        "required_by": sorted(set(required_by)),
        "source": source,
        "source_family": policy["source_family"],
        "tags": policy["tags"],
    }


def build_catalog(repo_root: Path) -> dict[str, Any]:
    sources, counts = candidate_sources(repo_root)
    entries = [catalog_entry(repo_root, path, sources[path]) for path in sorted(sources)]
    entries.sort(key=lambda entry: entry["id"])
    ids = [entry["id"] for entry in entries]
    if len(ids) != len(set(ids)):
        duplicates = sorted({asset_id for asset_id in ids if ids.count(asset_id) > 1})
        raise CatalogError(f"stable asset id collision: {duplicates}")
    return {
        "catalog_id": "kessel.runtime-foundation",
        "classes": {
            "collision": [
                "held_item",
                "none",
                "projectile",
                "simple_bounds",
                "static_mesh",
                "trunk",
                "vehicle_hull",
                "vehicle_part",
            ],
            "destruction": [
                "building",
                "debris",
                "foliage",
                "masonry",
                "none",
                "prop",
                "vehicle_module",
                "wreck",
            ],
        },
        "licenses": LICENSES,
        "schema": CATALOG_SCHEMA,
        "scope": counts,
        "version": CATALOG_VERSION,
        "assets": entries,
    }


def load_catalog(catalog_path: Path) -> dict[str, Any]:
    try:
        return json.loads(catalog_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise CatalogError(f"cannot read asset catalog {catalog_path}: {error}") from error


def float_vectors_equal(left: Any, right: Any, tolerance: float = 1e-6) -> bool:
    return (
        isinstance(left, list)
        and isinstance(right, list)
        and len(left) == len(right)
        and all(abs(float(a) - float(b)) <= tolerance for a, b in zip(left, right))
    )


def validate_catalog(
    catalog: dict[str, Any], repo_root: Path, enforce_repo_scope: bool = True
) -> list[dict[str, Any]]:
    if catalog.get("schema") != CATALOG_SCHEMA or catalog.get("version") != CATALOG_VERSION:
        raise CatalogError(
            f"catalog must declare schema={CATALOG_SCHEMA!r}, version={CATALOG_VERSION}"
        )
    licenses = catalog.get("licenses")
    assets = catalog.get("assets")
    if not isinstance(licenses, dict) or not isinstance(assets, list) or not assets:
        raise CatalogError("catalog licenses must be an object and assets a non-empty array")
    ids: set[str] = set()
    sources: set[str] = set()
    runtime_paths: set[str] = set()
    records: list[dict[str, Any]] = []
    previous_id = ""
    for index, entry in enumerate(assets):
        asset_id = entry.get("id")
        source = entry.get("source")
        if not isinstance(asset_id, str) or not ID_RE.fullmatch(asset_id):
            raise CatalogError(f"assets[{index}].id is not stable/canonical: {asset_id!r}")
        if asset_id <= previous_id:
            raise CatalogError("catalog assets must be sorted by unique stable id")
        previous_id = asset_id
        if asset_id in ids:
            raise CatalogError(f"duplicate asset id {asset_id!r}")
        ids.add(asset_id)
        if not isinstance(source, str):
            raise CatalogError(f"{asset_id}: source must be a string")
        rel = safe_repo_path(source, f"{asset_id}.source")
        reject_noncanonical_source(rel)
        if source in sources:
            raise CatalogError(f"duplicate catalog source {source!r}")
        sources.add(source)
        canonical = entry.get("canonical", {})
        runtime_raw = canonical.get("runtime_path")
        if not isinstance(runtime_raw, str):
            raise CatalogError(f"{asset_id}: canonical.runtime_path must be a string")
        runtime_rel = safe_repo_path(runtime_raw, f"{asset_id}.canonical.runtime_path")
        reject_noncanonical_source(runtime_rel)
        if runtime_raw in runtime_paths:
            raise CatalogError(f"duplicate runtime path {runtime_raw!r}")
        runtime_paths.add(runtime_raw)
        if runtime_raw != f"models/{asset_id}.glb":
            raise CatalogError(f"{asset_id}: runtime path must be models/<stable-id>.glb")
        if canonical.get("format") != "glb-2.0" or canonical.get("byte_preserving") is not True:
            raise CatalogError(f"{asset_id}: canonical GLB policy is incomplete")
        if entry.get("license") not in licenses:
            raise CatalogError(f"{asset_id}: unknown license record {entry.get('license')!r}")
        for field in ("tags", "required_by"):
            value = entry.get(field)
            if not isinstance(value, list) or value != sorted(set(value)) or not value:
                raise CatalogError(f"{asset_id}: {field} must be a sorted non-empty unique list")
        collision = entry.get("collision_class")
        destruction = entry.get("destruction_class")
        if collision not in catalog.get("classes", {}).get("collision", []):
            raise CatalogError(f"{asset_id}: unsupported collision class {collision!r}")
        if destruction not in catalog.get("classes", {}).get("destruction", []):
            raise CatalogError(f"{asset_id}: unsupported destruction class {destruction!r}")
        render = entry.get("render", {})
        range_m = render.get("range_m")
        if (
            not isinstance(range_m, list)
            or len(range_m) != 2
            or float(range_m[0]) != 0.0
            or float(range_m[1]) <= 0.0
        ):
            raise CatalogError(f"{asset_id}: render.range_m must be [0, positive]")
        lod = render.get("lod_hook", {})
        expected_level = [{"asset_id": asset_id, "distance_m": 0.0}]
        if lod.get("group") != asset_id or lod.get("levels") != expected_level:
            raise CatalogError(f"{asset_id}: deterministic LOD hook is malformed")
        origin = entry.get("origin", {})
        if origin.get("policy") != "source-authored" or origin.get("normalization_hook") != "identity":
            raise CatalogError(f"{asset_id}: unsupported origin policy")

        source_path = repo_root.joinpath(*rel.parts)
        if not source_path.is_file():
            raise CatalogError(f"{asset_id}: missing source {source}")
        if source_path.is_symlink():
            raise CatalogError(f"{asset_id}: source may not be a symlink")
        blob = source_path.read_bytes()
        parsed = parse_glb(blob, source)
        dependencies = parsed["dependencies"]
        if dependencies != entry.get("external_dependencies"):
            raise CatalogError(f"{asset_id}: external dependency lock drifted: {dependencies}")
        if dependencies:
            raise CatalogError(f"{asset_id}: canonical GLB is not self-contained: {dependencies}")
        expected = entry.get("expected", {})
        if expected.get("bytes") != len(blob) or expected.get("sha256") != sha256_bytes(blob):
            raise CatalogError(f"{asset_id}: source size/SHA-256 drifted; refresh and review catalog")
        bounds = parsed["bounds"]
        if expected.get("vertices") != bounds["vertices"]:
            raise CatalogError(f"{asset_id}: POSITION vertex count drifted")
        actual_min = rounded_vector(bounds["min"])
        actual_max = rounded_vector(bounds["max"])
        actual_dimensions = rounded_vector(
            [bounds["max"][axis] - bounds["min"][axis] for axis in range(3)]
        )
        if not float_vectors_equal(entry.get("bounds_m", {}).get("min"), actual_min):
            raise CatalogError(f"{asset_id}: minimum geometry bound drifted")
        if not float_vectors_equal(entry.get("bounds_m", {}).get("max"), actual_max):
            raise CatalogError(f"{asset_id}: maximum geometry bound drifted")
        if not float_vectors_equal(entry.get("dimensions_m"), actual_dimensions):
            raise CatalogError(f"{asset_id}: geometry dimensions drifted")
        records.append({"entry": entry, "blob": blob, "dependencies": dependencies})

    if enforce_repo_scope:
        selected, counts = candidate_sources(repo_root)
        if set(selected) != sources:
            missing = sorted(set(selected) - sources)
            extra = sorted(sources - set(selected))
            raise CatalogError(f"catalog/runtime reference closure differs: missing={missing}, extra={extra}")
        if catalog.get("scope") != counts:
            raise CatalogError(f"catalog scope counts drifted: expected {counts}")
        if counts["campaign_manifest_unique_glbs"] != 69:
            raise CatalogError(
                "campaign manifest reference count changed from the audited 69; review the new scope"
            )
    return records


def validate_output_tree(output: Path) -> None:
    for path in sorted(output.rglob("*")):
        if not path.is_file():
            continue
        rel = PurePosixPath(path.relative_to(output).as_posix())
        suffix = rel.suffix.casefold()
        if suffix in FORBIDDEN_RUNTIME_EXTENSIONS or path_has_preview_token(rel):
            raise CatalogError(f"forbidden raw/preview file escaped into runtime output: {rel}")
        if suffix not in ALLOWED_COOK_EXTENSIONS:
            raise CatalogError(f"unexpected runtime output type {suffix!r}: {rel}")


def cook_catalog(
    catalog: dict[str, Any],
    repo_root: Path,
    output: Path,
    enforce_repo_scope: bool = True,
) -> dict[str, Any]:
    output = output.resolve()
    protected_assets = (repo_root / "assets").resolve()
    protected_metadata = (repo_root / ".git").resolve()
    if output == repo_root or output.is_relative_to(protected_assets) or output.is_relative_to(
        protected_metadata
    ):
        raise CatalogError(
            f"cook output may not be the repository, source assets, or git metadata: {output}"
        )
    if output.exists():
        raise CatalogError(f"cook output must be a new path; refusing to overwrite {output}")
    records = validate_catalog(catalog, repo_root, enforce_repo_scope=enforce_repo_scope)
    output.mkdir(parents=True, exist_ok=False)
    runtime_assets: list[dict[str, Any]] = []
    byte_lines = ["bytes\truntime_path\tsource"]
    hash_lines: list[str] = []
    missing_dependencies: list[dict[str, Any]] = []
    total_bytes = 0
    for record in records:
        entry = record["entry"]
        blob = record["blob"]
        runtime_path = entry["canonical"]["runtime_path"]
        destination = output.joinpath(*PurePosixPath(runtime_path).parts)
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(blob)
        os.chmod(destination, 0o644)
        os.utime(destination, (0, 0))
        digest = sha256_bytes(blob)
        if destination.read_bytes() != blob:
            raise CatalogError(f"copy verification failed for {entry['id']}")
        total_bytes += len(blob)
        byte_lines.append(f"{len(blob)}\t{runtime_path}\t{entry['source']}")
        hash_lines.append(f"{digest}  {runtime_path}")
        for dependency in record["dependencies"]:
            missing_dependencies.append(
                {"asset_id": entry["id"], "dependency": dependency, "required": True}
            )
        runtime_assets.append(
            {
                "bounds_m": entry["bounds_m"],
                "bytes": len(blob),
                "collision_class": entry["collision_class"],
                "destruction_class": entry["destruction_class"],
                "dimensions_m": entry["dimensions_m"],
                "external_dependencies": record["dependencies"],
                "id": entry["id"],
                "license": {
                    "id": entry["license"],
                    **catalog["licenses"][entry["license"]],
                },
                "origin": entry["origin"],
                "path": runtime_path,
                "render": entry["render"],
                "required_by": entry["required_by"],
                "sha256": digest,
                "source": entry["source"],
                "source_family": entry["source_family"],
                "tags": entry["tags"],
            }
        )
    runtime_manifest = {
        "asset_count": len(runtime_assets),
        "assets": runtime_assets,
        "catalog_id": catalog["catalog_id"],
        "missing_dependencies": missing_dependencies,
        "rejected_source_classes": sorted(FORBIDDEN_RUNTIME_EXTENSIONS),
        "schema": RUNTIME_SCHEMA,
        "scope": catalog["scope"],
        "total_bytes": total_bytes,
        "version": RUNTIME_VERSION,
    }
    (output / "runtime-assets.manifest.json").write_bytes(canonical_json(runtime_manifest))
    (output / "missing-dependencies.json").write_bytes(
        canonical_json(
            {
                "catalog_id": catalog["catalog_id"],
                "dependencies": missing_dependencies,
                "schema": "kessel.missing-asset-dependencies",
                "version": 1,
            }
        )
    )
    (output / "MANIFEST.bytes.tsv").write_text("\n".join(byte_lines) + "\n", encoding="utf-8")
    (output / "MANIFEST.sha256").write_text("\n".join(hash_lines) + "\n", encoding="utf-8")
    validate_output_tree(output)
    return runtime_manifest


def repo_root_from(value: str) -> Path:
    root = Path(value).resolve()
    if not (root / "Cargo.toml").is_file() or not (root / "src").is_dir():
        raise CatalogError(f"not a Kessel repository root: {root}")
    return root


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--repo-root", default=".", help="Kessel repository root (default: cwd)")
    sub = result.add_subparsers(dest="command", required=True)
    bootstrap = sub.add_parser("bootstrap", help="create a new reviewed catalog snapshot")
    bootstrap.add_argument("--catalog", default=str(DEFAULT_CATALOG))
    validate = sub.add_parser("validate", help="validate catalog, sources, hashes, and geometry")
    validate.add_argument("--catalog", default=str(DEFAULT_CATALOG))
    cook = sub.add_parser("cook", help="copy the cataloged GLBs into a fresh runtime output")
    cook.add_argument("--catalog", default=str(DEFAULT_CATALOG))
    cook.add_argument("--output", required=True)
    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        repo_root = repo_root_from(args.repo_root)
        catalog_path = Path(args.catalog)
        if not catalog_path.is_absolute():
            catalog_path = repo_root / catalog_path
        if args.command == "bootstrap":
            if catalog_path.exists():
                raise CatalogError(f"refusing to overwrite existing catalog {catalog_path}")
            catalog = build_catalog(repo_root)
            catalog_path.parent.mkdir(parents=True, exist_ok=True)
            catalog_path.write_bytes(canonical_json(catalog))
            print(
                f"[asset-cook] catalog {catalog_path}: {catalog['scope']['total_glbs']} GLBs "
                f"({catalog['scope']['pizza_glbs']} Pizza, "
                f"{catalog['scope']['non_pizza_glbs']} non-Pizza)"
            )
        elif args.command == "validate":
            catalog = load_catalog(catalog_path)
            records = validate_catalog(catalog, repo_root)
            total = sum(len(record["blob"]) for record in records)
            print(f"[asset-cook] VALID: {len(records)} GLBs, {total} bytes, offline deterministic")
        else:
            output = Path(args.output)
            if not output.is_absolute():
                output = repo_root / output
            catalog = load_catalog(catalog_path)
            manifest = cook_catalog(catalog, repo_root, output)
            print(
                f"[asset-cook] COOKED: {manifest['asset_count']} GLBs, "
                f"{manifest['total_bytes']} bytes -> {output}"
            )
        return 0
    except CatalogError as error:
        print(f"[asset-cook] ERROR: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
