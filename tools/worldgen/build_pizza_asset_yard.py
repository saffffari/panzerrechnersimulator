#!/usr/bin/env python3
"""Build and validate an owner-private Blender yard for the Pizza Doggy GLBs.

The purchased vendor tree is an input only. This script never extracts, rewrites, or copies a
source GLB. It imports every available GLB into a second, raw-source Blender scene and creates one
normalized collection instance in the visible yard scene. Original bounds, file hashes, source
paths, and display scales remain searchable as custom properties and in a JSON manifest.

Build (run from the repository root):

    blender --background --factory-startup \
      --python tools/worldgen/build_pizza_asset_yard.py -- \
      --source-root /path/to/assets/pizza/_extracted \
      --output-dir artifacts/pizza-doggy-blender-yard-20260901 \
      --expected-count 1243 --render

Validate the saved file in a fresh Blender process:

    blender --background artifacts/pizza-doggy-blender-yard-20260901/pizza_doggy_all_1243.blend \
      --python tools/worldgen/build_pizza_asset_yard.py -- \
      --validate-blend \
      artifacts/pizza-doggy-blender-yard-20260901/pizza_doggy_all_1243.blend \
      --manifest artifacts/pizza-doggy-blender-yard-20260901/manifest.json \
      --expected-count 1243

The resulting .blend embeds whatever Blender imports from the GLBs. Under the retained Pizza
Doggy agreement it is private authoring material, not a redistributable asset library.
"""

from __future__ import annotations

import argparse
from collections import defaultdict
from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import sys
from typing import Any, Iterable


SCHEMA = "kessel.pizza-doggy-owner-private-yard"
SCHEMA_VERSION = 1
DEFAULT_EXPECTED_COUNT = 1243
HISTORICAL_INVENTORY_COUNT = 1243
HISTORICAL_NATURE_GAP = 49
GENERATED_BACKUP_SUFFIX = "_orig.glb"

PACK_ORDER = (
    "PSX Mega Pack 3.1.3",
    "PSX Mega Pack II v1.8",
    "PSX Nature v1.7.3",
    "PSX Tech v1.2.3",
    "Modular Retro FPS Kit v1.5",
)

PACK_DISPLAY_NAMES = {
    "PSX Mega Pack 3.1.3": "PSX MEGA PACK",
    "PSX Mega Pack II v1.8": "PSX MEGA PACK II",
    "PSX Nature v1.7.3": "PSX NATURE",
    "PSX Tech v1.2.3": "PSX TECH",
    "Modular Retro FPS Kit v1.5": "MODULAR RETRO FPS KIT",
}

PREFERRED_CATEGORIES = {
    "PSX Mega Pack 3.1.3": (
        "Furniture",
        "Items & Weapons",
        "Modular Structures",
        "Small Props",
        "Electronics & Misc",
        "Lighting",
        "Large Props",
        "Decals",
    ),
    "PSX Mega Pack II v1.8": (
        "Modular Structures",
        "Props",
        "Large Props & Machinery",
        "Debris & Misc",
        "Weapons & Tools",
        "Modular Props",
        "Buildings",
        "Decals",
        "Light Sources",
        "Masks",
        "Structures",
        "Doors & Gates",
    ),
    "PSX Nature v1.7.3": (
        "Pine Trees",
        "Trees, Logs & Stumps",
        "Mushrooms",
        "Stones",
        "Grass",
        "Ferns",
        "Wheat",
        "Nature Models",
    ),
    "PSX Tech v1.2.3": (
        "Levers",
        "Capacitors",
        "Panels & Terminals",
        "Gauges",
        "Buttons",
        "Receivers & Data",
        "Electrical Tools",
        "Tech Models",
    ),
    "Modular Retro FPS Kit v1.5": (
        "Walls",
        "Floors",
        "Columns",
        "Platforms",
        "Doorways",
        "Tunnels",
        "Stairs",
        "FPS Props",
    ),
}

CELL_SIZE_M = 7.5
MAX_CATEGORY_COLUMNS = 16
CATEGORY_LABEL_GAP_M = 6.0
CATEGORY_GAP_M = 5.0
PACK_GAP_M = 25.0
TARGET_DISPLAY_MAX_M = 4.5
MIN_DISPLAY_SCALE = 0.001
MAX_DISPLAY_SCALE = 250.0


@dataclass(frozen=True)
class AssetSource:
    path: Path
    relative_path: str
    pack: str
    category: str
    stem: str
    byte_count: int
    mtime_ns: int


@dataclass(frozen=True)
class Placement:
    source: AssetSource
    x: float
    y: float
    row: int
    column: int


def _slug(value: str) -> str:
    slug = "".join(char.lower() if char.isalnum() else "_" for char in value)
    return "_".join(part for part in slug.split("_") if part)


def _asset_key(relative_path: str) -> str:
    return hashlib.sha1(relative_path.encode("utf-8")).hexdigest()[:10]


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _check_glb2(path: Path) -> None:
    with path.open("rb") as handle:
        header = handle.read(12)
    if len(header) != 12 or header[:4] != b"glTF":
        raise ValueError(f"not a binary glTF file: {path}")
    version, declared_size = struct.unpack("<II", header[4:12])
    actual_size = path.stat().st_size
    if version != 2:
        raise ValueError(f"unsupported GLB version {version}: {path}")
    if declared_size != actual_size:
        raise ValueError(
            f"GLB header length mismatch ({declared_size} != {actual_size}): {path}"
        )


def _derived_category(pack: str, stem: str) -> str:
    lower = stem.lower()
    if pack == "PSX Nature v1.7.3":
        if lower.startswith("pine_tree"):
            return "Pine Trees"
        if lower.startswith(("tree_", "tree_log", "tree_stump")):
            return "Trees, Logs & Stumps"
        if lower.startswith("mushroom"):
            return "Mushrooms"
        if lower.startswith("stone"):
            return "Stones"
        if lower.startswith("grass"):
            return "Grass"
        if lower.startswith("fern"):
            return "Ferns"
        if lower.startswith("wheat"):
            return "Wheat"
        return "Nature Models"
    if pack == "PSX Tech v1.2.3":
        if lower.startswith("lever"):
            return "Levers"
        if lower.startswith("capacitor"):
            return "Capacitors"
        if lower.startswith(("panel", "control_panel", "computer_terminal", "screen")):
            return "Panels & Terminals"
        if lower.startswith("gauge"):
            return "Gauges"
        if lower.startswith(("button", "buttons", "fader")):
            return "Buttons"
        if lower.startswith(("signal_receiver", "data_block", "circuit_board")):
            return "Receivers & Data"
        if lower.startswith(("fuse_box", "electrical_tape", "hydraulic_spreader")):
            return "Electrical Tools"
        return "Tech Models"
    if pack == "Modular Retro FPS Kit v1.5":
        if lower.startswith("wall"):
            return "Walls"
        if lower.startswith("floor"):
            return "Floors"
        if lower.startswith("column"):
            return "Columns"
        if lower.startswith("platform"):
            return "Platforms"
        if lower.startswith("doorway"):
            return "Doorways"
        if lower.startswith("tunnel"):
            return "Tunnels"
        if lower.startswith("stairs"):
            return "Stairs"
        return "FPS Props"
    return "Models"


def category_for_relative(relative_path: str) -> str:
    parts = Path(relative_path).parts
    pack = parts[0]
    glb_folder_index = None
    for index, part in enumerate(parts[:-1]):
        if part.upper().startswith("GLB"):
            glb_folder_index = index
            break
    if glb_folder_index is not None and glb_folder_index + 1 < len(parts) - 1:
        return parts[glb_folder_index + 1]
    return _derived_category(pack, Path(relative_path).stem)


def _pack_sort_key(pack: str) -> tuple[int, str]:
    try:
        return PACK_ORDER.index(pack), pack.casefold()
    except ValueError:
        return len(PACK_ORDER), pack.casefold()


def _category_sort_key(pack: str, category: str) -> tuple[int, str]:
    preferred = PREFERRED_CATEGORIES.get(pack, ())
    try:
        return preferred.index(category), category.casefold()
    except ValueError:
        return len(preferred), category.casefold()


def discover_sources(source_root: Path) -> list[AssetSource]:
    source_root = source_root.resolve()
    if not source_root.is_dir():
        raise ValueError(f"Pizza source root does not exist: {source_root}")
    sources: list[AssetSource] = []
    for path in source_root.rglob("*.glb"):
        if not path.is_file():
            continue
        # fix_sideways_glbs.py preserves a pre-rewrite copy beside each Nature source. Those
        # owner-generated backups are provenance safeguards, not additional Pizza Doggy assets.
        if path.name.lower().endswith(GENERATED_BACKUP_SUFFIX):
            continue
        _check_glb2(path)
        stat = path.stat()
        relative_path = path.relative_to(source_root).as_posix()
        parts = Path(relative_path).parts
        if len(parts) < 2:
            raise ValueError(f"GLB has no pack directory: {path}")
        sources.append(
            AssetSource(
                path=path,
                relative_path=relative_path,
                pack=parts[0],
                category=category_for_relative(relative_path),
                stem=path.stem,
                byte_count=stat.st_size,
                mtime_ns=stat.st_mtime_ns,
            )
        )
    sources.sort(
        key=lambda item: (
            _pack_sort_key(item.pack),
            _category_sort_key(item.pack, item.category),
            item.stem.casefold(),
            item.relative_path.casefold(),
        )
    )
    return sources


def generated_backup_paths(source_root: Path) -> list[str]:
    return sorted(
        path.relative_to(source_root).as_posix()
        for path in source_root.rglob("*.glb")
        if path.is_file() and path.name.lower().endswith(GENERATED_BACKUP_SUFFIX)
    )


def inventory_explanation(source_count: int, backup_count: int) -> str:
    if source_count == HISTORICAL_INVENTORY_COUNT:
        return (
            "The full Hal source contains both 49-model vendor PSX Nature variants: the standard "
            "tree and 'Tree branches separated'. Locally generated *_orig.glb preservation copies "
            f"({backup_count}) are explicitly excluded because they are backups, not additional "
            "Pizza Doggy assets."
        )
    if source_count == HISTORICAL_INVENTORY_COUNT - HISTORICAL_NATURE_GAP:
        return (
            "This source contains only the 49-model 'Tree branches separated' PSX Nature variant; "
            "the second historical 49-model Nature variant is not present. Locally generated "
            f"*_orig.glb preservation copies ({backup_count}) are excluded."
        )
    return (
        f"Discovered {source_count} Pizza GLBs after excluding {backup_count} locally generated "
        "*_orig.glb preservation copies; compare this closure with the retained purchase archive."
    )


def create_layout(
    sources: Iterable[AssetSource],
) -> tuple[list[Placement], dict[str, Any]]:
    grouped: dict[str, dict[str, list[AssetSource]]] = defaultdict(lambda: defaultdict(list))
    for source in sources:
        grouped[source.pack][source.category].append(source)
    packs = sorted(grouped, key=_pack_sort_key)
    lane_width = MAX_CATEGORY_COLUMNS * CELL_SIZE_M
    stride = lane_width + PACK_GAP_M
    pack_centers = {
        pack: (index - (len(packs) - 1) * 0.5) * stride
        for index, pack in enumerate(packs)
    }
    placements: list[Placement] = []
    categories_meta: list[dict[str, Any]] = []
    pack_meta: list[dict[str, Any]] = []
    global_min_y = 0.0
    for pack in packs:
        y_cursor = 0.0
        categories = sorted(
            grouped[pack], key=lambda category: _category_sort_key(pack, category)
        )
        pack_count = 0
        for category in categories:
            assets = grouped[pack][category]
            columns = min(MAX_CATEGORY_COLUMNS, len(assets))
            rows = math.ceil(len(assets) / columns)
            label_y = -y_cursor
            first_row_y = label_y - CATEGORY_LABEL_GAP_M
            for index, source in enumerate(assets):
                row, column = divmod(index, columns)
                x = pack_centers[pack] + (column - (columns - 1) * 0.5) * CELL_SIZE_M
                y = first_row_y - row * CELL_SIZE_M
                placements.append(Placement(source, x, y, row, column))
            categories_meta.append(
                {
                    "pack": pack,
                    "category": category,
                    "count": len(assets),
                    "columns": columns,
                    "rows": rows,
                    "label_position": [pack_centers[pack], label_y, 0.04],
                }
            )
            pack_count += len(assets)
            y_cursor += CATEGORY_LABEL_GAP_M + rows * CELL_SIZE_M + CATEGORY_GAP_M
        global_min_y = min(global_min_y, -y_cursor)
        pack_meta.append(
            {
                "pack": pack,
                "display_name": PACK_DISPLAY_NAMES.get(pack, pack.upper()),
                "count": pack_count,
                "center_x": pack_centers[pack],
                "depth_m": y_cursor,
                "category_count": len(categories),
            }
        )
    min_x = min((placement.x for placement in placements), default=0.0) - CELL_SIZE_M
    max_x = max((placement.x for placement in placements), default=0.0) + CELL_SIZE_M
    bounds = {
        "min_x": min_x,
        "max_x": max_x,
        "min_y": global_min_y - CELL_SIZE_M,
        "max_y": 28.0,
    }
    return placements, {
        "cell_size_m": CELL_SIZE_M,
        "target_display_max_m": TARGET_DISPLAY_MAX_M,
        "min_display_scale": MIN_DISPLAY_SCALE,
        "max_display_scale": MAX_DISPLAY_SCALE,
        "max_category_columns": MAX_CATEGORY_COLUMNS,
        "bounds": bounds,
        "packs": pack_meta,
        "categories": categories_meta,
    }


def _parse_args() -> argparse.Namespace:
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else sys.argv[1:]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--expected-count", type=int, default=DEFAULT_EXPECTED_COUNT)
    parser.add_argument("--render", action="store_true")
    parser.add_argument("--force", action="store_true")
    parser.add_argument("--inventory-only", action="store_true")
    parser.add_argument("--validate-blend", type=Path)
    parser.add_argument("--manifest", type=Path)
    return parser.parse_args(argv)


def inventory(args: argparse.Namespace) -> int:
    if args.source_root is None:
        raise SystemExit("inventory requires --source-root")
    source_root = args.source_root.resolve()
    sources = discover_sources(source_root)
    backups = generated_backup_paths(source_root)
    packs: dict[str, dict[str, int]] = defaultdict(lambda: {"count": 0, "bytes": 0})
    for source in sources:
        packs[source.pack]["count"] += 1
        packs[source.pack]["bytes"] += source.byte_count
    result = {
        "source_root": str(source_root),
        "source_glb_count": len(sources),
        "source_glb_bytes": sum(source.byte_count for source in sources),
        "expected_count": args.expected_count,
        "expected_count_matches": len(sources) == args.expected_count,
        "excluded_generated_backup_count": len(backups),
        "packs": dict(sorted(packs.items(), key=lambda item: _pack_sort_key(item[0]))),
        "explanation": inventory_explanation(len(sources), len(backups)),
    }
    print("PIZZA_YARD_INVENTORY " + json.dumps(result, sort_keys=True), flush=True)
    return 0 if result["expected_count_matches"] else 1


def _id_props(target: Any, values: dict[str, Any]) -> None:
    for key, value in values.items():
        if isinstance(value, Path):
            value = str(value)
        if isinstance(value, tuple):
            value = list(value)
        target[key] = value


def _new_collection(bpy: Any, name: str, parent: Any) -> Any:
    collection = bpy.data.collections.new(name)
    parent.children.link(collection)
    return collection


def _new_material(bpy: Any, name: str, color: tuple[float, float, float, float]) -> Any:
    material = bpy.data.materials.new(name)
    material.diffuse_color = color
    return material


def _add_plane(
    bpy: Any,
    collection: Any,
    name: str,
    center: tuple[float, float, float],
    width: float,
    height: float,
    material: Any,
) -> Any:
    cx, cy, cz = center
    half_w, half_h = width * 0.5, height * 0.5
    mesh = bpy.data.meshes.new(name + " Mesh")
    mesh.from_pydata(
        [
            (cx - half_w, cy - half_h, cz),
            (cx + half_w, cy - half_h, cz),
            (cx + half_w, cy + half_h, cz),
            (cx - half_w, cy + half_h, cz),
        ],
        [],
        [(0, 1, 2, 3)],
    )
    mesh.materials.append(material)
    obj = bpy.data.objects.new(name, mesh)
    collection.objects.link(obj)
    return obj


def _add_text(
    bpy: Any,
    collection: Any,
    name: str,
    body: str,
    location: tuple[float, float, float],
    size: float,
    material: Any,
) -> Any:
    curve = bpy.data.curves.new(name + " Curve", type="FONT")
    curve.body = body
    curve.align_x = "CENTER"
    curve.align_y = "CENTER"
    curve.size = size
    curve.extrude = 0.012
    curve.materials.append(material)
    obj = bpy.data.objects.new(name, curve)
    obj.location = location
    collection.objects.link(obj)
    return obj


def _object_bounds(objects: Iterable[Any], vector_type: Any) -> tuple[list[float], list[float]]:
    points = []
    for obj in objects:
        if obj.type != "MESH":
            continue
        points.extend(obj.matrix_world @ vector_type(corner) for corner in obj.bound_box)
    if not points:
        raise ValueError("import created no mesh bounds")
    minimum = [min(point[axis] for point in points) for axis in range(3)]
    maximum = [max(point[axis] for point in points) for axis in range(3)]
    return minimum, maximum


def _is_packed_image(image: Any) -> bool:
    if getattr(image, "packed_file", None) is not None:
        return True
    return bool(getattr(image, "packed_files", ()))


def _configure_render(bpy: Any, scene: Any, bounds: dict[str, float], output: Path) -> Any:
    width = bounds["max_x"] - bounds["min_x"]
    height = bounds["max_y"] - bounds["min_y"]
    center_x = (bounds["min_x"] + bounds["max_x"]) * 0.5
    center_y = (bounds["min_y"] + bounds["max_y"]) * 0.5
    camera_data = bpy.data.cameras.new("Pizza Doggy Yard Overview Camera")
    camera_data.type = "ORTHO"
    camera_data.ortho_scale = max(height * 1.05, width * 1.05 / (16.0 / 9.0))
    camera = bpy.data.objects.new("Pizza Doggy Yard Overview Camera", camera_data)
    camera.location = (center_x, center_y, max(width, height) * 1.4)
    camera.rotation_euler = (0.0, 0.0, 0.0)
    scene.collection.objects.link(camera)
    scene.camera = camera

    available = {
        item.identifier
        for item in scene.render.bl_rna.properties["engine"].enum_items
    }
    for candidate in ("BLENDER_WORKBENCH_NEXT", "BLENDER_WORKBENCH", "BLENDER_EEVEE_NEXT", "BLENDER_EEVEE"):
        if candidate in available:
            scene.render.engine = candidate
            break
    if scene.render.engine.startswith("BLENDER_WORKBENCH"):
        shading = scene.display.shading
        shading.light = "STUDIO"
        shading.color_type = "MATERIAL"
        shading.show_shadows = True
        shading.show_cavity = True
        shading.cavity_type = "WORLD"
        shading.background_type = "VIEWPORT"
        shading.background_color = (0.035, 0.04, 0.05)
    scene.render.resolution_x = 2400
    scene.render.resolution_y = 1350
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.filepath = str(output)
    scene.render.film_transparent = False
    return camera


def _write_failure(output_dir: Path, failures: list[dict[str, str]]) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    path = output_dir / "build_failures.json"
    path.write_text(json.dumps({"failures": failures}, indent=2) + "\n", encoding="utf-8")
    print(f"PIZZA_YARD_FAILURE_REPORT {path}", flush=True)


def build(args: argparse.Namespace) -> int:
    if args.source_root is None or args.output_dir is None:
        raise SystemExit("build requires --source-root and --output-dir")
    source_root = args.source_root.resolve()
    output_dir = args.output_dir.resolve()
    if output_dir == source_root or source_root in output_dir.parents:
        raise SystemExit("output directory must not be inside the vendor source tree")
    sources = discover_sources(source_root)
    excluded_backups = generated_backup_paths(source_root)
    if len(sources) != args.expected_count:
        raise SystemExit(
            f"expected {args.expected_count} GLBs but discovered {len(sources)} below {source_root}"
        )
    placements, layout = create_layout(sources)
    output_dir.mkdir(parents=True, exist_ok=True)
    blend_path = output_dir / f"pizza_doggy_all_{len(sources)}.blend"
    manifest_path = output_dir / "manifest.json"
    report_path = output_dir / "report.md"
    screenshot_path = output_dir / "pizza_doggy_asset_yard_overview.png"
    generated_paths = (blend_path, manifest_path, report_path, screenshot_path)
    existing = [path for path in generated_paths if path.exists()]
    if existing and not args.force:
        raise SystemExit(
            "refusing to overwrite generated output; pass --force: "
            + ", ".join(str(path) for path in existing)
        )

    import bpy
    from mathutils import Vector

    bpy.ops.wm.read_factory_settings(use_empty=True)
    yard_scene = bpy.context.scene
    yard_scene.name = "PIZZA_DOGGY_ASSET_YARD"
    yard_scene.unit_settings.system = "METRIC"
    yard_scene.unit_settings.length_unit = "METERS"
    source_scene = bpy.data.scenes.new("PIZZA_DOGGY_RAW_SOURCE_LIBRARY")
    source_scene.unit_settings.system = "METRIC"
    source_scene.unit_settings.length_unit = "METERS"

    warning = (
        "OWNER-PRIVATE PIZZA DOGGY AUTHORING LIBRARY — DO NOT DISTRIBUTE, SHIP, "
        "OR PUBLISH AS STANDALONE ASSETS"
    )
    common_scene_props = {
        "pizza_yard_schema": SCHEMA,
        "pizza_yard_schema_version": SCHEMA_VERSION,
        "pizza_asset_count": len(sources),
        "pizza_license_id": "pizzadoggy-local-purchase",
        "pizza_owner_private": True,
        "pizza_distribution_warning": warning,
        "pizza_source_root": str(source_root),
    }
    _id_props(yard_scene, common_scene_props)
    _id_props(source_scene, common_scene_props)

    yard_root = _new_collection(bpy, "PIZZA DOGGY YARD — OWNER PRIVATE", yard_scene.collection)
    source_root_collection = _new_collection(
        bpy, "PIZZA DOGGY RAW SOURCE — OWNER PRIVATE", source_scene.collection
    )
    infrastructure = _new_collection(bpy, "YARD INFRASTRUCTURE", yard_root)

    source_hierarchy: dict[tuple[str, str], Any] = {}
    yard_hierarchy: dict[tuple[str, str], Any] = {}
    for pack_meta in layout["packs"]:
        pack = pack_meta["pack"]
        source_pack = _new_collection(bpy, f"PACK | {pack}", source_root_collection)
        yard_pack = _new_collection(bpy, f"PACK | {pack}", yard_root)
        _id_props(source_pack, {"pizza_pack": pack, "pizza_owner_private": True})
        _id_props(yard_pack, {"pizza_pack": pack, "pizza_owner_private": True})
        categories = [
            item for item in layout["categories"] if item["pack"] == pack
        ]
        for category_meta in categories:
            category = category_meta["category"]
            source_category = _new_collection(
                bpy, f"CATEGORY | {category}", source_pack
            )
            yard_category = _new_collection(bpy, f"CATEGORY | {category}", yard_pack)
            props = {
                "pizza_pack": pack,
                "pizza_category": category,
                "pizza_asset_count": category_meta["count"],
                "pizza_owner_private": True,
            }
            _id_props(source_category, props)
            _id_props(yard_category, props)
            source_hierarchy[(pack, category)] = source_category
            yard_hierarchy[(pack, category)] = yard_category

    ground_material = _new_material(bpy, "Yard Ground", (0.025, 0.03, 0.038, 1.0))
    lane_colors = (
        (0.12, 0.055, 0.045, 1.0),
        (0.055, 0.09, 0.12, 1.0),
        (0.055, 0.115, 0.065, 1.0),
        (0.105, 0.075, 0.13, 1.0),
        (0.125, 0.105, 0.045, 1.0),
    )
    text_material = _new_material(bpy, "Yard Labels", (0.88, 0.9, 0.94, 1.0))
    warning_material = _new_material(bpy, "Private Warning", (1.0, 0.22, 0.08, 1.0))
    bounds = layout["bounds"]
    ground_width = bounds["max_x"] - bounds["min_x"]
    ground_height = bounds["max_y"] - bounds["min_y"]
    _add_plane(
        bpy,
        infrastructure,
        "Yard Ground",
        (
            (bounds["min_x"] + bounds["max_x"]) * 0.5,
            (bounds["min_y"] + bounds["max_y"]) * 0.5,
            -0.06,
        ),
        ground_width + 10.0,
        ground_height + 10.0,
        ground_material,
    )
    lane_width = MAX_CATEGORY_COLUMNS * CELL_SIZE_M
    for index, pack_meta in enumerate(layout["packs"]):
        _add_plane(
            bpy,
            infrastructure,
            f"Lane | {pack_meta['pack']}",
            (
                pack_meta["center_x"],
                (bounds["min_y"] + bounds["max_y"]) * 0.5,
                -0.04,
            ),
            lane_width,
            ground_height,
            _new_material(bpy, f"Lane {index + 1}", lane_colors[index % len(lane_colors)]),
        )
        _add_text(
            bpy,
            infrastructure,
            f"PACK LABEL | {pack_meta['pack']}",
            f"{pack_meta['display_name']}  ·  {pack_meta['count']} ASSETS",
            (pack_meta["center_x"], 17.0, 0.05),
            3.4,
            text_material,
        )
    for category_meta in layout["categories"]:
        x, y, z = category_meta["label_position"]
        _add_text(
            bpy,
            infrastructure,
            f"CATEGORY LABEL | {category_meta['pack']} | {category_meta['category']}",
            f"{category_meta['category'].upper()}  ·  {category_meta['count']}",
            (x, y, z),
            1.45,
            text_material,
        )
    _add_text(
        bpy,
        infrastructure,
        "OWNER PRIVATE WARNING",
        "OWNER-PRIVATE · PIZZA DOGGY · DO NOT DISTRIBUTE",
        (
            (bounds["min_x"] + bounds["max_x"]) * 0.5,
            25.0,
            0.06,
        ),
        4.0,
        warning_material,
    )

    source_snapshot = {
        source.relative_path: (source.byte_count, source.mtime_ns) for source in sources
    }
    failures: list[dict[str, str]] = []
    records: list[dict[str, Any]] = []
    automatic_packed_images: set[str] = set()
    relative_external_images: set[str] = set()
    bpy.context.window.scene = source_scene
    start_time = datetime.now(timezone.utc)

    for index, placement in enumerate(placements, start=1):
        source = placement.source
        key = _asset_key(source.relative_path)
        before_objects = set(bpy.data.objects)
        before_collections = set(bpy.data.collections)
        before_images = set(bpy.data.images)
        try:
            bpy.ops.import_scene.gltf(filepath=str(source.path), loglevel=50)
            new_objects = [obj for obj in bpy.data.objects if obj not in before_objects]
            imported_collections = [
                collection
                for collection in bpy.data.collections
                if collection not in before_collections
            ]
            new_images = [image for image in bpy.data.images if image not in before_images]
            if not new_objects:
                raise ValueError("Blender importer created no objects")
            asset_collection = bpy.data.collections.new(
                f"ASSET | {source.stem} | {key}"
            )
            source_hierarchy[(source.pack, source.category)].children.link(asset_collection)
            for obj in new_objects:
                original_name = obj.name
                asset_collection.objects.link(obj)
                for old_collection in list(obj.users_collection):
                    if old_collection != asset_collection:
                        old_collection.objects.unlink(obj)
                obj.name = f"{source.stem} | {original_name} | {key}"
                _id_props(
                    obj,
                    {
                        "pizza_asset_id": source.relative_path,
                        "pizza_asset_key": key,
                        "pizza_original_object_name": original_name,
                        "pizza_source_relpath": source.relative_path,
                        "pizza_pack": source.pack,
                        "pizza_category": source.category,
                        "pizza_owner_private": True,
                    },
                )
            for collection in imported_collections:
                if collection == asset_collection:
                    continue
                if not collection.objects and not collection.children:
                    bpy.data.collections.remove(collection)

            bpy.context.view_layer.update()
            minimum, maximum = _object_bounds(new_objects, Vector)
            dimensions = [maximum[axis] - minimum[axis] for axis in range(3)]
            maximum_dimension = max(dimensions)
            if not math.isfinite(maximum_dimension) or maximum_dimension <= 1.0e-8:
                raise ValueError(f"invalid model bounds: {dimensions}")
            center = [
                (minimum[axis] + maximum[axis]) * 0.5 for axis in range(3)
            ]
            display_scale = min(
                MAX_DISPLAY_SCALE,
                max(MIN_DISPLAY_SCALE, TARGET_DISPLAY_MAX_M / maximum_dimension),
            )
            file_sha256 = _sha256(source.path)
            props = {
                "pizza_asset_id": source.relative_path,
                "pizza_asset_key": key,
                "pizza_source_relpath": source.relative_path,
                "pizza_source_sha256": file_sha256,
                "pizza_source_bytes": source.byte_count,
                "pizza_pack": source.pack,
                "pizza_category": source.category,
                "pizza_original_dimensions_m": dimensions,
                "pizza_original_bounds_min_m": minimum,
                "pizza_original_bounds_max_m": maximum,
                "pizza_display_scale": display_scale,
                "pizza_license_id": "pizzadoggy-local-purchase",
                "pizza_owner_private": True,
                "pizza_distribution_warning": warning,
            }
            _id_props(asset_collection, props)
            asset_collection.asset_mark()
            asset_collection.asset_data.author = "Pizza Doggy"
            asset_collection.asset_data.description = (
                f"OWNER-PRIVATE {source.pack} / {source.category} / {source.stem}; "
                "do not redistribute as a standalone asset"
            )
            for tag in ("Pizza Doggy", source.pack, source.category, "Owner Private"):
                asset_collection.asset_data.tags.new(tag)

            for image in new_images:
                if _is_packed_image(image):
                    automatic_packed_images.add(image.name)
                    continue
                if image.source != "FILE" or not image.filepath:
                    continue
                absolute = Path(bpy.path.abspath(image.filepath)).resolve()
                relative = os.path.relpath(absolute, output_dir)
                image.filepath = "//" + relative
                relative_external_images.add(relative)

            bpy.context.window.scene = yard_scene
            instance = bpy.data.objects.new(
                f"DISPLAY | {source.stem} | {key}", None
            )
            instance.instance_type = "COLLECTION"
            instance.instance_collection = asset_collection
            instance.empty_display_type = "CUBE"
            instance.empty_display_size = 0.18
            instance.scale = (display_scale,) * 3
            instance.location = (
                placement.x - display_scale * center[0],
                placement.y - display_scale * center[1],
                0.08 - display_scale * minimum[2],
            )
            _id_props(instance, {**props, "pizza_asset_role": "display_root"})
            yard_category = yard_hierarchy[(source.pack, source.category)]
            yard_category.objects.link(instance)

            label = bpy.data.objects.new(
                f"LABEL | {source.stem} | {key}", None
            )
            label.empty_display_type = "PLAIN_AXES"
            label.empty_display_size = 0.11
            label.show_name = True
            label.show_in_front = True
            label.location = (
                placement.x,
                placement.y - CELL_SIZE_M * 0.39,
                0.12,
            )
            _id_props(
                label,
                {
                    "pizza_asset_id": source.relative_path,
                    "pizza_asset_key": key,
                    "pizza_asset_role": "viewport_label",
                    "pizza_pack": source.pack,
                    "pizza_category": source.category,
                    "pizza_owner_private": True,
                },
            )
            yard_category.objects.link(label)
            bpy.context.window.scene = source_scene

            records.append(
                {
                    "asset_id": source.relative_path,
                    "asset_key": key,
                    "source_relpath": source.relative_path,
                    "source_sha256": file_sha256,
                    "source_bytes": source.byte_count,
                    "pack": source.pack,
                    "category": source.category,
                    "stem": source.stem,
                    "original_bounds_min_m": [round(value, 9) for value in minimum],
                    "original_bounds_max_m": [round(value, 9) for value in maximum],
                    "original_dimensions_m": [round(value, 9) for value in dimensions],
                    "display_scale": round(display_scale, 12),
                    "yard_cell": {
                        "x": placement.x,
                        "y": placement.y,
                        "row": placement.row,
                        "column": placement.column,
                    },
                    "source_collection": asset_collection.name,
                    "display_object": instance.name,
                    "label_object": label.name,
                    "imported_object_count": len(new_objects),
                    "automatic_packed_image_count": sum(
                        1 for image in new_images if _is_packed_image(image)
                    ),
                    "license_id": "pizzadoggy-local-purchase",
                    "owner_private": True,
                }
            )
        except Exception as error:
            failures.append(
                {
                    "source_relpath": source.relative_path,
                    "error": f"{type(error).__name__}: {error}",
                }
            )
            print(
                f"PIZZA_YARD_IMPORT_FAILURE {source.relative_path}: "
                f"{type(error).__name__}: {error}",
                flush=True,
            )
            bpy.context.window.scene = source_scene
        if index % 50 == 0 or index == len(placements):
            print(
                f"PIZZA_YARD_PROGRESS {index}/{len(placements)} "
                f"imported={len(records)} failed={len(failures)}",
                flush=True,
            )

    after_snapshot = {
        source.relative_path: (source.path.stat().st_size, source.path.stat().st_mtime_ns)
        for source in sources
    }
    changed_sources = sorted(
        relative_path
        for relative_path, before in source_snapshot.items()
        if after_snapshot.get(relative_path) != before
    )
    if failures or len(records) != len(sources) or changed_sources:
        if changed_sources:
            failures.append(
                {
                    "source_relpath": "[source tree]",
                    "error": "source files changed during build: " + ", ".join(changed_sources),
                }
            )
        _write_failure(output_dir, failures)
        raise SystemExit(
            f"yard build failed: imported {len(records)}/{len(sources)}; "
            f"{len(failures)} failures"
        )

    bpy.context.window.scene = yard_scene
    _configure_render(bpy, yard_scene, bounds, screenshot_path)
    yard_scene["pizza_manifest_filename"] = manifest_path.name
    yard_scene["pizza_report_filename"] = report_path.name
    yard_scene["pizza_screenshot_filename"] = screenshot_path.name
    yard_scene["pizza_raw_source_scene"] = source_scene.name
    yard_scene["pizza_automatic_packed_images"] = len(automatic_packed_images)
    yard_scene["pizza_relative_external_images"] = len(relative_external_images)
    bpy.ops.wm.save_as_mainfile(filepath=str(blend_path), compress=True)
    print(f"PIZZA_YARD_SAVED {blend_path}", flush=True)
    if args.render:
        bpy.ops.render.render(write_still=True)
        print(f"PIZZA_YARD_RENDERED {screenshot_path}", flush=True)

    finish_time = datetime.now(timezone.utc)
    blend_bytes = blend_path.stat().st_size
    blend_sha256 = _sha256(blend_path)
    screenshot_meta = None
    if screenshot_path.exists():
        screenshot_meta = {
            "path": screenshot_path.name,
            "bytes": screenshot_path.stat().st_size,
            "sha256": _sha256(screenshot_path),
        }
    manifest = {
        "schema": SCHEMA,
        "schema_version": SCHEMA_VERSION,
        "generated_at_utc": finish_time.isoformat(),
        "build_started_at_utc": start_time.isoformat(),
        "blender_version": bpy.app.version_string,
        "source_root": str(source_root),
        "source_glb_count": len(sources),
        "source_glb_bytes": sum(source.byte_count for source in sources),
        "imported_asset_count": len(records),
        "viewport_label_count": len(records),
        "source_files_unchanged": not changed_sources,
        "historical_inventory": {
            "documented_model_count": HISTORICAL_INVENTORY_COUNT,
            "currently_available_count": len(sources),
            "gap_count": HISTORICAL_INVENTORY_COUNT - len(sources),
            "excluded_generated_backup_count": len(excluded_backups),
            "excluded_generated_backups": excluded_backups,
            "explanation": inventory_explanation(len(sources), len(excluded_backups)),
        },
        "license": {
            "id": "pizzadoggy-local-purchase",
            "credit": "Pizza Doggy",
            "owner_private": True,
            "distribution_warning": warning,
            "policy": (
                "Use only as integrated game authoring material. Do not redistribute this .blend, "
                "the source GLBs, or extracted/cooked models as an asset pack, SDK, template, or "
                "standalone download. Re-review retained purchase evidence and current vendor "
                "terms before public/commercial release."
            ),
        },
        "texture_policy": {
            "automatically_packed_embedded_image_count": len(automatic_packed_images),
            "relative_external_image_count": len(relative_external_images),
            "relative_external_images": sorted(relative_external_images),
            "explicit_pack_operation_used": False,
        },
        "layout": layout,
        "blend": {
            "path": blend_path.name,
            "bytes": blend_bytes,
            "sha256": blend_sha256,
            "yard_scene": yard_scene.name,
            "raw_source_scene": source_scene.name,
            "display_root_count": len(records),
        },
        "screenshot": screenshot_meta,
        "assets": records,
    }
    manifest_path.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    category_lines = []
    for pack_meta in layout["packs"]:
        pack = pack_meta["pack"]
        categories = [
            item for item in layout["categories"] if item["pack"] == pack
        ]
        summary = ", ".join(
            f"{item['category']} {item['count']}" for item in categories
        )
        category_lines.append(f"- **{pack}:** {pack_meta['count']} ({summary})")
    screenshot_line = (
        f"- Screenshot: `{screenshot_path.name}` ({screenshot_meta['bytes']:,} bytes)"
        if screenshot_meta
        else "- Screenshot: not requested"
    )
    inventory_paragraph = inventory_explanation(len(sources), len(excluded_backups))
    report = f"""# Pizza Doggy owner-private Blender yard report

Status: **PASS**

- Blender: {bpy.app.version_string}
- Source root (read-only input): `{source_root}`
- Imported GLBs: **{len(records)} / {len(sources)}**
- Source bytes: **{sum(source.byte_count for source in sources):,}**
- Source files changed during build: **no**
- Yard instances: **{len(records)}**
- Lightweight viewport labels: **{len(records)}**
- Raw source collections marked as Blender assets: **{len(records)}**
- `.blend`: `{blend_path.name}` ({blend_bytes:,} bytes; SHA-256 `{blend_sha256}`)
{screenshot_line}
- Manifest: `{manifest_path.name}`

## Pack/category closure

{chr(10).join(category_lines)}

## Historical count discrepancy

`docs/ASSET_INVENTORY.md` records 1,243 models. {inventory_paragraph} This yard contains every
currently available Pizza Doggy model in the selected source closure and does not count generated
preservation copies as extra vendor assets.

## Authoring behavior

The visible `PIZZA_DOGGY_ASSET_YARD` scene contains one normalized collection instance per source
GLB. Its second scene, `PIZZA_DOGGY_RAW_SOURCE_LIBRARY`, preserves the imported dimensions and a
pack/category/asset hierarchy. Every asset collection is searchable in Blender's Asset Browser;
the collection, its display instance, and its viewport label carry source path, SHA-256, byte
count, pack/category, original bounds, display scale, and owner-private properties. Embedded GLB
images were retained through Blender's importer; the build performed no explicit texture packing.
Any non-packed image references are stored relative to the yard output.

## License guardrail

**OWNER-PRIVATE — DO NOT DISTRIBUTE THIS `.blend` OR ITS EXTRACTED MODELS AS A STANDALONE ASSET
LIBRARY.** Pizza Doggy permits game integration under the retained purchased agreement, but not
resale/sharing of source assets, edited copies, asset packs, templates, SDKs, or general-purpose
bundles. Re-review current vendor terms and retained purchase evidence before a public or
commercial Kessel release.
"""
    report_path.write_text(report, encoding="utf-8")
    print(
        f"PIZZA_YARD_COMPLETE assets={len(records)} bytes={sum(source.byte_count for source in sources)} "
        f"blend_bytes={blend_bytes} manifest={manifest_path} report={report_path}",
        flush=True,
    )
    return 0


def validate(args: argparse.Namespace) -> int:
    if args.validate_blend is None or args.manifest is None:
        raise SystemExit("validation requires --validate-blend and --manifest")
    import bpy

    blend_path = args.validate_blend.resolve()
    manifest_path = args.manifest.resolve()
    if Path(bpy.data.filepath).resolve() != blend_path:
        raise SystemExit(
            f"Blender opened {bpy.data.filepath!r}, not requested yard {str(blend_path)!r}"
        )
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    records = manifest.get("assets", [])
    display_roots = [
        obj
        for obj in bpy.data.objects
        if obj.get("pizza_asset_role") == "display_root"
    ]
    labels = [
        obj
        for obj in bpy.data.objects
        if obj.get("pizza_asset_role") == "viewport_label"
    ]
    asset_collections = [
        collection
        for collection in bpy.data.collections
        if collection.get("pizza_asset_id")
    ]
    expected_ids = {record["asset_id"] for record in records}
    display_ids = {obj.get("pizza_asset_id") for obj in display_roots}
    label_ids = {obj.get("pizza_asset_id") for obj in labels}
    collection_ids = {
        collection.get("pizza_asset_id") for collection in asset_collections
    }
    errors = []
    checks = {
        "manifest_asset_count": len(records),
        "display_root_count": len(display_roots),
        "viewport_label_count": len(labels),
        "asset_collection_count": len(asset_collections),
        "expected_count": args.expected_count,
        "yard_scene_present": "PIZZA_DOGGY_ASSET_YARD" in bpy.data.scenes,
        "raw_source_scene_present": "PIZZA_DOGGY_RAW_SOURCE_LIBRARY" in bpy.data.scenes,
        "display_ids_match": display_ids == expected_ids,
        "label_ids_match": label_ids == expected_ids,
        "collection_ids_match": collection_ids == expected_ids,
    }
    for name in (
        "manifest_asset_count",
        "display_root_count",
        "viewport_label_count",
        "asset_collection_count",
    ):
        if checks[name] != args.expected_count:
            errors.append(f"{name}={checks[name]} expected={args.expected_count}")
    for name in (
        "yard_scene_present",
        "raw_source_scene_present",
        "display_ids_match",
        "label_ids_match",
        "collection_ids_match",
    ):
        if checks[name] is not True:
            errors.append(f"{name}=false")
    expected_blend_sha = manifest.get("blend", {}).get("sha256")
    actual_blend_sha = _sha256(blend_path)
    checks["blend_sha256_matches"] = actual_blend_sha == expected_blend_sha
    if not checks["blend_sha256_matches"]:
        errors.append("blend SHA-256 does not match manifest")
    screenshot = manifest.get("screenshot")
    if screenshot:
        screenshot_path = manifest_path.parent / screenshot["path"]
        checks["screenshot_present"] = screenshot_path.is_file()
        checks["screenshot_sha256_matches"] = (
            screenshot_path.is_file()
            and _sha256(screenshot_path) == screenshot.get("sha256")
        )
        if not checks["screenshot_present"]:
            errors.append("screenshot is missing")
        elif not checks["screenshot_sha256_matches"]:
            errors.append("screenshot SHA-256 does not match manifest")
    result = {"status": "PASS" if not errors else "FAIL", "checks": checks, "errors": errors}
    print("PIZZA_YARD_VALIDATION " + json.dumps(result, sort_keys=True), flush=True)
    return 0 if not errors else 1


def main() -> int:
    args = _parse_args()
    if args.inventory_only:
        return inventory(args)
    if args.validate_blend is not None:
        return validate(args)
    return build(args)


if __name__ == "__main__":
    raise SystemExit(main())
