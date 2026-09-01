#!/usr/bin/env python3
"""Bake the reviewed CC BY metal bridge into one engine-ready runtime GLB.

Run with Blender from the repository root::

    blender --background --factory-startup --python-exit-code 1 \
      --python tools/bake_city_metal_bridge.py -- \
      /path/to/Pont.fbx /path/to/textures \
      assets/flavor/Structures/city-metal-bridge-remidoes3d-cc-by-4/baked

The source geometry is already economical, so all 7,384 triangles are retained.  Its four material
textures are resized into a single embedded 2x2 atlas for Kessel's first-material-wins GLB loader.
The placement origin comes from the upper face of the model's broad road slab: glTF Y=0 is the
driveable deck, while the long structural axis remains glTF X.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import reviewed_static_asset_recipe as recipe  # noqa: E402
import reviewed_static_blender as common  # noqa: E402


SOURCE_TRIANGLES = 7_384
SOURCE_DIMENSIONS_BLENDER_M = (19.978382, 4.065169, 3.508359)


def parse_arguments() -> argparse.Namespace:
    try:
        separator = sys.argv.index("--")
    except ValueError as error:
        raise SystemExit("expected: -- <Pont.fbx> <textures-directory> <output-directory>") from error
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source_fbx", type=Path)
    parser.add_argument("texture_directory", type=Path)
    parser.add_argument("output_directory", type=Path)
    arguments = parser.parse_args(sys.argv[separator + 1 :])
    for name in ("source_fbx", "texture_directory", "output_directory"):
        setattr(arguments, name, getattr(arguments, name).expanduser().resolve())
    return arguments


def material_tile(material_name: str) -> int:
    expected = {"pont1": 0, "pont2": 1, "pont3": 2, "pont4": 3}
    try:
        return expected[material_name.casefold()]
    except KeyError as error:
        raise RuntimeError(f"unreviewed bridge material family: {material_name!r}") from error


def gltf_bounds_from_blender(minimum, maximum) -> tuple[list[float], list[float]]:
    gltf_minimum = (minimum.x, minimum.z, -maximum.y)
    gltf_maximum = (maximum.x, maximum.z, -minimum.y)
    return common.round_vector(gltf_minimum), common.round_vector(gltf_maximum)


def horizontal_faces(meshes) -> list[tuple[float, float]]:
    faces: list[tuple[float, float]] = []
    for obj in meshes:
        normal_matrix = obj.matrix_world.to_3x3()
        for polygon in obj.data.polygons:
            world_normal = (normal_matrix @ polygon.normal).normalized()
            if abs(world_normal.z) < 0.99999:
                continue
            center = obj.matrix_world @ polygon.center
            # Transforms are applied before this measurement, so polygon.area is square metres.
            faces.append((float(center.z), float(polygon.area)))
    return faces


def main() -> None:
    arguments = parse_arguments()
    observed_hashes = recipe.validate_reviewed_inputs(
        recipe.BRIDGE, arguments.source_fbx, arguments.texture_directory
    )
    meshes = common.import_reviewed_fbx(arguments.source_fbx)
    common.apply_object_transforms(meshes)
    source_minimum, source_maximum = common.exact_world_bounds(meshes)
    source_dimensions = source_maximum - source_minimum
    source_triangles = common.triangle_count(meshes)
    source_vertices = common.vertex_count(meshes)
    source_object_count = len(meshes)
    source_materials = sorted(
        {
            slot.material.name
            for obj in meshes
            for slot in obj.material_slots
            if slot.material is not None
        }
    )
    if source_triangles != SOURCE_TRIANGLES:
        raise RuntimeError(
            f"reviewed bridge topology changed: expected {SOURCE_TRIANGLES} triangles, "
            f"got {source_triangles}"
        )
    for observed, expected in zip(source_dimensions, SOURCE_DIMENSIONS_BLENDER_M):
        if abs(observed - expected) > 2.0e-5:
            raise RuntimeError(
                "reviewed bridge dimensions changed: "
                f"expected {SOURCE_DIMENSIONS_BLENDER_M}, got {tuple(source_dimensions)}"
            )
    deck_source_z = recipe.select_bridge_deck_plane(horizontal_faces(meshes))

    common.triangulate(meshes)
    if common.triangle_count(meshes) != SOURCE_TRIANGLES:
        raise RuntimeError("bridge triangulation did not preserve the reviewed triangle count")
    material_polygon_counts = common.remap_material_uvs(meshes, material_tile)

    textures = arguments.texture_directory
    base_color_atlas = common.create_atlas(
        "Kessel_CityMetalBridge_BaseColor_Atlas",
        {
            0: common.load_base_color_tile(textures / "Pont_Pont1_BaseColor.png"),
            1: common.load_base_color_tile(textures / "Pont_Pont2_BaseColor.png"),
            2: common.load_base_color_tile(textures / "Pont_Pont3_BaseColor.png"),
            3: common.load_base_color_tile(textures / "Pont_Pont4_BaseColor.png"),
        },
        colorspace="sRGB",
        default_color=(0.20, 0.20, 0.20, 1.0),
    )
    material = common.new_base_color_material(
        "Kessel_CityMetalBridge_PaintedMetal",
        base_color_atlas,
        roughness=0.72,
    )

    joined = common.join_meshes(meshes, "city_metal_bridge_runtime")
    common.assign_only_material(joined, material)
    placement_offset = common.center_xy_and_set_deck((joined,), deck_source_z)
    baked_minimum, baked_maximum = common.exact_world_bounds((joined,))
    baked_dimensions = baked_maximum - baked_minimum
    baked_triangles = common.triangle_count((joined,))
    baked_vertices = common.vertex_count((joined,))
    if baked_triangles != SOURCE_TRIANGLES:
        raise RuntimeError(
            f"bridge runtime topology changed: expected {SOURCE_TRIANGLES}, got {baked_triangles}"
        )
    if not baked_minimum.z < 0.0 < baked_maximum.z:
        raise RuntimeError("bridge deck origin no longer lies inside the structure height")
    deck_baked_z = deck_source_z + placement_offset.z
    if abs(deck_baked_z) > 1.0e-6:
        raise RuntimeError(f"bridge deck is not Blender Z=0: {deck_baked_z:.9f}")

    output_path = arguments.output_directory / recipe.BRIDGE.output_filename
    report_path = arguments.output_directory / recipe.BRIDGE.report_filename
    common.export_glb(joined, output_path)
    gltf_minimum, gltf_maximum = gltf_bounds_from_blender(baked_minimum, baked_maximum)
    report = {
        "schema": "kessel.reviewed-static-asset-bake.v1",
        "recipe_version": recipe.RECIPE_VERSION,
        "asset": {
            "key": recipe.BRIDGE.key,
            "title": recipe.BRIDGE.title,
            "creator": recipe.BRIDGE.creator,
            "sketchfab_uid": recipe.BRIDGE.sketchfab_uid,
            "source_url": recipe.BRIDGE.source_url,
            "license": "CC-BY-4.0",
        },
        "blender": common.blender_version_record(),
        "toolchain": recipe.toolchain_receipt(recipe.BRIDGE),
        "source_archive": recipe.source_archive_record(recipe.BRIDGE),
        "inputs": {
            "sha256": dict(sorted(observed_hashes.items())),
            "source_objects": source_object_count,
            "source_vertices": source_vertices,
            "source_triangles": source_triangles,
            "source_materials": source_materials,
            "source_bounds_blender_m": {
                "min": common.round_vector(source_minimum),
                "max": common.round_vector(source_maximum),
                "dimensions": common.round_vector(source_dimensions),
            },
        },
        "conversion": {
            "all_source_mesh_objects_retained": True,
            "preview_or_ground_geometry_detected": False,
            "excluded_texture_inputs": {
                "internal_ground_ao_texture.jpeg": (
                    "not referenced by the reviewed bridge mesh and not used to bake fake shading"
                )
            },
            "triangulation": "Blender BEAUTY triangulation; triangle count preserved",
            "decimation": "none; source geometry is already within runtime budget",
            "atlas": {
                "size_px": [recipe.ATLAS_SIZE_PX, recipe.ATLAS_SIZE_PX],
                "tile_size_px": [recipe.ATLAS_TILE_SIZE_PX, recipe.ATLAS_TILE_SIZE_PX],
                "tiles": {
                    "0": "Pont1 base color",
                    "1": "Pont2 base color",
                    "2": "Pont3 base color",
                    "3": "Pont4 base color",
                },
                "embedded_channels": ["base_color"],
            },
            "material_polygon_counts": material_polygon_counts,
            "joined_runtime_meshes": 1,
            "runtime_materials": 1,
            "deck_plane_source_blender_z_m": round(deck_source_z, 6),
            "placement_offset_blender_m": common.round_vector(placement_offset),
        },
        "runtime_geometry": {
            "blender_vertices_before_gltf_attribute_splits": baked_vertices,
            "triangles": baked_triangles,
            "gltf_up_axis": "+Y",
            "gltf_deck": {"axis": "Y", "value_m": 0.0},
            "gltf_long_axis": "X",
            "origin": "driveable road-deck plane at plan bounds center",
            "bounds_m": {"min": gltf_minimum, "max": gltf_maximum},
            "dimensions_m": common.round_vector(
                recipe.blender_dimensions_to_gltf(baked_dimensions)
            ),
            "source_dimensions_blender_xyz_m": common.round_vector(source_dimensions),
        },
        "output": common.output_record(output_path),
    }
    common.write_report(report_path, report)
    print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
