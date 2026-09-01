#!/usr/bin/env python3
"""Bake the reviewed Vinrax PSX skeleton into Kessel authoring/runtime GLBs.

Run with Blender from the repository root::

    blender --background --factory-startup --python-exit-code 1 \
      --python tools/bake_skeleton_character.py -- \
      /path/to/skeleton-character-psx.zip \
      assets/flavor/Characters/skeleton-character-psx-vinrax-cc-by-4

The raw ZIP and FBX remain outside the repository.  The exact outer and nested archive hashes are
gated before import.  No bones are invented: the 27-bone native skin is retained in the authoring
master, while the current non-skeletal runtime receives one evaluated relaxed static mesh.
"""

from __future__ import annotations

import argparse
from collections import Counter
import json
from math import sqrt
from pathlib import Path
import sys
from typing import Any, Iterable, Sequence

import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import skeleton_character_recipe as recipe  # noqa: E402


ARMATURE_NAME = "rig"
MESH_NAME = "skeleton"

# Arm directions are armature-local Blender vectors.  The source faces Blender -Y and exports as
# glTF +Z.  A small -Y component relaxes the hands forward instead of pinning them to the thighs.
RELAXED_DIRECTION_TARGETS = {
    "upper_arm.L": (0.08, -0.02, -1.0),
    "forearm.L": (0.03, -0.14, -1.0),
    "hand.L": (0.015, -0.08, -1.0),
    "upper_arm.R": (-0.08, -0.02, -1.0),
    "forearm.R": (-0.03, -0.14, -1.0),
    "hand.R": (-0.015, -0.08, -1.0),
}


def parse_arguments() -> argparse.Namespace:
    try:
        separator = sys.argv.index("--")
    except ValueError as error:
        raise SystemExit("expected: -- <skeleton-character-psx.zip> <asset-root> [--preview PATH]") from error
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("asset_root", type=Path)
    parser.add_argument(
        "--preview",
        type=Path,
        default=Path("/private/tmp/skeleton-character-psx-relaxed-preview.png"),
    )
    arguments = parser.parse_args(sys.argv[separator + 1 :])
    arguments.archive = arguments.archive.expanduser().resolve()
    arguments.asset_root = arguments.asset_root.expanduser().resolve()
    arguments.preview = arguments.preview.expanduser().resolve()
    return arguments


def supported_keywords(operator, values: dict[str, Any]) -> dict[str, Any]:
    supported = {item.identifier for item in operator.get_rna_type().properties}
    return {key: value for key, value in values.items() if key in supported}


def clear_scene() -> None:
    if bpy.context.object is not None and bpy.context.object.mode != "OBJECT":
        bpy.ops.object.mode_set(mode="OBJECT")
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for collection in (
        bpy.data.meshes,
        bpy.data.armatures,
        bpy.data.materials,
        bpy.data.images,
        bpy.data.actions,
        bpy.data.cameras,
        bpy.data.lights,
    ):
        for datablock in list(collection):
            if datablock.users == 0:
                collection.remove(datablock)


def import_source(fbx_path: Path) -> None:
    clear_scene()
    result = bpy.ops.import_scene.fbx(
        **supported_keywords(
            bpy.ops.import_scene.fbx,
            {
                "filepath": str(fbx_path),
                "use_anim": True,
                "use_custom_normals": True,
                "use_image_search": False,
                "automatic_bone_orientation": False,
            },
        )
    )
    if "FINISHED" not in result:
        raise RuntimeError(f"reviewed skeleton FBX import failed: {result}")
    bpy.context.view_layer.update()


def mesh_topology(obj: bpy.types.Object) -> dict[str, int]:
    obj.data.calc_loop_triangles()
    return {
        "vertices": len(obj.data.vertices),
        "edges": len(obj.data.edges),
        "polygons": len(obj.data.polygons),
        "triangles": len(obj.data.loop_triangles),
        "loops": len(obj.data.loops),
    }


def exact_bounds(objects: Sequence[bpy.types.Object]) -> tuple[Vector, Vector]:
    points = [obj.matrix_world @ vertex.co for obj in objects for vertex in obj.data.vertices]
    if not points:
        raise RuntimeError("skeleton mesh selection has no vertices")
    return (
        Vector(tuple(min(point[axis] for point in points) for axis in range(3))),
        Vector(tuple(max(point[axis] for point in points) for axis in range(3))),
    )


def round_vector(values: Iterable[float], digits: int = 6) -> list[float]:
    return [round(float(value), digits) for value in values]


def validate_source_scene(texture_path: Path) -> dict[str, Any]:
    armature = bpy.data.objects.get(ARMATURE_NAME)
    mesh = bpy.data.objects.get(MESH_NAME)
    if armature is None or armature.type != "ARMATURE":
        raise RuntimeError("reviewed skeleton native armature 'rig' is missing")
    if mesh is None or mesh.type != "MESH":
        raise RuntimeError("reviewed skeleton weighted mesh 'skeleton' is missing")
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    armatures = [obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE"]
    if meshes != [mesh] or armatures != [armature]:
        raise RuntimeError("reviewed skeleton source object set changed")
    bones = tuple(bone.name for bone in armature.data.bones)
    if bones != recipe.ARMATURE_BONES:
        raise RuntimeError(f"reviewed skeleton 27-bone order/name contract changed: {bones}")
    topology = mesh_topology(mesh)
    expected = {
        "vertices": recipe.SOURCE_VERTICES,
        "edges": recipe.SOURCE_EDGES,
        "polygons": recipe.SOURCE_POLYGONS,
        "triangles": recipe.SOURCE_TRIANGLES,
        "loops": recipe.SOURCE_LOOPS,
    }
    if topology != expected:
        raise RuntimeError(f"reviewed skeleton topology changed: expected {expected}, got {topology}")
    modifiers = [modifier for modifier in mesh.modifiers if modifier.type == "ARMATURE"]
    if len(modifiers) != 1 or modifiers[0].object is not armature:
        raise RuntimeError("reviewed skeleton mesh is not bound to its native armature")
    if any(not vertex.groups for vertex in mesh.data.vertices):
        raise RuntimeError("reviewed skeleton mesh contains an unweighted vertex")
    if not mesh.data.uv_layers or len(mesh.data.uv_layers.active.data) != recipe.SOURCE_LOOPS:
        raise RuntimeError("reviewed skeleton mesh has no complete UV layer")
    if bpy.data.actions:
        raise RuntimeError("reviewed skeleton source unexpectedly contains animation actions")
    image = bpy.data.images.load(str(texture_path), check_existing=False)
    texture_dimensions = tuple(int(value) for value in image.size)
    bpy.data.images.remove(image)
    if texture_dimensions != recipe.TEXTURE_SIZE_PX:
        raise RuntimeError(f"reviewed skeleton texture dimensions changed: {texture_dimensions}")

    head = armature.data.bones["head"].head_local
    jaw_end = armature.data.bones["jaw_end"].tail_local
    if jaw_end.y >= head.y - 0.10:
        raise RuntimeError("reviewed skeleton facial-forward landmark no longer points Blender -Y")
    minimum, maximum = exact_bounds([mesh])
    source_height = maximum.z - minimum.z
    if not 1.700 < source_height < 1.702:
        raise RuntimeError(f"reviewed skeleton standing height changed: {source_height}")
    return {
        "armature": ARMATURE_NAME,
        "mesh": MESH_NAME,
        "bone_count": len(bones),
        "bones": list(bones),
        "topology": topology,
        "weighted_vertex_count": recipe.SOURCE_VERTICES,
        "unweighted_vertex_count": 0,
        "animation_count": 0,
        "texture_dimensions_px": list(texture_dimensions),
        "source_bounds_min_m": round_vector(minimum),
        "source_bounds_max_m": round_vector(maximum),
        "source_standing_height_m": round(float(source_height), 9),
        "orientation_evidence": {
            "blender_face_forward": "-Y",
            "gltf_face_forward": "+Z",
            "head_y_m": round(float(head.y), 9),
            "jaw_end_y_m": round(float(jaw_end.y), 9),
        },
    }


def build_material(texture_path: Path) -> bpy.types.Material:
    source = bpy.data.images.load(str(texture_path), check_existing=False)
    if tuple(int(value) for value in source.size) != recipe.TEXTURE_SIZE_PX:
        raise RuntimeError("reviewed skeleton texture dimensions changed during material build")
    pixels = list(source.pixels[:])
    bpy.data.images.remove(source)
    image = bpy.data.images.new(
        "SkeletonCharacterPSX_BaseColor",
        width=recipe.TEXTURE_SIZE_PX[0],
        height=recipe.TEXTURE_SIZE_PX[1],
        alpha=True,
    )
    image.colorspace_settings.name = "sRGB"
    image.pixels.foreach_set(pixels)
    image.update()
    image.pack()

    material = bpy.data.materials.new("SkeletonCharacterPSX_Material")
    material.use_nodes = True
    material.diffuse_color = (1.0, 1.0, 1.0, 1.0)
    material.metallic = 0.0
    material.roughness = 0.72
    nodes = material.node_tree.nodes
    nodes.clear()
    output = nodes.new("ShaderNodeOutputMaterial")
    shader = nodes.new("ShaderNodeBsdfPrincipled")
    shader.inputs["Metallic"].default_value = 0.0
    shader.inputs["Roughness"].default_value = 0.72
    texture = nodes.new("ShaderNodeTexImage")
    texture.image = image
    texture.interpolation = "Closest"
    material.node_tree.links.new(texture.outputs["Color"], shader.inputs["Base Color"])
    material.node_tree.links.new(shader.outputs["BSDF"], output.inputs["Surface"])
    return material


def assign_only_material(mesh: bpy.types.Object, material: bpy.types.Material) -> None:
    mesh.data.materials.clear()
    mesh.data.materials.append(material)
    for polygon in mesh.data.polygons:
        polygon.material_index = 0
    mesh.data.update()


def select_only(objects: Sequence[bpy.types.Object]) -> None:
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.hide_set(False)
        obj.hide_render = False
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]


def export_glb(path: Path, objects: Sequence[bpy.types.Object], *, skins: bool) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    select_only(objects)
    result = bpy.ops.export_scene.gltf(
        **supported_keywords(
            bpy.ops.export_scene.gltf,
            {
                "filepath": str(path),
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
                "export_cameras": False,
                "export_lights": False,
                "export_yup": True,
                "export_extras": False,
            },
        )
    )
    if "FINISHED" not in result or not path.is_file():
        raise RuntimeError(f"skeleton GLB export failed for {path}: {result}")


def normalization(mesh: bpy.types.Object) -> tuple[float, Vector, dict[str, Any]]:
    minimum, maximum = exact_bounds([mesh])
    height = maximum.z - minimum.z
    scale = recipe.TARGET_HEIGHT_M / height
    translation = Vector(
        (
            -(minimum.x + maximum.x) * 0.5 * scale,
            -(minimum.y + maximum.y) * 0.5 * scale,
            -minimum.z * scale,
        )
    )
    return scale, translation, {
        "source_bounds_min_m": round_vector(minimum),
        "source_bounds_max_m": round_vector(maximum),
        "source_height_m": round(float(height), 9),
        "uniform_scale": round(float(scale), 12),
        "translation_blender_xyz_m": round_vector(translation, 9),
        "target_height_m": recipe.TARGET_HEIGHT_M,
        "gltf_origin": "horizontal bounds center, floor Y=0",
        "gltf_axes": {"up": "+Y", "face_forward": "+Z"},
    }


def output_record(asset_root: Path, path: Path, *, rigged: bool, role: str) -> dict[str, Any]:
    summary = recipe.validate_glb(path, rigged=rigged)
    record = recipe.file_record(path, logical_path=path.relative_to(asset_root).as_posix())
    record.update(summary)
    record["role"] = role
    return record


def bake_rigged_master(
    fbx_path: Path,
    texture_path: Path,
    asset_root: Path,
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    import_source(fbx_path)
    source_scene = validate_source_scene(texture_path)
    armature = bpy.data.objects[ARMATURE_NAME]
    mesh = bpy.data.objects[MESH_NAME]
    material = build_material(texture_path)
    assign_only_material(mesh, material)
    scale, translation, normalization_record = normalization(mesh)
    armature.matrix_world = Matrix.Translation(translation) @ Matrix.Scale(scale, 4) @ armature.matrix_world
    bpy.context.view_layer.update()
    output = asset_root / recipe.RIGGED_RELATIVE
    export_glb(output, [armature, mesh], skins=True)
    record = output_record(
        asset_root,
        output,
        rigged=True,
        role="source-native rigged authoring master; not loaded by current runtime",
    )
    record.update(
        {
            "retargeted": False,
            "invented_bones": 0,
            "animation_count": 0,
            "runtime_authority": False,
        }
    )
    return record, source_scene, normalization_record


def align_pose_bone(armature: bpy.types.Object, bone_name: str, direction: Sequence[float]) -> None:
    pose_bone = armature.pose.bones[bone_name]
    current = (pose_bone.tail - pose_bone.head).normalized()
    target = Vector(direction).normalized()
    delta = current.rotation_difference(target)
    pivot = pose_bone.head.copy()
    pose_bone.matrix = (
        Matrix.Translation(pivot)
        @ delta.to_matrix().to_4x4()
        @ Matrix.Translation(-pivot)
        @ pose_bone.matrix
    )
    bpy.context.view_layer.update()
    observed = (pose_bone.tail - pose_bone.head).normalized()
    if observed.dot(target) < 0.9999:
        raise RuntimeError(f"could not align relaxed bone {bone_name}: {observed} vs {target}")


def apply_relaxed_pose(armature: bpy.types.Object) -> None:
    for bone in armature.pose.bones:
        bone.location = (0.0, 0.0, 0.0)
        bone.rotation_mode = "QUATERNION"
        bone.rotation_quaternion = (1.0, 0.0, 0.0, 0.0)
        bone.scale = (1.0, 1.0, 1.0)
    bpy.context.view_layer.update()
    for bone_name, direction in RELAXED_DIRECTION_TARGETS.items():
        align_pose_bone(armature, bone_name, direction)


def bake_evaluated_static(mesh: bpy.types.Object) -> bpy.types.Object:
    depsgraph = bpy.context.evaluated_depsgraph_get()
    evaluated = mesh.evaluated_get(depsgraph)
    baked_mesh = bpy.data.meshes.new_from_object(
        evaluated,
        preserve_all_data_layers=True,
        depsgraph=depsgraph,
    )
    baked_mesh.transform(evaluated.matrix_world)
    baked_mesh.update()
    result = bpy.data.objects.new("SkeletonCharacterPSX_Relaxed_Static", baked_mesh)
    bpy.context.scene.collection.objects.link(result)
    for obj in list(bpy.context.scene.objects):
        if obj is not result:
            bpy.data.objects.remove(obj, do_unlink=True)
    bpy.context.view_layer.update()
    if mesh_topology(result) != {
        "vertices": recipe.SOURCE_VERTICES,
        "edges": recipe.SOURCE_EDGES,
        "polygons": recipe.SOURCE_POLYGONS,
        "triangles": recipe.SOURCE_TRIANGLES,
        "loops": recipe.SOURCE_LOOPS,
    }:
        raise RuntimeError("evaluated relaxed skeleton topology changed")
    return result


def gltf_point(blender_point: Vector) -> Vector:
    return Vector((blender_point.x, blender_point.z, -blender_point.y))


def landmark_points(armature: bpy.types.Object) -> dict[str, Vector]:
    bones = armature.pose.bones

    def midpoint(start_bone: str, start_endpoint: str, end_bone: str, end_endpoint: str) -> Vector:
        start = getattr(bones[start_bone], start_endpoint).copy()
        end = getattr(bones[end_bone], end_endpoint).copy()
        return (start + end) * 0.5

    # The source faces -Y, so its named .R chain occupies negative X and maps to the engine's
    # negative-X Left* partition.  This is an explicit coordinate crosswalk, not bone renaming.
    return {
        "Pelvis": midpoint("hips", "head", "hips", "tail"),
        "Torso": midpoint("spine", "head", "chest", "tail"),
        "Head": midpoint("head", "head", "head", "tail"),
        "LeftUpperArm": midpoint("upper_arm.R", "head", "upper_arm.R", "tail"),
        "LeftForearm": midpoint("forearm.R", "head", "hand.R_end", "tail"),
        "RightUpperArm": midpoint("upper_arm.L", "head", "upper_arm.L", "tail"),
        "RightForearm": midpoint("forearm.L", "head", "hand.L_end", "tail"),
        "LeftThigh": midpoint("thigh.R", "head", "thigh.R", "tail"),
        "LeftShin": midpoint("shin.R", "head", "toe.R_end", "tail"),
        "RightThigh": midpoint("thigh.L", "head", "thigh.L", "tail"),
        "RightShin": midpoint("shin.L", "head", "toe.L_end", "tail"),
    }


def normalized_landmark_alignment(
    raw_landmarks: dict[str, Vector],
    scale: float,
    translation: Vector,
) -> dict[str, Any]:
    records = []
    maximum = 0.0
    squared = 0.0
    for part in recipe.BODY_PARTS:
        observed = gltf_point(raw_landmarks[part] * scale + translation)
        target = Vector(recipe.BODY_PART_TARGET_CENTERS_M[part])
        error = (observed - target).length
        maximum = max(maximum, error)
        squared += error * error
        records.append(
            {
                "body_part": part,
                "landmark_gltf_xyz_m": round_vector(observed),
                "runtime_target_center_gltf_xyz_m": round_vector(target),
                "euclidean_error_m": round(float(error), 6),
            }
        )
    return {
        "method": (
            "native-bone chain midpoints compared with the 1.53 m CrewBodyPart rest-center snapshot"
        ),
        "landmarks": records,
        "maximum_error_m": round(float(maximum), 6),
        "rms_error_m": round(sqrt(squared / len(records)), 6),
        "exact_registration": False,
    }


def triangle_partition(mesh: bpy.types.Object) -> dict[str, Any]:
    mesh.data.calc_loop_triangles()
    counts: Counter[str] = Counter()
    bounds: dict[str, list[Vector]] = {part: [] for part in recipe.BODY_PARTS}
    for triangle in mesh.data.loop_triangles:
        points = [gltf_point(mesh.data.vertices[index].co) for index in triangle.vertices]
        centroid = sum(points, Vector()) / 3.0
        part = recipe.classify_relaxed_point(centroid)
        counts[part] += 1
        bounds[part].extend(points)
    if sum(counts.values()) != recipe.SOURCE_TRIANGLES:
        raise RuntimeError("relaxed skeleton triangle partition lost source triangles")
    if any(counts[part] <= 0 for part in recipe.BODY_PARTS):
        raise RuntimeError(f"relaxed skeleton has an empty runtime body-part page: {counts}")
    part_bounds = {}
    for part in recipe.BODY_PARTS:
        points = bounds[part]
        part_bounds[part] = {
            "min_gltf_xyz_m": round_vector(
                min(point[axis] for point in points) for axis in range(3)
            ),
            "max_gltf_xyz_m": round_vector(
                max(point[axis] for point in points) for axis in range(3)
            ),
        }
    return {
        "classifier": "src/crew_physics.rs::classify_relaxed_mesh_point snapshot",
        "assignment": "whole output triangle assigned by its glTF-space centroid",
        "counts": {part: counts[part] for part in recipe.BODY_PARTS},
        "bounds": part_bounds,
        "total_triangles": recipe.SOURCE_TRIANGLES,
        "lossless_triangle_assignment": True,
    }


def render_preview(mesh: bpy.types.Object, output: Path) -> None:
    # The preview is deliberately untracked QA output.  It shows the actual evaluated runtime mesh,
    # not a separate high-detail render source.
    bpy.ops.mesh.primitive_plane_add(size=6.0, location=(0.0, 0.0, -0.004))
    plane = bpy.context.object
    plane.name = "QA_Ground"
    ground = bpy.data.materials.new("QA_Ground_Material")
    ground.diffuse_color = (0.055, 0.065, 0.078, 1.0)
    ground.roughness = 0.92
    plane.data.materials.append(ground)

    bpy.ops.object.camera_add(location=(2.35, -4.25, 1.58))
    camera = bpy.context.object
    camera.data.type = "ORTHO"
    camera.data.ortho_scale = 1.88
    target = Vector((0.0, 0.0, 0.76))
    camera.rotation_euler = (target - camera.location).to_track_quat("-Z", "Y").to_euler()
    bpy.context.scene.camera = camera

    for name, location, energy, size in (
        ("QA_Key", (-2.0, -3.0, 3.3), 900.0, 2.3),
        ("QA_Fill", (2.4, -1.2, 2.0), 420.0, 2.0),
        ("QA_Rim", (0.4, 2.0, 2.8), 720.0, 1.6),
    ):
        bpy.ops.object.light_add(type="AREA", location=location)
        light = bpy.context.object
        light.name = name
        light.data.energy = energy
        light.data.shape = "DISK"
        light.data.size = size
        light.rotation_euler = (target - light.location).to_track_quat("-Z", "Y").to_euler()

    world = bpy.context.scene.world or bpy.data.worlds.new("QA_World")
    bpy.context.scene.world = world
    world.use_nodes = True
    background = world.node_tree.nodes.get("Background")
    background.inputs["Color"].default_value = (0.018, 0.025, 0.04, 1.0)
    background.inputs["Strength"].default_value = 0.36

    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 640
    scene.render.resolution_y = 640
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.film_transparent = False
    scene.render.filepath = str(output)
    scene.view_settings.look = "AgX - Medium High Contrast"
    output.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.render.render(write_still=True)
    if not output.is_file():
        raise RuntimeError(f"skeleton QA preview was not written: {output}")


def bake_relaxed_static(
    fbx_path: Path,
    texture_path: Path,
    asset_root: Path,
    preview_path: Path,
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    import_source(fbx_path)
    validate_source_scene(texture_path)
    armature = bpy.data.objects[ARMATURE_NAME]
    mesh = bpy.data.objects[MESH_NAME]
    scale, _rest_translation, _rest_normalization = normalization(mesh)
    material = build_material(texture_path)
    assign_only_material(mesh, material)
    apply_relaxed_pose(armature)
    raw_landmarks = landmark_points(armature)
    static = bake_evaluated_static(mesh)
    # Bake_evaluated_static retains the copied material datablock and UVs.
    minimum, maximum = exact_bounds([static])
    if abs((maximum.z - minimum.z) * scale - recipe.TARGET_HEIGHT_M) > 2.0e-5:
        raise RuntimeError("relaxed arm pose unexpectedly changed standing height")
    translation = Vector(
        (
            -(minimum.x + maximum.x) * 0.5 * scale,
            -(minimum.y + maximum.y) * 0.5 * scale,
            -minimum.z * scale,
        )
    )
    static.data.transform(Matrix.Translation(translation) @ Matrix.Scale(scale, 4))
    static.data.update()
    normalized_minimum, normalized_maximum = exact_bounds([static])
    if abs(normalized_minimum.z) > 1.0e-6:
        raise RuntimeError("relaxed skeleton did not land on Blender Z=0/glTF Y=0")
    alignment = normalized_landmark_alignment(raw_landmarks, scale, translation)
    partition = triangle_partition(static)
    output = asset_root / recipe.STATIC_RELATIVE
    export_glb(output, [static], skins=False)
    record = output_record(
        asset_root,
        output,
        rigged=False,
        role="single-material relaxed static runtime anatomy mesh",
    )
    record.update(
        {
            "runtime_skinning_required": False,
            "source_topology_preserved": True,
            "decimation_applied": False,
        }
    )
    alignment_manifest = {
        "schema": "kessel.skeleton-anatomy-alignment",
        "version": 1,
        "asset": {
            "title": recipe.TITLE,
            "creator": recipe.CREATOR,
            "sketchfab_uid": recipe.SKETCHFAB_UID,
        },
        "coordinate_frame": {
            "units": "metres",
            "up": "+Y",
            "face_forward": "+Z",
            "floor_y_m": 0.0,
            "standing_height_m": recipe.TARGET_HEIGHT_M,
        },
        "pose": {
            "name": "relaxed-down-anatomical",
            "source_rest_pose": "T-pose",
            "native_skin_evaluated": True,
            "animation_clip": False,
            "direction_targets_armature_local_blender_xyz": RELAXED_DIRECTION_TARGETS,
            "legs": "unchanged native standing alignment",
        },
        "bone_crosswalk": {
            "Pelvis": ["hips"],
            "Torso": ["spine", "chest"],
            "Head": ["neck", "head", "jaw", "jaw_end"],
            "LeftUpperArm": ["shoulder.R", "upper_arm.R"],
            "LeftForearm": ["forearm.R", "hand.R", "hand.R_end"],
            "RightUpperArm": ["shoulder.L", "upper_arm.L"],
            "RightForearm": ["forearm.L", "hand.L", "hand.L_end"],
            "LeftThigh": ["thigh.R"],
            "LeftShin": ["shin.R", "foot.R", "toe.R", "toe.R_end"],
            "RightThigh": ["thigh.L"],
            "RightShin": ["shin.L", "foot.L", "toe.L", "toe.L_end"],
        },
        "side_crosswalk_note": (
            "the source faces Blender -Y; its .R chain occupies glTF negative X and therefore "
            "maps to the runtime's negative-X Left* partitions without renaming source bones"
        ),
        "landmark_alignment": alignment,
        "triangle_centroid_partition": partition,
        "registration_claim": "measured-approximate-not-exact",
        "limitations": [
            "The Vinrax skeleton was not authored inside the AdrielCZ clothed crew mesh.",
            "Landmark comparison quantifies only pose/scale compatibility; it is not vertex registration.",
            "Centroid partitioning is a deterministic runtime page assignment, not a medical segmentation.",
            "No bones, constraints, or animation clips were invented or inferred beyond the relaxed pose.",
        ],
    }
    alignment_path = asset_root / recipe.ALIGNMENT_RELATIVE
    alignment_path.parent.mkdir(parents=True, exist_ok=True)
    alignment_path.write_text(recipe.stable_json(alignment_manifest), encoding="utf-8")
    alignment_record = recipe.file_record(
        alignment_path,
        logical_path=recipe.ALIGNMENT_RELATIVE,
    )
    render_preview(static, preview_path)
    return record, alignment_record, {
        "normalized_bounds_min_blender_xyz_m": round_vector(normalized_minimum),
        "normalized_bounds_max_blender_xyz_m": round_vector(normalized_maximum),
        "preview": str(preview_path),
        "preview_tracked": False,
    }


def main() -> None:
    arguments = parse_arguments()
    arguments.asset_root.mkdir(parents=True, exist_ok=True)
    (arguments.asset_root / "authoring").mkdir(parents=True, exist_ok=True)
    (arguments.asset_root / "baked").mkdir(parents=True, exist_ok=True)
    for relative in (
        recipe.RIGGED_RELATIVE,
        recipe.STATIC_RELATIVE,
        recipe.ALIGNMENT_RELATIVE,
        recipe.REPORT_RELATIVE,
    ):
        path = arguments.asset_root.joinpath(*Path(relative).parts)
        if path.exists():
            path.unlink()

    with recipe.extract_reviewed_source(arguments.archive) as (fbx, texture, source_receipt):
        rigged, source_scene, normalization_record = bake_rigged_master(
            fbx, texture, arguments.asset_root
        )
        runtime_static, alignment_record, static_qa = bake_relaxed_static(
            fbx,
            texture,
            arguments.asset_root,
            arguments.preview,
        )

    report = {
        "schema": "kessel.skeleton-character-bake",
        "version": 1,
        "source": {
            "title": recipe.TITLE,
            "creator": recipe.CREATOR,
            "sketchfab_uid": recipe.SKETCHFAB_UID,
            "source_url": recipe.SOURCE_URL,
            "license": recipe.LICENSE_SPDX,
            "archive_sha256": recipe.ARCHIVE_SHA256,
            "private_source_receipt": source_receipt,
        },
        "toolchain": {
            "blender_version": bpy.app.version_string,
            "blender_version_tuple": list(bpy.app.version),
            "files": recipe.toolchain_receipt(),
        },
        "source_scene": source_scene,
        "normalization": normalization_record,
        "optimization": {
            "source_triangles": recipe.SOURCE_TRIANGLES,
            "runtime_triangles": recipe.SOURCE_TRIANGLES,
            "decimation": "none; the reviewed source is already a 1,596-triangle PSX mesh",
            "mesh_count": 1,
            "material_count": 1,
            "embedded_base_color_images": 1,
            "texture_dimensions_px": list(recipe.TEXTURE_SIZE_PX),
        },
        "rigged_master": rigged,
        "runtime_static": runtime_static,
        "alignment_manifest": alignment_record,
        "qa": static_qa,
        "claims": {
            "native_rig_preserved": True,
            "native_bone_count": len(recipe.ARMATURE_BONES),
            "invented_bone_count": 0,
            "source_animation_count": 0,
            "exact_registration_to_tiger_crew": False,
        },
    }
    report_path = arguments.asset_root / recipe.REPORT_RELATIVE
    report_path.write_text(recipe.stable_json(report), encoding="utf-8")
    verification = recipe.verify_bake_receipt(arguments.asset_root, report)
    print(recipe.stable_json(verification), end="")


if __name__ == "__main__":
    main()
