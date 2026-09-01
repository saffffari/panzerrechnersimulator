#!/usr/bin/env python3
"""Bake the reviewed CC BY Enigma Machine into one optimized runtime GLB.

Run with Blender from the repository root::

    blender --background --factory-startup --python-exit-code 1 \
      --python tools/bake_enigma_machine.py -- \
      /path/to/low.fbx /path/to/textures \
      assets/flavor/Props/enigma-machine-ashish0096-cc-by-4/baked

The exact reviewed FBX and texture hashes are mandatory.  The original has 147,636 triangles and
five material slots spread across many small objects.  This bake triangulates deterministically,
uses a per-part collapse budget that retains disconnected controls, and emits a single joined mesh
within the 10k--25k runtime target.  Two source PBR texture families and the small glass element are
packed into one 2x2 atlas per channel because Kessel's current GLB loader is first-material-wins.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

import bpy

sys.path.insert(0, str(Path(__file__).resolve().parent))
import reviewed_static_asset_recipe as recipe  # noqa: E402
import reviewed_static_blender as common  # noqa: E402


SOURCE_TRIANGLES = 147_636
DECIMATION_FRACTION = 0.125
MINIMUM_TRIANGLES_PER_PART = 36
TARGET_TRIANGLES_MIN = 10_000
TARGET_TRIANGLES_MAX = 25_000


def parse_arguments() -> argparse.Namespace:
    try:
        separator = sys.argv.index("--")
    except ValueError as error:
        raise SystemExit("expected: -- <low.fbx> <textures-directory> <output-directory>") from error
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source_fbx", type=Path)
    parser.add_argument("texture_directory", type=Path)
    parser.add_argument("output_directory", type=Path)
    arguments = parser.parse_args(sys.argv[separator + 1 :])
    for name in ("source_fbx", "texture_directory", "output_directory"):
        setattr(arguments, name, getattr(arguments, name).expanduser().resolve())
    return arguments


def material_tile(material_name: str) -> int:
    normalized = material_name.casefold()
    if normalized == "glass":
        return 2
    if "box" in normalized:
        return 0
    if "mec" in normalized:
        return 1
    raise RuntimeError(f"unreviewed Enigma material family: {material_name!r}")


def gltf_bounds_from_blender(minimum, maximum) -> tuple[list[float], list[float]]:
    gltf_minimum = (minimum.x, minimum.z, -maximum.y)
    gltf_maximum = (maximum.x, maximum.z, -minimum.y)
    return common.round_vector(gltf_minimum), common.round_vector(gltf_maximum)


def main() -> None:
    arguments = parse_arguments()
    observed_hashes = recipe.validate_reviewed_inputs(
        recipe.ENIGMA, arguments.source_fbx, arguments.texture_directory
    )
    meshes = common.import_reviewed_fbx(arguments.source_fbx)
    common.apply_object_transforms(meshes)
    source_minimum, source_maximum = common.exact_world_bounds(meshes)
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
            f"reviewed Enigma topology changed: expected {SOURCE_TRIANGLES} triangles, "
            f"got {source_triangles}"
        )

    common.triangulate(meshes)
    part_records = common.decimate_to_fraction_with_floor(
        meshes,
        fraction=DECIMATION_FRACTION,
        minimum_triangles_per_part=MINIMUM_TRIANGLES_PER_PART,
    )
    material_polygon_counts = common.remap_material_uvs(meshes, material_tile)

    textures = arguments.texture_directory
    base_color_atlas = common.create_atlas(
        "Kessel_Enigma_BaseColor_Atlas",
        {
            0: common.load_base_color_tile(textures / "low_box_Base_color.png"),
            1: common.load_base_color_tile(textures / "low_mec_Base_color.png"),
            # The runtime's one-material path cannot preserve a separate transparent shader.
            # Keep the six-triangle display pane as a dark, smooth dielectric instead of silently
            # pretending opaque geometry is refractive glass.
            2: common.constant_tile((0.035, 0.045, 0.04, 1.0)),
        },
        colorspace="sRGB",
        default_color=(0.10, 0.08, 0.055, 1.0),
    )
    metallic_roughness_atlas = common.create_atlas(
        "Kessel_Enigma_MetallicRoughness_Atlas",
        {
            0: common.load_metallic_roughness_tile(
                textures / "low_box_Roughness.png",
                textures / "low_box_Metallic.png",
            ),
            1: common.load_metallic_roughness_tile(
                textures / "low_mec_Roughness.png",
                textures / "low_mec_Metallic.png",
            ),
            2: common.constant_tile((1.0, 0.15, 0.0, 1.0)),
        },
        colorspace="Non-Color",
        default_color=(1.0, 0.80, 0.0, 1.0),
    )
    normal_atlas = common.create_atlas(
        "Kessel_Enigma_Normal_Atlas",
        {
            0: common.load_opengl_normal_tile(
                textures / "low_box_Normal_DirectX.png"
            ),
            1: common.load_opengl_normal_tile(
                textures / "low_mec_Normal_DirectX.png"
            ),
            2: common.constant_tile((0.5, 0.5, 1.0, 1.0)),
        },
        colorspace="Non-Color",
        default_color=(0.5, 0.5, 1.0, 1.0),
    )
    material = common.new_pbr_material(
        "Kessel_Enigma_PBR",
        base_color_atlas,
        metallic_roughness_atlas,
        normal_atlas,
    )

    joined = common.join_meshes(meshes, "enigma_machine_runtime")
    common.assign_only_material(joined, material)
    placement_offset = common.center_xy_and_ground((joined,))
    baked_minimum, baked_maximum = common.exact_world_bounds((joined,))
    baked_triangles = common.triangle_count((joined,))
    baked_vertices = common.vertex_count((joined,))
    if not TARGET_TRIANGLES_MIN <= baked_triangles <= TARGET_TRIANGLES_MAX:
        raise RuntimeError(
            f"optimized Enigma has {baked_triangles} triangles; expected "
            f"{TARGET_TRIANGLES_MIN}..{TARGET_TRIANGLES_MAX}"
        )
    if abs(baked_minimum.z) > 1.0e-6:
        raise RuntimeError(f"Enigma floor is not Blender Z=0: {baked_minimum.z:.9f}")
    baked_dimensions = baked_maximum - baked_minimum
    if not (0.40 <= baked_dimensions.x <= 0.46 and 0.50 <= baked_dimensions.z <= 0.58):
        raise RuntimeError(
            "optimized Enigma dimensions left the reviewed physical envelope: "
            f"{tuple(baked_dimensions)}"
        )

    output_path = arguments.output_directory / recipe.ENIGMA.output_filename
    report_path = arguments.output_directory / recipe.ENIGMA.report_filename
    common.export_glb(joined, output_path)
    gltf_minimum, gltf_maximum = gltf_bounds_from_blender(baked_minimum, baked_maximum)
    report = {
        "schema": "kessel.reviewed-static-asset-bake.v1",
        "recipe_version": recipe.RECIPE_VERSION,
        "asset": {
            "key": recipe.ENIGMA.key,
            "title": recipe.ENIGMA.title,
            "creator": recipe.ENIGMA.creator,
            "sketchfab_uid": recipe.ENIGMA.sketchfab_uid,
            "source_url": recipe.ENIGMA.source_url,
            "license": "CC-BY-4.0",
        },
        "blender": common.blender_version_record(),
        "toolchain": recipe.toolchain_receipt(recipe.ENIGMA),
        "source_archive": recipe.source_archive_record(recipe.ENIGMA),
        "inputs": {
            "sha256": dict(sorted(observed_hashes.items())),
            "source_objects": source_object_count,
            "source_vertices": source_vertices,
            "source_triangles": source_triangles,
            "source_materials": source_materials,
            "source_bounds_blender_m": {
                "min": common.round_vector(source_minimum),
                "max": common.round_vector(source_maximum),
                "dimensions": common.round_vector(source_maximum - source_minimum),
            },
        },
        "conversion": {
            "all_source_mesh_objects_retained": True,
            "preview_or_ground_geometry_detected": False,
            "excluded_texture_inputs": {
                "low_box_Mixed_AO.png": (
                    "not baked into base color; physical ambient occlusion must come from lighting"
                ),
                "low_mec_Mixed_AO.png": (
                    "not baked into base color; physical ambient occlusion must come from lighting"
                ),
            },
            "triangulation": "Blender BEAUTY triangulation before collapse",
            "decimation": {
                "method": "per-object Blender COLLAPSE",
                "fraction": DECIMATION_FRACTION,
                "minimum_triangles_per_disconnected_part": MINIMUM_TRIANGLES_PER_PART,
                "parts": part_records,
                "target_range_triangles": [TARGET_TRIANGLES_MIN, TARGET_TRIANGLES_MAX],
            },
            "atlas": {
                "size_px": [recipe.ATLAS_SIZE_PX, recipe.ATLAS_SIZE_PX],
                "tile_size_px": [recipe.ATLAS_TILE_SIZE_PX, recipe.ATLAS_TILE_SIZE_PX],
                "tiles": {
                    "0": "wooden box PBR set",
                    "1": "mechanism PBR set",
                    "2": "opaque smooth dielectric display pane",
                    "3": "unused neutral fallback",
                },
                "normal_conversion": "DirectX green channel inverted to glTF/OpenGL convention",
                "embedded_channels": ["base_color", "metallic_roughness", "normal"],
            },
            "material_polygon_counts_after_decimation": material_polygon_counts,
            "joined_runtime_meshes": 1,
            "runtime_materials": 1,
            "placement_offset_blender_m": common.round_vector(placement_offset),
        },
        "runtime_geometry": {
            "blender_vertices_before_gltf_attribute_splits": baked_vertices,
            "triangles": baked_triangles,
            "gltf_up_axis": "+Y",
            "gltf_forward_axis": "+Z",
            "gltf_floor": {"axis": "Y", "value_m": 0.0},
            "origin": "floor contact at XY footprint bounds center",
            "bounds_m": {"min": gltf_minimum, "max": gltf_maximum},
            "dimensions_m": common.round_vector(
                recipe.blender_dimensions_to_gltf(baked_dimensions)
            ),
        },
        "output": common.output_record(output_path),
    }
    common.write_report(report_path, report)
    print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
