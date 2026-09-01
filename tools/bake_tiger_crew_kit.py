#!/usr/bin/env python3
"""Bake the reviewed AdrielCZ Panzer crew kit into Kessel runtime GLBs.

Run with Blender, not system Python::

    blender --background --factory-startup --python-exit-code 1 \
      --python tools/bake_tiger_crew_kit.py -- \
      /private/tmp/panzer-crew-reviewed-extract-v2 \
      assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4

The private extraction is re-hashed against its deterministic receipt.  The raw ZIP/FBX is never
copied into the asset directory.  The source-native 39-bone rig is retained only in the authoring
master.  Runtime files contain evaluated static geometry and exactly one embedded base-color atlas
material, matching the current loader's first-material-wins behavior.
"""

from __future__ import annotations

import argparse
from array import array
import json
from math import radians
from pathlib import Path
import sys
import tempfile
from typing import Any, Iterable, Sequence

import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import tiger_crew_recipe as recipe  # noqa: E402


ARMATURE_NAME = "PanzerTruppe_N1"
BODY_NAME = "PanzerTruppe_Body_N1"
LEFT_ARM_NAME = "PanzerTruppe_L_Arm_N1"
RIGHT_ARM_NAME = "PanzerTruppe_R_Arm_N1"
HEAD_NAME = "PanzerTruppe_Head_N1"
CHARACTER_NAMES = (BODY_NAME, LEFT_ARM_NAME, RIGHT_ARM_NAME, HEAD_NAME)
ALT_HEAD_NAMES = tuple(f"PanzerTruppe_Head_N{index}" for index in range(2, 6))
BINOCULAR_NAME = "GER_Binoculars_N1"
SOURCE_FBX_RELATIVE = "source/Free Tiger and Crew FBX.fbx"
RECEIPT_RELATIVE = "extraction.receipt.json"
TEXTURE_RELATIVES = {
    "body": "textures/reviewed/body_basecolor.png",
    **{
        identity: f"textures/reviewed/face_{index:02d}_basecolor.png"
        for index, identity in enumerate(recipe.IDENTITIES, 1)
    },
    "accessories": "textures/reviewed/accessories_basecolor.png",
}

CHARACTER_ATLAS_SIZE = (1024, 1024)
CHARACTER_TILES = {
    "body": {"x": 4, "y": 4, "width": 512, "height": 512, "gutter": 4},
    "arm_head": {"x": 524, "y": 4, "width": 256, "height": 256, "gutter": 4},
}
PROP_ATLAS_SIZE = (512, 512)
PROP_TILE = {"x": 4, "y": 4, "width": 256, "height": 256, "gutter": 4}

POSE_ROTATIONS_DEGREES = {
    "standing": {
        "L_Arm_N1": (0.0, -4.0, -72.0),
        "R_Arm_N1": (0.0, 4.0, 72.0),
        "L_Elbow_N1": (0.0, -8.0, -12.0),
        "R_Elbow_N1": (0.0, 8.0, 12.0),
    },
    "seated": {
        "L_Arm_N1": (0.0, 0.0, -72.0),
        "R_Arm_N1": (0.0, 0.0, 72.0),
        "L_Elbow_N1": (0.0, -20.0, -24.0),
        "R_Elbow_N1": (0.0, 20.0, 24.0),
        "L_Leg_N1": (-72.0, 0.0, 0.0),
        "R_Leg_N1": (-72.0, 0.0, 0.0),
        "L_Knee_N1": (95.0, 0.0, 0.0),
        "R_Knee_N1": (95.0, 0.0, 0.0),
    },
    "relaxed": {
        "Chest_N1": (4.0, 0.0, 0.0),
        "Neck_N1": (-3.0, 0.0, 0.0),
        "L_Arm_N1": (0.0, -7.0, -68.0),
        "R_Arm_N1": (0.0, 9.0, 75.0),
        "L_Elbow_N1": (0.0, -18.0, -30.0),
        "R_Elbow_N1": (0.0, 12.0, 19.0),
        "L_Leg_N1": (-2.0, 0.0, 1.5),
        "R_Leg_N1": (1.0, 0.0, -1.0),
    },
}


def parse_arguments() -> argparse.Namespace:
    try:
        separator = sys.argv.index("--")
    except ValueError as error:
        raise SystemExit("expected: -- <reviewed-extraction> <asset-root>") from error
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("extraction_root", type=Path)
    parser.add_argument("asset_root", type=Path)
    arguments = parser.parse_args(sys.argv[separator + 1 :])
    arguments.extraction_root = arguments.extraction_root.expanduser().resolve()
    arguments.asset_root = arguments.asset_root.expanduser().resolve()
    if not arguments.extraction_root.is_dir():
        raise SystemExit(f"reviewed extraction does not exist: {arguments.extraction_root}")
    if arguments.asset_root.suffix.casefold() == ".zip":
        raise SystemExit("asset root must be a directory, not an archive")
    return arguments


def clear_scene() -> None:
    if bpy.context.object is not None and bpy.context.object.mode != "OBJECT":
        bpy.ops.object.mode_set(mode="OBJECT")
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for datablocks in (
        bpy.data.meshes,
        bpy.data.armatures,
        bpy.data.materials,
        bpy.data.images,
        bpy.data.actions,
        bpy.data.cameras,
        bpy.data.lights,
    ):
        for datablock in list(datablocks):
            if datablock.users == 0:
                datablocks.remove(datablock)


def supported_keywords(operator, values: dict[str, Any]) -> dict[str, Any]:
    supported = {item.identifier for item in operator.get_rna_type().properties}
    return {key: value for key, value in values.items() if key in supported}


def verify_extraction(root: Path) -> tuple[dict[str, Any], dict[str, Any]]:
    receipt_path = root / RECEIPT_RELATIVE
    if not receipt_path.is_file():
        raise RuntimeError(f"reviewed extraction receipt is missing: {receipt_path}")
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    if receipt.get("schema") != "kessel.tiger-crew-source-extraction" or receipt.get("version") != 1:
        raise RuntimeError("unsupported Tiger crew extraction receipt")
    archive = receipt.get("archive", {})
    if archive.get("sha256") != recipe.ARCHIVE_SHA256:
        raise RuntimeError("extraction receipt does not identify the reviewed archive")
    receipt_members = receipt.get("members", [])
    if len(receipt_members) != 50:
        raise RuntimeError(
            f"reviewed extraction receipt must contain exactly 50 members, found {len(receipt_members)}"
        )
    members = {str(member.get("output")): member for member in receipt_members}
    if len(members) != len(receipt_members):
        raise RuntimeError("reviewed extraction receipt contains duplicate output paths")
    required = (SOURCE_FBX_RELATIVE, *TEXTURE_RELATIVES.values())
    extracted_records = []
    for relative in required:
        record = members.get(relative)
        if record is None:
            raise RuntimeError(f"required reviewed member is absent from extraction receipt: {relative}")
        path = root.joinpath(*Path(relative).parts)
        if not path.is_file():
            raise RuntimeError(f"required reviewed member is missing: {path}")
        digest = recipe.sha256_file(path)
        size = path.stat().st_size
        if digest != record.get("sha256") or size != int(record.get("bytes", -1)):
            raise RuntimeError(f"reviewed extraction member changed after extraction: {relative}")
        extracted_records.append(recipe.file_record(path, logical_path=relative))
    if members[SOURCE_FBX_RELATIVE]["sha256"] != recipe.SOURCE_FBX_SHA256:
        raise RuntimeError("reviewed FBX hash does not match the audited source")
    return receipt, {
        "original_archive": {
            "filename": archive.get("filename"),
            "bytes": int(archive.get("bytes", 0)),
            "sha256": archive.get("sha256"),
        },
        "extraction_receipt": recipe.file_record(receipt_path, logical_path=RECEIPT_RELATIVE),
        "extracted_files": sorted(extracted_records, key=lambda item: item["path"]),
    }


def import_source(source_fbx: Path) -> None:
    clear_scene()
    result = bpy.ops.import_scene.fbx(
        **supported_keywords(
            bpy.ops.import_scene.fbx,
            {
                "filepath": str(source_fbx),
                "use_anim": False,
                "use_custom_normals": True,
                "use_image_search": False,
                "automatic_bone_orientation": False,
            },
        )
    )
    if "FINISHED" not in result:
        raise RuntimeError(f"reviewed FBX import failed: {result}")


def mesh_topology(obj: bpy.types.Object) -> dict[str, int]:
    obj.data.calc_loop_triangles()
    return {
        "vertices": len(obj.data.vertices),
        "edges": len(obj.data.edges),
        "polygons": len(obj.data.polygons),
        "triangles": len(obj.data.loop_triangles),
        "loops": len(obj.data.loops),
    }


def validate_source_scene() -> dict[str, Any]:
    armature = bpy.data.objects.get(ARMATURE_NAME)
    if armature is None or armature.type != "ARMATURE":
        raise RuntimeError(f"reviewed native armature {ARMATURE_NAME!r} is missing")
    bones = tuple(bone.name for bone in armature.data.bones)
    if bones != recipe.ARMATURE_BONES:
        raise RuntimeError("reviewed native 39-bone order/name contract changed")
    if bpy.data.actions:
        raise RuntimeError("reviewed source unexpectedly contains animation actions")

    source_meshes = {}
    for name, expected in recipe.SOURCE_MESH_TOPOLOGY.items():
        obj = bpy.data.objects.get(name)
        if obj is None or obj.type != "MESH":
            raise RuntimeError(f"reviewed weighted mesh {name!r} is missing")
        actual = mesh_topology(obj)
        for field, value in expected.items():
            if actual[field] != value:
                raise RuntimeError(f"reviewed topology changed on {name}: {field}={actual[field]}")
        modifiers = [modifier for modifier in obj.modifiers if modifier.type == "ARMATURE"]
        if len(modifiers) != 1 or modifiers[0].object is not armature:
            raise RuntimeError(f"reviewed mesh {name!r} is not bound to the native armature")
        if any(not vertex.groups for vertex in obj.data.vertices):
            raise RuntimeError(f"reviewed weighted mesh {name!r} contains an unweighted vertex")
        if not obj.data.uv_layers or len(obj.data.uv_layers.active.data) != len(obj.data.loops):
            raise RuntimeError(f"reviewed weighted mesh {name!r} has no complete UV layer")
        source_meshes[name] = actual
    if sum(item["vertices"] for item in source_meshes.values()) != recipe.CHARACTER_VERTICES:
        raise RuntimeError("reviewed character vertex total changed")
    if sum(item["triangles"] for item in source_meshes.values()) != recipe.CHARACTER_TRIANGLES:
        raise RuntimeError("reviewed character triangle total changed")

    head = bpy.data.objects[HEAD_NAME]
    head_polygons = [tuple(polygon.vertices) for polygon in head.data.polygons]
    head_uv = [tuple(float(value) for value in loop.uv) for loop in head.data.uv_layers.active.data]
    alt_heads = {}
    for name in ALT_HEAD_NAMES:
        obj = bpy.data.objects.get(name)
        if obj is None or obj.type != "MESH":
            raise RuntimeError(f"reviewed display head {name!r} is missing")
        topology = mesh_topology(obj)
        if topology["vertices"] != 64 or topology["polygons"] != 58 or topology["triangles"] != 116:
            raise RuntimeError(f"reviewed display head topology changed on {name}")
        if [tuple(polygon.vertices) for polygon in obj.data.polygons] != head_polygons:
            raise RuntimeError(f"reviewed display head polygon indices differ on {name}")
        uv = [tuple(float(value) for value in loop.uv) for loop in obj.data.uv_layers.active.data]
        if len(uv) != len(head_uv) or any((Vector(first) - Vector(second)).length > 1.0e-7 for first, second in zip(uv, head_uv)):
            raise RuntimeError(f"reviewed display head UV topology differs on {name}")
        alt_heads[name] = {**topology, "used_for_runtime_geometry": False}

    binoculars = bpy.data.objects.get(BINOCULAR_NAME)
    if binoculars is None or binoculars.type != "MESH":
        raise RuntimeError("reviewed handheld binocular object is missing")
    binocular_topology = mesh_topology(binoculars)
    if binocular_topology["vertices"] != recipe.BINOCULAR_VERTICES or binocular_topology["triangles"] != recipe.BINOCULAR_TRIANGLES:
        raise RuntimeError("reviewed handheld binocular topology changed")

    head_point = armature.matrix_world @ armature.data.bones["Head_N1"].head_local
    eyes_point = armature.matrix_world @ armature.data.bones["Eyes_N1"].head_local
    if eyes_point.y >= head_point.y - 0.05:
        raise RuntimeError("source facial-forward landmark no longer points along Blender -Y")
    return {
        "armature": {
            "object": ARMATURE_NAME,
            "bone_count": len(bones),
            "bones": list(bones),
            "retargeted": False,
            "source_native_skin_weights_retained": True,
        },
        "weighted_character_meshes": source_meshes,
        "character_totals": {
            "vertices": recipe.CHARACTER_VERTICES,
            "triangles": recipe.CHARACTER_TRIANGLES,
        },
        "display_heads": alt_heads,
        "identity_geometry": (
            "weighted PanzerTruppe_Head_N1 for every identity; N2-N5 are topology-compatible "
            "unskinned display copies and are intentionally not rebound"
        ),
        "binoculars": binocular_topology,
        "orientation_evidence": {
            "blender_face_forward": "-Y",
            "gltf_face_forward": "+Z",
            "head_bone_world_y_m": round(float(head_point.y), 6),
            "eyes_bone_world_y_m": round(float(eyes_point.y), 6),
        },
    }


def exact_bounds(objects: Sequence[bpy.types.Object]) -> tuple[Vector, Vector]:
    points = [obj.matrix_world @ vertex.co for obj in objects for vertex in obj.data.vertices]
    if not points:
        raise RuntimeError("selection has no mesh vertices")
    minimum = Vector(tuple(min(point[axis] for point in points) for axis in range(3)))
    maximum = Vector(tuple(max(point[axis] for point in points) for axis in range(3)))
    return minimum, maximum


def round_vector(values: Iterable[float]) -> list[float]:
    return [round(float(value), 6) for value in values]


def character_rest_normalization() -> dict[str, Any]:
    meshes = [bpy.data.objects[name] for name in CHARACTER_NAMES]
    minimum, maximum = exact_bounds(meshes)
    height = maximum.z - minimum.z
    if not 1.70 < height < 1.73:
        raise RuntimeError(f"reviewed rest height changed unexpectedly: {height}")
    scale = recipe.TARGET_HEIGHT_M / height
    return {
        "source_bounds_min_m": round_vector(minimum),
        "source_bounds_max_m": round_vector(maximum),
        "source_height_m": round(float(height), 9),
        "uniform_scale": round(float(scale), 12),
        "target_standing_height_m": recipe.TARGET_HEIGHT_M,
        "policy": "one rest-height scale reused for standing, seated, and relaxed poses",
    }


def reset_and_apply_pose(armature: bpy.types.Object, pose_name: str) -> None:
    rotations = POSE_ROTATIONS_DEGREES[pose_name]
    for bone in armature.pose.bones:
        bone.rotation_mode = "XYZ"
        bone.location = (0.0, 0.0, 0.0)
        bone.rotation_euler = (0.0, 0.0, 0.0)
        bone.scale = (1.0, 1.0, 1.0)
    for bone_name, degrees in rotations.items():
        armature.pose.bones[bone_name].rotation_euler = tuple(radians(value) for value in degrees)
    bpy.context.view_layer.update()


def bake_character_skin_to_static() -> dict[str, bpy.types.Object]:
    depsgraph = bpy.context.evaluated_depsgraph_get()
    result: dict[str, bpy.types.Object] = {}
    for source_name in CHARACTER_NAMES:
        source = bpy.data.objects[source_name]
        evaluated = source.evaluated_get(depsgraph)
        mesh = bpy.data.meshes.new_from_object(
            evaluated,
            preserve_all_data_layers=True,
            depsgraph=depsgraph,
        )
        mesh.transform(evaluated.matrix_world)
        mesh.update()
        obj = bpy.data.objects.new(f"{source_name}_Static", mesh)
        bpy.context.scene.collection.objects.link(obj)
        result[source_name] = obj
    for obj in list(bpy.context.scene.objects):
        if obj not in result.values():
            bpy.data.objects.remove(obj, do_unlink=True)
    bpy.context.view_layer.update()
    for source_name, obj in result.items():
        expected = recipe.SOURCE_MESH_TOPOLOGY[source_name]
        topology = mesh_topology(obj)
        if any(topology[field] != value for field, value in expected.items()):
            raise RuntimeError(f"evaluated static topology changed on {source_name}")
    return result


def transform_static_character(
    objects: Sequence[bpy.types.Object],
    *,
    scale: float,
) -> tuple[Vector, Vector]:
    for obj in objects:
        obj.data.transform(Matrix.Scale(scale, 4))
        obj.data.update()
    minimum, maximum = exact_bounds(objects)
    translation = Vector((-(minimum.x + maximum.x) * 0.5, 0.0, -minimum.z))
    for obj in objects:
        obj.data.transform(Matrix.Translation(translation))
        obj.data.update()
    minimum, maximum = exact_bounds(objects)
    if abs(minimum.z) > 1.0e-6:
        raise RuntimeError("static character did not land on Blender Z=0/glTF Y=0")
    return minimum, maximum


def remap_uv(obj: bpy.types.Object, tile: dict[str, int], atlas_size: tuple[int, int]) -> None:
    uv_layer = obj.data.uv_layers.active
    if uv_layer is None:
        raise RuntimeError(f"mesh {obj.name!r} has no active UV layer")
    width, height = atlas_size
    for loop in uv_layer.data:
        source_u, source_v = float(loop.uv.x), float(loop.uv.y)
        if not (-1.0e-6 <= source_u <= 1.000001 and -1.0e-6 <= source_v <= 1.000001):
            raise RuntimeError(f"mesh {obj.name!r} has UVs outside the reviewed [0,1] range")
        loop.uv.x = (tile["x"] + source_u * tile["width"]) / width
        loop.uv.y = (tile["y"] + source_v * tile["height"]) / height
    obj.data.update()


def image_pixels(image: bpy.types.Image) -> array:
    width, height = image.size
    values = array("f", [0.0]) * (width * height * 4)
    image.pixels.foreach_get(values)
    return values


def copy_tile(
    destination: array,
    destination_size: tuple[int, int],
    source: bpy.types.Image,
    tile: dict[str, int],
) -> None:
    source_width, source_height = source.size
    if (source_width, source_height) != (tile["width"], tile["height"]):
        raise RuntimeError(
            f"reviewed texture {source.name!r} dimensions changed: "
            f"expected {(tile['width'], tile['height'])}, got {(source_width, source_height)}"
        )
    destination_width, destination_height = destination_size
    pixels = image_pixels(source)
    x0, y0, gutter = tile["x"], tile["y"], tile["gutter"]
    if x0 - gutter < 0 or y0 - gutter < 0 or x0 + source_width + gutter > destination_width or y0 + source_height + gutter > destination_height:
        raise RuntimeError("atlas tile and gutter exceed the generated image")

    def set_pixel(x: int, y: int, rgba: array) -> None:
        offset = (y * destination_width + x) * 4
        destination[offset : offset + 4] = rgba

    for source_y in range(source_height):
        source_offset = source_y * source_width * 4
        destination_offset = ((y0 + source_y) * destination_width + x0) * 4
        destination[destination_offset : destination_offset + source_width * 4] = pixels[
            source_offset : source_offset + source_width * 4
        ]
        left = pixels[source_offset : source_offset + 4]
        right_offset = source_offset + (source_width - 1) * 4
        right = pixels[right_offset : right_offset + 4]
        for amount in range(1, gutter + 1):
            set_pixel(x0 - amount, y0 + source_y, left)
            set_pixel(x0 + source_width - 1 + amount, y0 + source_y, right)
    for amount in range(1, gutter + 1):
        source_bottom = y0
        source_top = y0 + source_height - 1
        for x in range(x0 - gutter, x0 + source_width + gutter):
            bottom_offset = (source_bottom * destination_width + x) * 4
            top_offset = (source_top * destination_width + x) * 4
            set_pixel(x, y0 - amount, destination[bottom_offset : bottom_offset + 4])
            set_pixel(x, y0 + source_height - 1 + amount, destination[top_offset : top_offset + 4])


def build_atlas_material(
    extraction_root: Path,
    temporary_directory: Path,
    *,
    identity: str | None,
    prop_only: bool = False,
) -> tuple[bpy.types.Material, dict[str, Any]]:
    if prop_only:
        atlas_size = PROP_ATLAS_SIZE
        sources = [("accessories", TEXTURE_RELATIVES["accessories"], PROP_TILE)]
        atlas_name = "Panzertruppe_Binoculars_BaseColor_Atlas"
    else:
        if identity not in recipe.IDENTITIES:
            raise RuntimeError(f"unknown reviewed face identity: {identity}")
        atlas_size = CHARACTER_ATLAS_SIZE
        sources = [
            ("body", TEXTURE_RELATIVES["body"], CHARACTER_TILES["body"]),
            ("arm_head", TEXTURE_RELATIVES[identity], CHARACTER_TILES["arm_head"]),
        ]
        atlas_name = f"Panzertruppe_{identity}_BaseColor_Atlas"

    background = array("f", (0.18, 0.18, 0.18, 1.0)) * (atlas_size[0] * atlas_size[1])
    tile_receipts = []
    loaded_images = []
    for semantic, relative, tile in sources:
        path = extraction_root.joinpath(*Path(relative).parts)
        image = bpy.data.images.load(str(path), check_existing=False)
        image.colorspace_settings.name = "sRGB"
        loaded_images.append(image)
        copy_tile(background, atlas_size, image, tile)
        tile_receipts.append(
            {
                "semantic": semantic,
                "source": relative,
                "source_sha256": recipe.sha256_file(path),
                "rect_px_lower_left": {
                    "x": tile["x"],
                    "y": tile["y"],
                    "width": tile["width"],
                    "height": tile["height"],
                },
                "gutter_px": tile["gutter"],
            }
        )
    atlas = bpy.data.images.new(atlas_name, width=atlas_size[0], height=atlas_size[1], alpha=True)
    atlas.colorspace_settings.name = "sRGB"
    atlas.pixels.foreach_set(background)
    atlas.update()
    atlas.file_format = "PNG"
    atlas_path = temporary_directory / f"{atlas_name}.png"
    atlas.filepath_raw = str(atlas_path)
    atlas.save()
    atlas.pack()
    for image in loaded_images:
        bpy.data.images.remove(image)

    material = bpy.data.materials.new(f"{atlas_name}_Material")
    material.use_nodes = True
    material.diffuse_color = (1.0, 1.0, 1.0, 1.0)
    material.metallic = 0.0
    material.roughness = 0.88
    nodes = material.node_tree.nodes
    nodes.clear()
    output = nodes.new("ShaderNodeOutputMaterial")
    shader = nodes.new("ShaderNodeBsdfPrincipled")
    shader.inputs["Metallic"].default_value = 0.0
    shader.inputs["Roughness"].default_value = 0.88
    texture = nodes.new("ShaderNodeTexImage")
    texture.name = atlas_name
    texture.image = atlas
    texture.interpolation = "Linear"
    material.node_tree.links.new(texture.outputs["Color"], shader.inputs["Base Color"])
    material.node_tree.links.new(shader.outputs["BSDF"], output.inputs["Surface"])
    return material, {
        "image_name": atlas_name,
        "dimensions_px": list(atlas_size),
        "color_space": "sRGB",
        "channel": "glTF baseColor",
        "tiles": tile_receipts,
        "single_material_policy": True,
    }


def assign_sole_material(objects: Sequence[bpy.types.Object], material: bpy.types.Material) -> None:
    for obj in objects:
        obj.data.materials.clear()
        obj.data.materials.append(material)
        for polygon in obj.data.polygons:
            polygon.material_index = 0
        obj.data.update()


def select_only(objects: Sequence[bpy.types.Object]) -> None:
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.hide_set(False)
        obj.hide_render = False
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]


def export_glb(
    output: Path,
    objects: Sequence[bpy.types.Object],
    *,
    skins: bool,
) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    select_only(objects)
    result = bpy.ops.export_scene.gltf(
        **supported_keywords(
            bpy.ops.export_scene.gltf,
            {
                "filepath": str(output),
                "export_format": "GLB",
                "use_selection": True,
                "export_apply": not skins,
                "export_texcoords": True,
                "export_normals": True,
                "export_materials": "EXPORT",
                "export_image_format": "AUTO",
                "export_skins": skins,
                "export_all_influences": skins,
                "export_animations": False,
                "export_yup": True,
            },
        )
    )
    if "FINISHED" not in result or not output.is_file():
        raise RuntimeError(f"GLB export failed for {output}: {result}")


def augmented_output_record(
    asset_root: Path,
    output: Path,
    *,
    role: str,
    identity: str | None = None,
    pose: str | None = None,
) -> dict[str, Any]:
    relative = output.relative_to(asset_root).as_posix()
    expected_triangles = recipe.BINOCULAR_TRIANGLES if role == "physical_binoculars" else recipe.CHARACTER_TRIANGLES
    summary = recipe.validate_static_runtime_glb(output, expected_triangles=expected_triangles)
    record = recipe.file_record(output, logical_path=relative)
    record.update(summary)
    record["role"] = role
    if identity is not None:
        record["identity"] = identity
    if pose is not None:
        record["pose"] = pose
    return record


def bake_rigged_master(
    extraction_root: Path,
    asset_root: Path,
    temporary_directory: Path,
    *,
    normalization_scale: float,
) -> tuple[dict[str, Any], dict[str, Any]]:
    import_source(extraction_root / SOURCE_FBX_RELATIVE)
    validate_source_scene()
    armature = bpy.data.objects[ARMATURE_NAME]
    meshes = [bpy.data.objects[name] for name in CHARACTER_NAMES]
    for name, obj in zip(CHARACTER_NAMES, meshes):
        tile = CHARACTER_TILES["body"] if name == BODY_NAME else CHARACTER_TILES["arm_head"]
        remap_uv(obj, tile, CHARACTER_ATLAS_SIZE)
    material, atlas_record = build_atlas_material(
        extraction_root,
        temporary_directory,
        identity="face01",
    )
    assign_sole_material(meshes, material)
    minimum, maximum = exact_bounds(meshes)
    transform = Matrix.Translation(
        Vector((-(minimum.x + maximum.x) * 0.5 * normalization_scale, 0.0, -minimum.z * normalization_scale))
    ) @ Matrix.Scale(normalization_scale, 4)
    armature.matrix_world = transform @ armature.matrix_world
    bpy.context.view_layer.update()
    output = asset_root / "authoring/german_panzertruppe_rigged.glb"
    export_glb(output, [armature, *meshes], skins=True)
    summary = recipe.validate_rigged_master_glb(output)
    if summary["joint_count"] != len(recipe.ARMATURE_BONES):
        raise RuntimeError(
            f"rigged master exported {summary['joint_count']} joints, expected {len(recipe.ARMATURE_BONES)}"
        )
    if abs(summary["bounds_min_m"][1]) > 1.0e-5:
        raise RuntimeError("rigged master did not export on glTF Y=0")
    if abs(summary["bounds_max_m"][1] - recipe.TARGET_HEIGHT_M) > 1.0e-4:
        raise RuntimeError("rigged master standing height changed during export")
    record = recipe.file_record(output, logical_path=output.relative_to(asset_root).as_posix())
    record.update(summary)
    record.update(
        {
            "role": "source-native authoring rig",
            "retargeted": False,
            "animation_count": 0,
            "runtime_authority": False,
        }
    )
    return record, atlas_record


def bake_character_pose(
    extraction_root: Path,
    asset_root: Path,
    temporary_directory: Path,
    *,
    pose: str,
    normalization_scale: float,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    import_source(extraction_root / SOURCE_FBX_RELATIVE)
    validate_source_scene()
    armature = bpy.data.objects[ARMATURE_NAME]
    reset_and_apply_pose(armature, pose)
    static_by_source = bake_character_skin_to_static()
    static_objects = list(static_by_source.values())
    transform_static_character(static_objects, scale=normalization_scale)
    for source_name, obj in static_by_source.items():
        tile = CHARACTER_TILES["body"] if source_name == BODY_NAME else CHARACTER_TILES["arm_head"]
        remap_uv(obj, tile, CHARACTER_ATLAS_SIZE)

    records = []
    atlas_contract = None
    for identity in recipe.IDENTITIES:
        material, current_atlas = build_atlas_material(
            extraction_root,
            temporary_directory,
            identity=identity,
        )
        assign_sole_material(static_objects, material)
        output = asset_root / f"baked/german_panzertruppe_{identity}_{pose}_static.glb"
        export_glb(output, static_objects, skins=False)
        record = augmented_output_record(
            asset_root,
            output,
            role="static_character_pose",
            identity=identity,
            pose=pose,
        )
        if abs(record["bounds_min_m"][1]) > 1.0e-5:
            raise RuntimeError(f"{identity}/{pose} did not export on glTF Y=0")
        if pose == "standing" and abs(
            record["bounds_max_m"][1] - record["bounds_min_m"][1] - recipe.TARGET_HEIGHT_M
        ) > 1.0e-4:
            raise RuntimeError(f"{identity} standing height changed during export")
        records.append(record)
        atlas_contract = current_atlas
        for obj in static_objects:
            obj.data.materials.clear()
        bpy.data.materials.remove(material)
    if atlas_contract is None:
        raise RuntimeError("no identity atlases were produced")
    return records, {
        "pose": pose,
        "bone_rotations_degrees_xyz": POSE_ROTATIONS_DEGREES[pose],
        "geometry": "evaluated native armature deformation baked to static vertices",
        "fixed_rest_scale_reused": True,
        "runtime_skinning_required": False,
    }


def bake_physical_binoculars(
    extraction_root: Path,
    asset_root: Path,
    temporary_directory: Path,
) -> tuple[dict[str, Any], dict[str, Any]]:
    import_source(extraction_root / SOURCE_FBX_RELATIVE)
    validate_source_scene()
    source = bpy.data.objects[BINOCULAR_NAME]
    mesh = source.data.copy()
    mesh.transform(source.matrix_world)
    mesh.update()
    obj = bpy.data.objects.new("German_Field_Binoculars_Physical", mesh)
    bpy.context.scene.collection.objects.link(obj)
    for candidate in list(bpy.context.scene.objects):
        if candidate is not obj:
            bpy.data.objects.remove(candidate, do_unlink=True)
    minimum, maximum = exact_bounds([obj])
    obj.data.transform(
        Matrix.Translation(
            Vector((-(minimum.x + maximum.x) * 0.5, -(minimum.y + maximum.y) * 0.5, -minimum.z))
        )
    )
    obj.data.update()
    remap_uv(obj, PROP_TILE, PROP_ATLAS_SIZE)
    material, atlas_record = build_atlas_material(
        extraction_root,
        temporary_directory,
        identity=None,
        prop_only=True,
    )
    assign_sole_material([obj], material)
    output = asset_root / "baked/german_field_binoculars_physical.glb"
    export_glb(output, [obj], skins=False)
    record = augmented_output_record(asset_root, output, role="physical_binoculars")
    dimensions = [
        record["bounds_max_m"][axis] - record["bounds_min_m"][axis]
        for axis in range(3)
    ]
    expected_dimensions = [0.187466, 0.136485, 0.0785]
    if any(abs(actual - expected) > 2.0e-4 for actual, expected in zip(dimensions, expected_dimensions)):
        raise RuntimeError(f"physical binocular dimensions changed: {dimensions}")
    if abs(record["bounds_min_m"][1]) > 1.0e-5:
        raise RuntimeError("physical binocular prop did not export on glTF Y=0")
    record["physical_dimensions_m"] = round_vector(dimensions)
    record["source_display_transform_baked"] = True
    record["runtime_skinning_required"] = False
    return record, atlas_record


def build_manifest(
    runtime_records: Sequence[dict[str, Any]],
    *,
    sources: dict[str, Any],
) -> dict[str, Any]:
    by_identity_pose = {
        (record.get("identity"), record.get("pose")): record["path"]
        for record in runtime_records
        if record.get("role") == "static_character_pose"
    }
    identities = {}
    extracted_by_path = {record["path"]: record for record in sources["extracted_files"]}
    for identity in recipe.IDENTITIES:
        texture_index = int(identity[-2:])
        texture_path = f"textures/reviewed/face_{texture_index:02d}_basecolor.png"
        identities[identity] = {
            "reviewed_face_basecolor": extracted_by_path[texture_path],
            "geometry": HEAD_NAME,
            "poses": {
                pose: by_identity_pose[(identity, pose)]
                for pose in recipe.POSES
            },
        }
    binoculars = next(record for record in runtime_records if record.get("role") == "physical_binoculars")
    return {
        "schema": "kessel.tiger-crew-runtime-manifest",
        "version": 1,
        "runtime_mode": "static GLB pose selection; no runtime skinning required",
        "coordinate_system": {
            "up": "glTF +Y",
            "forward": "glTF +Z",
            "character_floor": "Y=0",
            "units": "metres",
        },
        "target_standing_height_m": recipe.TARGET_HEIGHT_M,
        "identities": identities,
        "props": {"binoculars": binoculars["path"]},
        "first_person_binocular_frames": {},
        "deferred": [
            {
                "output": "first-person binocular arm frames",
                "reason": (
                    "source-native raise pose was explored, but hand-to-prop registration was not "
                    "quality-gated in time for this required deterministic bake"
                ),
            }
        ],
    }


def main() -> None:
    arguments = parse_arguments()
    receipt, sources = verify_extraction(arguments.extraction_root)
    arguments.asset_root.mkdir(parents=True, exist_ok=True)
    if any(arguments.asset_root.rglob("*.zip")):
        raise RuntimeError("raw ZIP archives are forbidden in the committed Tiger crew asset root")
    (arguments.asset_root / "authoring").mkdir(parents=True, exist_ok=True)
    (arguments.asset_root / "baked").mkdir(parents=True, exist_ok=True)
    report_path = arguments.asset_root / "bake.report.json"
    if report_path.exists():
        report_path.unlink()

    import_source(arguments.extraction_root / SOURCE_FBX_RELATIVE)
    source_contract = validate_source_scene()
    normalization = character_rest_normalization()
    scale = float(normalization["uniform_scale"])

    runtime_records: list[dict[str, Any]] = []
    pose_contracts = []
    with tempfile.TemporaryDirectory(prefix=".tiger-crew-atlas-", dir=arguments.asset_root) as temporary:
        temporary_directory = Path(temporary)
        rigged_record, rigged_atlas = bake_rigged_master(
            arguments.extraction_root,
            arguments.asset_root,
            temporary_directory,
            normalization_scale=scale,
        )
        for pose in recipe.POSES:
            records, pose_contract = bake_character_pose(
                arguments.extraction_root,
                arguments.asset_root,
                temporary_directory,
                pose=pose,
                normalization_scale=scale,
            )
            runtime_records.extend(records)
            pose_contracts.append(pose_contract)
        binocular_record, binocular_atlas = bake_physical_binoculars(
            arguments.extraction_root,
            arguments.asset_root,
            temporary_directory,
        )
        runtime_records.append(binocular_record)

    manifest = build_manifest(runtime_records, sources=sources)
    manifest_path = arguments.asset_root / "baked/pose_manifest.v1.json"
    manifest_path.write_text(recipe.stable_json(manifest), encoding="utf-8")
    script_path = Path(__file__).resolve()
    recipe_path = Path(recipe.__file__).resolve()
    report = {
        "schema": "kessel.tiger-crew-bake",
        "version": 1,
        "asset": receipt["asset"],
        "sources": sources,
        "baker": {
            "name": "Blender",
            "version": bpy.app.version_string,
            "script": recipe.file_record(script_path, logical_path="tools/bake_tiger_crew_kit.py"),
            "offline_contract": recipe.file_record(
                recipe_path,
                logical_path="tools/tiger_crew_recipe.py",
            ),
        },
        "source_contract": source_contract,
        "normalization": normalization,
        "materials": {
            "runtime_policy": (
                "exactly one material and one embedded sRGB base-color atlas per runtime GLB; "
                "all primitives reference material index zero"
            ),
            "character_atlas_example": rigged_atlas,
            "binocular_atlas": binocular_atlas,
            "metallic_factor": 0.0,
            "roughness_factor": 0.88,
            "normal_map": "deferred; runtime deliverable requested a unified base-color atlas",
            "height_map": "not a glTF runtime channel",
        },
        "poses": pose_contracts,
        "rigged_master": rigged_record,
        "runtime_manifest": recipe.file_record(
            manifest_path,
            logical_path=manifest_path.relative_to(arguments.asset_root).as_posix(),
        ),
        "runtime_outputs": sorted(runtime_records, key=lambda item: item["path"]),
        "deferred": manifest["deferred"],
    }
    verification = recipe.verify_bake_receipt(
        arguments.asset_root,
        report,
        extraction_root=arguments.extraction_root,
    )
    report_path.write_text(recipe.stable_json(report), encoding="utf-8")
    print(recipe.stable_json(verification), end="")


if __name__ == "__main__":
    main()
