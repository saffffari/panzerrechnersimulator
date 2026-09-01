#!/usr/bin/env python3
"""Rig and pose-bake the selected CC BY Soviet infantry model for Kessel.

Run this script with Blender, not system Python::

    /Applications/Blender.app/Contents/MacOS/Blender \
      --background --factory-startup --python-exit-code 1 \
      --python tools/bake_soviet_infantry.py -- \
      assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/source/ww2_soviet_soldier_original.glb \
      assets/flavor/Characters/german-ww2-soldier-cc0/source/LowpolySoldier.fbx \
      assets/flavor/Characters/soviet-ww2-soldier-cc-by-4

The first argument must be the GLB downloaded by the operator from the exact attributed Sketchfab
page.  This script never performs or bypasses that authenticated download.  It archives those
input bytes unchanged, adapts the existing 49-bone German donor rig to the source rest stance,
adds deterministic fallbacks for uncovered vertices, retains a rigged authoring GLB, and exports
static pose frames for the engine's current non-skeletal GLB loader.

The static pose files, not the skeletal clips, are runtime authority.  This distinction is recorded
in ``animation_manifest.v1.json`` and ``bake.report.json`` rather than being hidden as an engine
animation claim.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from math import radians
from pathlib import Path
import sys
from typing import Any, Dict, Iterable, List, Optional, Sequence, Tuple

import bmesh
import bpy
from mathutils import Matrix, Quaternion, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import soviet_character_recipe as recipe  # noqa: E402


FALLBACK_BONES = (
    "Hips",
    "Spine",
    "Spine1",
    "Spine2",
    "Neck",
    "Head",
    "LeftShoulder",
    "LeftArm",
    "LeftForeArm",
    "LeftHand",
    "RightShoulder",
    "RightArm",
    "RightForeArm",
    "RightHand",
    "LeftUpLeg",
    "LeftLeg",
    "LeftFoot",
    "RightUpLeg",
    "RightLeg",
    "RightFoot",
)
MAX_VERTEX_INFLUENCES = 4
ACTION_FPS = recipe.ANIMATION_FPS


def parse_arguments() -> argparse.Namespace:
    try:
        separator = sys.argv.index("--")
    except ValueError as error:
        raise SystemExit(
            "expected: -- <soviet-source.glb> <german-donor.fbx> <output-directory>"
        ) from error
    parser = argparse.ArgumentParser(
        description="Rig and pose-bake the attributed Soviet infantry GLB",
    )
    parser.add_argument("source_glb", type=Path)
    parser.add_argument("donor_fbx", type=Path)
    parser.add_argument("output_directory", type=Path)
    parser.add_argument(
        "--source-forward",
        default="+Z",
        choices=("+Z", "-Z", "+X", "-X"),
        help=(
            "forward axis declared by the source glTF; default +Z maps to Blender -Y and Kessel +Z"
        ),
    )
    parser.add_argument(
        "--height-m",
        type=float,
        default=recipe.TARGET_HEIGHT_M,
        help="standing source height after reversible root normalization",
    )
    parser.add_argument(
        "--rigging-mode",
        default="auto",
        choices=("auto", "donor-transfer", "a-pose", "compact-standing"),
        help=(
            "auto chooses donor surface transfer for a T-pose, a refitted A-pose rig plus bone "
            "heat, or a conservative compact-standing fallback"
        ),
    )
    parser.add_argument(
        "--retrieved",
        default=datetime.now(timezone.utc).date().isoformat(),
        help="actual UTC retrieval date for attribution metadata (YYYY-MM-DD)",
    )
    arguments = parser.parse_args(sys.argv[separator + 1 :])
    arguments.source_glb = arguments.source_glb.expanduser().resolve()
    arguments.donor_fbx = arguments.donor_fbx.expanduser().resolve()
    arguments.output_directory = arguments.output_directory.expanduser().resolve()
    if not arguments.source_glb.is_file() or arguments.source_glb.suffix.casefold() != ".glb":
        raise SystemExit(f"Soviet source must be an existing GLB: {arguments.source_glb}")
    if not arguments.donor_fbx.is_file() or arguments.donor_fbx.suffix.casefold() != ".fbx":
        raise SystemExit(f"German rig donor must be an existing FBX: {arguments.donor_fbx}")
    if arguments.height_m <= 0.5 or arguments.height_m >= 2.5:
        raise SystemExit(f"implausible authored character height: {arguments.height_m}")
    try:
        datetime.strptime(arguments.retrieved, "%Y-%m-%d")
    except ValueError as error:
        raise SystemExit("--retrieved must be YYYY-MM-DD") from error
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


def imported_objects(importer, **keywords) -> List[bpy.types.Object]:
    before = set(bpy.context.scene.objects)
    # Blender importer properties move occasionally between LTS releases.  Required arguments
    # (notably filepath) are shared; optional compatibility switches are passed only when the
    # installed operator exposes them.
    result = importer(**supported_operator_keywords(importer, keywords))
    if "FINISHED" not in result:
        raise RuntimeError(f"asset import failed: {result}")
    return [obj for obj in bpy.context.scene.objects if obj not in before]


def world_vertex_points(
    objects: Sequence[bpy.types.Object], *, evaluated: bool = False
) -> Dict[str, List[Vector]]:
    depsgraph = bpy.context.evaluated_depsgraph_get() if evaluated else None
    result: Dict[str, List[Vector]] = {}
    for obj in sorted(objects, key=lambda candidate: candidate.name):
        if obj.type != "MESH":
            continue
        source = obj.evaluated_get(depsgraph) if depsgraph is not None else obj
        result[obj.name] = [source.matrix_world @ vertex.co for vertex in source.data.vertices]
    return result


def evaluated_deformation_snapshot(
    objects: Sequence[bpy.types.Object],
) -> Dict[str, Any]:
    """Capture evaluated world vertices and unchanged topology for local strain validation."""

    depsgraph = bpy.context.evaluated_depsgraph_get()
    records: Dict[str, Any] = {}
    coincident: Dict[Tuple[int, int, int], List[Tuple[str, int]]] = {}
    for obj in sorted(objects, key=lambda candidate: candidate.name):
        if obj.type != "MESH":
            continue
        evaluated = obj.evaluated_get(depsgraph)
        points = [evaluated.matrix_world @ vertex.co for vertex in evaluated.data.vertices]
        edges = [tuple(int(index) for index in edge.vertices) for edge in evaluated.data.edges]
        if not points or not edges:
            raise RuntimeError(f"source mesh {obj.name!r} has no deformation topology")
        records[obj.name] = {
            "points": points,
            "edges": edges,
        }
        for index, point in enumerate(points):
            # A 10-micrometre logical weld catches glTF UV/normal splits without treating separate
            # garment layers as one surface.  It is only used as a validation relationship; source
            # topology and UVs remain untouched.
            key = tuple(round(float(point[axis]) * 100000.0) for axis in range(3))
            coincident.setdefault(key, []).append((obj.name, index))
    weld_pairs = []
    for members in coincident.values():
        if len(members) < 2:
            continue
        anchor = members[0]
        weld_pairs.extend((anchor, member) for member in members[1:])
    return {"objects": records, "weld_pairs": weld_pairs}


def deformation_quality_between(
    rest: Dict[str, Any],
    posed: Dict[str, Any],
    *,
    height_m: float,
) -> Dict[str, Any]:
    if set(rest["objects"]) != set(posed["objects"]):
        raise RuntimeError("deformation validation object set changed during pose evaluation")
    rest_lengths: List[float] = []
    posed_lengths: List[float] = []
    for object_name, rest_record in rest["objects"].items():
        posed_record = posed["objects"][object_name]
        if rest_record["edges"] != posed_record["edges"]:
            raise RuntimeError(f"deformation topology changed on {object_name!r}")
        if len(rest_record["points"]) != len(posed_record["points"]):
            raise RuntimeError(f"deformation vertex count changed on {object_name!r}")
        for first, second in rest_record["edges"]:
            rest_lengths.append(
                (rest_record["points"][first] - rest_record["points"][second]).length
            )
            posed_lengths.append(
                (posed_record["points"][first] - posed_record["points"][second]).length
            )
    weld_separations = []
    for (first_object, first_index), (second_object, second_index) in rest["weld_pairs"]:
        first = posed["objects"][first_object]["points"][first_index]
        second = posed["objects"][second_object]["points"][second_index]
        weld_separations.append((first - second).length)
    return recipe.deformation_quality_report(
        rest_edge_lengths=rest_lengths,
        posed_edge_lengths=posed_lengths,
        weld_separations_m=weld_separations,
        height_m=height_m,
    )


def exact_world_bounds(objects: Sequence[bpy.types.Object]) -> Tuple[Vector, Vector]:
    points = [
        obj.matrix_world @ vertex.co
        for obj in objects
        if obj.type == "MESH"
        for vertex in obj.data.vertices
    ]
    if not points:
        raise RuntimeError("character selection has no mesh vertices")
    minimum = Vector(tuple(min(point[axis] for point in points) for axis in range(3)))
    maximum = Vector(tuple(max(point[axis] for point in points) for axis in range(3)))
    return minimum, maximum


def round_vector(vector: Iterable[float]) -> List[float]:
    return [round(float(value), 6) for value in vector]


def top_level_objects(objects: Sequence[bpy.types.Object]) -> List[bpy.types.Object]:
    selected = set(objects)
    return [obj for obj in objects if obj.parent not in selected]


def source_forward_rotation(source_forward: str) -> float:
    # Blender's glTF import maps glTF +Z to Blender -Y, which is already Kessel +Z after export.
    return {
        "+Z": 0.0,
        "-Z": math.pi,
        "+X": -math.pi * 0.5,
        "-X": math.pi * 0.5,
    }[source_forward]


def align_object_set(
    objects: Sequence[bpy.types.Object],
    *,
    name: str,
    height_m: float,
    rotation_z: float,
) -> bpy.types.Object:
    root = bpy.data.objects.new(name, None)
    root.empty_display_type = "PLAIN_AXES"
    bpy.context.scene.collection.objects.link(root)
    for obj in top_level_objects(objects):
        world = obj.matrix_world.copy()
        obj.parent = root
        obj.matrix_world = world
    root.rotation_euler[2] = rotation_z
    bpy.context.view_layer.update()
    meshes = [obj for obj in objects if obj.type == "MESH"]
    minimum, maximum = exact_world_bounds(meshes)
    source_height = maximum.z - minimum.z
    if source_height <= 1.0e-6:
        raise RuntimeError(f"{name} has no positive height")
    scale = height_m / source_height
    root.scale = (scale, scale, scale)
    bpy.context.view_layer.update()
    minimum, maximum = exact_world_bounds(meshes)
    root.location += Vector(
        (-(minimum.x + maximum.x) * 0.5, -(minimum.y + maximum.y) * 0.5, -minimum.z)
    )
    bpy.context.view_layer.update()
    return root


def detach_keep_world(obj: bpy.types.Object) -> None:
    world = obj.matrix_world.copy()
    obj.parent = None
    obj.matrix_world = world


def apply_object_world_transforms(objects: Sequence[bpy.types.Object]) -> None:
    """Bake a shared normalized FBX transform into mesh and armature data.

    Leaving the donor's centimetre scale/axis conversion on its armature object makes Blender's
    GLB round-trip reconstruct 24-centimetre arm bones as 25-metre tails. Applying the same world
    transform to the donor skin and skeleton keeps their bind relationship while giving the
    exported rig an identity object transform.
    """

    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = next(
        obj for obj in objects if obj.type == "ARMATURE"
    )
    result = bpy.ops.object.transform_apply(
        location=True,
        rotation=True,
        scale=True,
        properties=False,
    )
    if "FINISHED" not in result:
        raise RuntimeError(f"could not apply normalized donor transforms: {result}")
    bpy.context.view_layer.update()


def mesh_snapshot(meshes: Sequence[bpy.types.Object]) -> List[Dict[str, Any]]:
    return [
        {
            "object": obj.name,
            "mesh": obj.data.name,
            "vertices": len(obj.data.vertices),
            "polygons": len(obj.data.polygons),
            "triangles": sum(
                max(1, len(polygon.vertices) - 2) for polygon in obj.data.polygons
            ),
            "uv_layers": [layer.name for layer in obj.data.uv_layers],
            "materials": [
                slot.material.name if slot.material else None for slot in obj.material_slots
            ],
        }
        for obj in sorted(meshes, key=lambda candidate: candidate.name)
    ]


def mesh_authoring_fingerprint(meshes: Sequence[bpy.types.Object]) -> str:
    """Hash source positions, topology, material slots, and every UV loop before skinning.

    Skin weights and armature modifiers are intentionally excluded because they are the authored
    adaptation. Float ``hex`` strings retain the imported values exactly for the before/after
    in-memory gate without depending on JSON's decimal formatting precision.
    """

    records = []
    for obj in sorted(meshes, key=lambda candidate: candidate.name):
        records.append(
            {
                "object": obj.name,
                "vertices": [
                    [float(component).hex() for component in vertex.co]
                    for vertex in obj.data.vertices
                ],
                "edges": [list(map(int, edge.vertices)) for edge in obj.data.edges],
                "polygons": [
                    {
                        "vertices": list(map(int, polygon.vertices)),
                        "material_index": int(polygon.material_index),
                    }
                    for polygon in obj.data.polygons
                ],
                "uv_layers": {
                    layer.name: [
                        [float(component).hex() for component in loop.uv]
                        for loop in layer.data
                    ]
                    for layer in obj.data.uv_layers
                },
                "materials": [
                    slot.material.name if slot.material else None
                    for slot in obj.material_slots
                ],
            }
        )
    encoded = recipe.stable_json(records).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def image_snapshot(images: Iterable[bpy.types.Image]) -> List[Dict[str, Any]]:
    return [
        {
            "name": image.name,
            "width": int(image.size[0]),
            "height": int(image.size[1]),
            "packed": image.packed_file is not None,
            "source": image.source,
        }
        for image in sorted(images, key=lambda candidate: candidate.name)
        if image.source != "GENERATED"
    ]


def pack_source_images(images: Iterable[bpy.types.Image]) -> None:
    for image in images:
        if image.source == "FILE" and image.packed_file is None:
            resolved = Path(bpy.path.abspath(image.filepath))
            if resolved.is_file():
                image.pack()


def validate_source_import(
    objects: Sequence[bpy.types.Object], images: Sequence[bpy.types.Image]
) -> List[bpy.types.Object]:
    meshes = [obj for obj in objects if obj.type == "MESH"]
    armatures = [obj for obj in objects if obj.type == "ARMATURE"]
    if not meshes:
        raise RuntimeError("Soviet GLB did not import any mesh")
    if armatures:
        raise RuntimeError(
            "Soviet GLB unexpectedly contains an armature; refusing to discard or silently "
            "replace an authored source rig"
        )
    if not any(obj.data.uv_layers for obj in meshes):
        raise RuntimeError("Soviet source mesh has no UV set for its claimed atlas")
    if not any(obj.material_slots for obj in meshes):
        raise RuntimeError("Soviet source mesh has no assigned material")
    if not images:
        raise RuntimeError("Soviet source GLB imported no atlas image")
    for obj in meshes:
        obj["kessel_source_mesh"] = True
    return meshes


def apply_modifier(obj: bpy.types.Object, modifier_name: str) -> None:
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    result = bpy.ops.object.modifier_apply(modifier=modifier_name)
    if "FINISHED" not in result:
        raise RuntimeError(f"could not apply {modifier_name!r} on {obj.name!r}: {result}")


def bone_world_segment(armature: bpy.types.Object, bone_name: str) -> Tuple[Vector, Vector]:
    bone = armature.data.bones[bone_name]
    return armature.matrix_world @ bone.head_local, armature.matrix_world @ bone.tail_local


def point_segment_distance(point: Vector, start: Vector, end: Vector) -> float:
    delta = end - start
    denominator = delta.length_squared
    if denominator <= 1.0e-12:
        return (point - start).length
    factor = max(0.0, min(1.0, (point - start).dot(delta) / denominator))
    return (point - (start + delta * factor)).length


def posture_from_bounds(meshes: Sequence[bpy.types.Object]) -> Dict[str, Any]:
    minimum, maximum = exact_world_bounds(meshes)
    dimensions = maximum - minimum
    classification = recipe.classify_source_posture(dimensions.x, dimensions.z)
    return {
        **classification,
        "dimensions_m": round_vector(dimensions),
    }


def configure_deform_bones(
    armature: bpy.types.Object, *, retain_all_donor_weights: bool
) -> List[str]:
    deform = (
        {bone.name for bone in armature.data.bones}
        if retain_all_donor_weights
        else set(FALLBACK_BONES)
    )
    for bone in armature.data.bones:
        bone.use_deform = bone.name in deform
    return [bone.name for bone in armature.data.bones if bone.use_deform]


def refit_compact_standing_armature(
    armature: bpy.types.Object, *, height_m: float
) -> Dict[str, Any]:
    """Refit the donor topology to a compact standing rest silhouette.

    The exact source is intentionally not remeshed or forced into a T-pose.  Instead, the donor's
    major joints are moved to a conservative arms-down rest skeleton, after which Blender bone
    heat operates on the source in its supplied pose.  Finger/end bones stay in the 49-bone rig but
    are non-deforming until a later hand-specific art pass.
    """

    unit = height_m / recipe.TARGET_HEIGHT_M
    points = {
        "Hips": ((0.0, 0.0, 0.88), (0.0, 0.0, 0.98)),
        "Spine": ((0.0, 0.0, 0.98), (0.0, 0.0, 1.13)),
        "Spine1": ((0.0, 0.0, 1.13), (0.0, 0.0, 1.30)),
        "Spine2": ((0.0, 0.0, 1.30), (0.0, 0.0, 1.46)),
        "Neck": ((0.0, 0.0, 1.46), (0.0, 0.0, 1.55)),
        "Head": ((0.0, 0.0, 1.55), (0.0, 0.0, 1.69)),
        "HeadTop_End": ((0.0, 0.0, 1.69), (0.0, 0.0, 1.72)),
        "LeftShoulder": ((0.0, 0.0, 1.43), (0.18, 0.0, 1.43)),
        "LeftArm": ((0.18, 0.0, 1.43), (0.255, -0.015, 1.20)),
        "LeftForeArm": ((0.255, -0.015, 1.20), (0.255, -0.040, 0.96)),
        "LeftHand": ((0.255, -0.040, 0.96), (0.255, -0.055, 0.82)),
        "RightShoulder": ((0.0, 0.0, 1.43), (-0.18, 0.0, 1.43)),
        "RightArm": ((-0.18, 0.0, 1.43), (-0.255, -0.015, 1.20)),
        "RightForeArm": ((-0.255, -0.015, 1.20), (-0.255, -0.040, 0.96)),
        "RightHand": ((-0.255, -0.040, 0.96), (-0.255, -0.055, 0.82)),
        "LeftUpLeg": ((0.105, 0.0, 0.92), (0.115, 0.0, 0.52)),
        "LeftLeg": ((0.115, 0.0, 0.52), (0.115, 0.0, 0.105)),
        "LeftFoot": ((0.115, 0.0, 0.105), (0.115, -0.21, 0.055)),
        "RightUpLeg": ((-0.105, 0.0, 0.92), (-0.115, 0.0, 0.52)),
        "RightLeg": ((-0.115, 0.0, 0.52), (-0.115, 0.0, 0.105)),
        "RightFoot": ((-0.115, 0.0, 0.105), (-0.115, -0.21, 0.055)),
    }
    world_to_armature = armature.matrix_world.inverted_safe()
    bpy.ops.object.select_all(action="DESELECT")
    armature.select_set(True)
    bpy.context.view_layer.objects.active = armature
    bpy.ops.object.mode_set(mode="EDIT")
    for edit_bone in armature.data.edit_bones:
        edit_bone.use_connect = False
    for bone_name, (head, tail) in points.items():
        edit_bone = armature.data.edit_bones.get(bone_name)
        if edit_bone is None:
            raise RuntimeError(f"cannot refit missing donor edit bone {bone_name!r}")
        head_world = Vector(tuple(value * unit for value in head))
        tail_world = Vector(tuple(value * unit for value in tail))
        edit_bone.head = world_to_armature @ head_world
        edit_bone.tail = world_to_armature @ tail_world
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.context.view_layer.update()
    return {
        "method": "major donor edit bones refitted to normalized compact standing joints",
        "joint_count": len(points),
        "deforming_bones": list(FALLBACK_BONES),
        "non_deforming_detail_bones_retained": True,
    }


def refit_a_pose_armature(
    armature: bpy.types.Object, *, height_m: float
) -> Dict[str, Any]:
    """Refit major donor joints to the selected source's separated A-pose silhouette.

    The exact mesh remains untouched. This prevents a diagonally authored sleeve from borrowing
    torso weights merely because the donor's original T-pose arm surface is far away. Dimensions
    are normalized human landmarks and intentionally leave the 49-bone donor topology intact.
    """

    unit = height_m / recipe.TARGET_HEIGHT_M
    points = {
        "Hips": ((0.0, 0.0, 0.88), (0.0, 0.0, 0.98)),
        "Spine": ((0.0, 0.0, 0.98), (0.0, 0.0, 1.13)),
        "Spine1": ((0.0, 0.0, 1.13), (0.0, 0.0, 1.30)),
        "Spine2": ((0.0, 0.0, 1.30), (0.0, 0.0, 1.46)),
        "Neck": ((0.0, 0.0, 1.46), (0.0, 0.0, 1.55)),
        "Head": ((0.0, 0.0, 1.55), (0.0, 0.0, 1.69)),
        "HeadTop_End": ((0.0, 0.0, 1.69), (0.0, 0.0, 1.72)),
        "LeftShoulder": ((0.0, 0.0, 1.43), (0.18, 0.0, 1.43)),
        "LeftArm": ((0.18, 0.0, 1.43), (0.42, -0.01, 1.20)),
        "LeftForeArm": ((0.42, -0.01, 1.20), (0.62, -0.02, 0.99)),
        "LeftHand": ((0.62, -0.02, 0.99), (0.70, -0.03, 0.91)),
        "RightShoulder": ((0.0, 0.0, 1.43), (-0.18, 0.0, 1.43)),
        "RightArm": ((-0.18, 0.0, 1.43), (-0.42, -0.01, 1.20)),
        "RightForeArm": ((-0.42, -0.01, 1.20), (-0.62, -0.02, 0.99)),
        "RightHand": ((-0.62, -0.02, 0.99), (-0.70, -0.03, 0.91)),
        "LeftUpLeg": ((0.105, 0.0, 0.92), (0.115, 0.0, 0.52)),
        "LeftLeg": ((0.115, 0.0, 0.52), (0.115, 0.0, 0.105)),
        "LeftFoot": ((0.115, 0.0, 0.105), (0.115, -0.21, 0.055)),
        "RightUpLeg": ((-0.105, 0.0, 0.92), (-0.115, 0.0, 0.52)),
        "RightLeg": ((-0.115, 0.0, 0.52), (-0.115, 0.0, 0.105)),
        "RightFoot": ((-0.115, 0.0, 0.105), (-0.115, -0.21, 0.055)),
    }
    world_to_armature = armature.matrix_world.inverted_safe()
    bpy.ops.object.select_all(action="DESELECT")
    armature.select_set(True)
    bpy.context.view_layer.objects.active = armature
    bpy.ops.object.mode_set(mode="EDIT")
    for edit_bone in armature.data.edit_bones:
        edit_bone.use_connect = False
    for bone_name, (head, tail) in points.items():
        edit_bone = armature.data.edit_bones.get(bone_name)
        if edit_bone is None:
            raise RuntimeError(f"cannot refit missing donor edit bone {bone_name!r}")
        edit_bone.head = world_to_armature @ Vector(tuple(value * unit for value in head))
        edit_bone.tail = world_to_armature @ Vector(tuple(value * unit for value in tail))
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.context.view_layer.update()
    return {
        "method": "major donor edit bones refitted to normalized separated A-pose joints",
        "joint_count": len(points),
        "deforming_bones": list(FALLBACK_BONES),
        "non_deforming_detail_bones_retained": True,
    }


def parent_to_armature_keep_world(obj: bpy.types.Object, armature: bpy.types.Object) -> None:
    world = obj.matrix_world.copy()
    obj.parent = armature
    obj.parent_type = "OBJECT"
    obj.matrix_world = world


def build_voxel_weight_proxy(
    source_meshes: Sequence[bpy.types.Object], *, height_m: float
) -> Tuple[bpy.types.Object, Dict[str, Any]]:
    """Build a disposable watertight-ish heat cage without modifying source geometry.

    Automatic bone heat commonly fails on layered coats, separate boots, or non-manifold glTF
    seams.  A voxel union gives Blender a clean volume to solve; its resulting groups are later
    surface-transferred to the exact source vertices and this proxy is deleted before export.
    """

    proxy_mesh = bpy.data.meshes.new("SovietInfantry_DisposableVoxelHeatCageMesh")
    builder = bmesh.new()
    source_vertex_count = 0
    for source in source_meshes:
        first_new_vertex = len(builder.verts)
        builder.from_mesh(source.data)
        builder.verts.ensure_lookup_table()
        new_vertices = list(builder.verts[first_new_vertex:])
        bmesh.ops.transform(builder, matrix=source.matrix_world, verts=new_vertices)
        source_vertex_count += len(new_vertices)
    builder.to_mesh(proxy_mesh)
    builder.free()
    proxy_mesh.update()
    proxy = bpy.data.objects.new("SovietInfantry_DisposableVoxelHeatCage", proxy_mesh)
    bpy.context.scene.collection.objects.link(proxy)
    bpy.ops.object.select_all(action="DESELECT")
    proxy.select_set(True)
    bpy.context.view_layer.objects.active = proxy
    voxel_size = height_m / 96.0
    proxy.data.remesh_voxel_size = voxel_size
    proxy.data.remesh_voxel_adaptivity = 0.0
    vertices_before = len(proxy.data.vertices)
    result = bpy.ops.object.voxel_remesh()
    if "FINISHED" not in result or not proxy.data.vertices:
        raise RuntimeError(f"voxel heat-cage remesh failed: {result}")
    return proxy, {
        "method": "disposable voxel union used only as a bone-heat/weight-transfer cage",
        "voxel_size_m": round(voxel_size, 8),
        "source_vertices": source_vertex_count,
        "proxy_vertices": len(proxy.data.vertices),
        "source_geometry_exported": False,
    }


def transfer_proxy_weights(
    source_meshes: Sequence[bpy.types.Object],
    proxy: bpy.types.Object,
    armature: bpy.types.Object,
    bone_names: Sequence[str],
    *,
    weight_source: str,
) -> List[Dict[str, Any]]:
    bone_set = set(bone_names)
    reports = []
    for obj in source_meshes:
        # Data Transfer must evaluate against the exact source rest geometry, not after a partially
        # configured armature modifier.  Re-add one clean armature modifier after the weights have
        # been applied; this also avoids Blender's "modifier was not first" ambiguity.
        for modifier in list(obj.modifiers):
            if modifier.type == "ARMATURE":
                obj.modifiers.remove(modifier)
        for group in list(obj.vertex_groups):
            if group.name in bone_set:
                obj.vertex_groups.remove(group)
        for bone_name in bone_names:
            obj.vertex_groups.new(name=bone_name)
        modifier = obj.modifiers.new("Kessel_VoxelCageWeightTransfer", "DATA_TRANSFER")
        modifier.object = proxy
        modifier.use_vert_data = True
        modifier.data_types_verts = {"VGROUP_WEIGHTS"}
        modifier.vert_mapping = "POLYINTERP_NEAREST"
        modifier.layers_vgroup_select_src = "ALL"
        modifier.layers_vgroup_select_dst = "NAME"
        modifier.mix_mode = "REPLACE"
        apply_modifier(obj, modifier.name)
        report = normalize_vertex_weights(obj, armature, bone_names)
        armature_modifier = obj.modifiers.new("Kessel_SovietRig", "ARMATURE")
        armature_modifier.object = armature
        armature_modifier.use_vertex_groups = True
        report["object"] = obj.name
        report["weight_source"] = weight_source
        reports.append(report)
    return reports


def build_compact_donor_weight_proxy(
    donor_mesh: bpy.types.Object,
    armature: bpy.types.Object,
    *,
    height_m: float,
) -> Tuple[bpy.types.Object, Dict[str, Any]]:
    """Evaluate the known donor skin in the shipped arms-down crew posture.

    A compact source's sleeves lie close to its coat, where a single voxel union cannot reliably
    tell arm surface from torso surface.  The donor already has authored anatomical weights.  We
    temporarily pose that donor to the same relaxed silhouette used by Kessel's current crew bake,
    freeze its evaluated surface while retaining vertex-group values, and use it only as a transfer
    cage.  It is never exported and does not replace any Soviet mesh/material/texture data.
    """

    armature.data.pose_position = "POSE"
    reset_pose(armature)
    unit = height_m / 1.53
    add_ik(
        armature,
        "LeftForeArm",
        tuple(value * unit for value in (0.24, -0.05, 1.02)),
        tuple(value * unit for value in (0.50, -0.17, 1.22)),
        2,
        90.0,
    )
    add_ik(
        armature,
        "RightForeArm",
        tuple(value * unit for value in (-0.22, 0.01, 0.97)),
        tuple(value * unit for value in (-0.49, -0.12, 1.16)),
        2,
        -90.0,
    )
    bpy.context.view_layer.update()
    depsgraph = bpy.context.evaluated_depsgraph_get()
    evaluated = donor_mesh.evaluated_get(depsgraph)
    if len(evaluated.data.vertices) != len(donor_mesh.data.vertices):
        raise RuntimeError("posed donor proxy unexpectedly changed vertex topology")
    mesh = bpy.data.meshes.new_from_object(
        evaluated,
        preserve_all_data_layers=True,
        depsgraph=depsgraph,
    )
    mesh.transform(evaluated.matrix_world)
    mesh.update()
    proxy = bpy.data.objects.new("SovietInfantry_DisposablePosedDonorCage", mesh)
    bpy.context.scene.collection.objects.link(proxy)
    for source_group in donor_mesh.vertex_groups:
        proxy.vertex_groups.new(name=source_group.name)
    for vertex in donor_mesh.data.vertices:
        for membership in vertex.groups:
            group_name = donor_mesh.vertex_groups[membership.group].name
            proxy.vertex_groups[group_name].add(
                [vertex.index], float(membership.weight), "REPLACE"
            )
    reset_pose(armature)
    return proxy, {
        "method": "evaluated arms-down German donor surface with authored donor weights",
        "vertices": len(mesh.vertices),
        "source_geometry_exported": False,
        "target_soviet_geometry_modified": False,
        "pose_reference": "tools/bake_crew_soldier_variants.py pose_relaxed",
    }


def automatic_compact_weights(
    source_meshes: Sequence[bpy.types.Object],
    armature: bpy.types.Object,
    *,
    height_m: float,
    posed_donor_proxy: Optional[bpy.types.Object] = None,
    posed_donor_report: Optional[Dict[str, Any]] = None,
) -> List[Dict[str, Any]]:
    bone_names = [bone.name for bone in armature.data.bones if bone.use_deform]
    bone_set = set(bone_names)
    proxy, proxy_report = build_voxel_weight_proxy(source_meshes, height_m=height_m)
    for obj in source_meshes:
        for modifier in list(obj.modifiers):
            if modifier.type == "ARMATURE":
                obj.modifiers.remove(modifier)
        for group in list(obj.vertex_groups):
            if group.name in bone_set:
                obj.vertex_groups.remove(group)
    bpy.ops.object.select_all(action="DESELECT")
    for obj in source_meshes:
        obj.select_set(True)
    armature.select_set(True)
    bpy.context.view_layer.objects.active = armature
    heat_succeeded = True
    try:
        result = bpy.ops.object.parent_set(type="ARMATURE_AUTO", keep_transform=True)
        heat_succeeded = "FINISHED" in result
    except RuntimeError:
        heat_succeeded = False
    heat_coverage_by_object = {}
    for obj in source_meshes:
        heat_covered, heat_coverage = existing_weight_coverage(obj, bone_names)
        heat_coverage_by_object[obj.name] = (heat_covered, heat_coverage)

    proxy_heat_succeeded = True
    bpy.ops.object.select_all(action="DESELECT")
    proxy.select_set(True)
    armature.select_set(True)
    bpy.context.view_layer.objects.active = armature
    try:
        proxy_result = bpy.ops.object.parent_set(type="ARMATURE_AUTO", keep_transform=True)
        proxy_heat_succeeded = "FINISHED" in proxy_result
    except RuntimeError:
        proxy_heat_succeeded = False
    proxy_covered, proxy_coverage = existing_weight_coverage(proxy, bone_names)
    proxy_report.update(
        {
            "bone_heat_operator_finished": proxy_heat_succeeded,
            "bone_heat_covered_vertices": proxy_covered,
            "bone_heat_coverage": round(proxy_coverage, 6),
        }
    )
    use_direct_heat = all(coverage >= 0.95 for _, coverage in heat_coverage_by_object.values())
    proxy_reports: Dict[str, Dict[str, Any]] = {}
    weight_proxy_kind = None
    if not use_direct_heat and proxy_coverage >= 0.95:
        proxy_reports = {
            report["object"]: report
            for report in transfer_proxy_weights(
                source_meshes,
                proxy,
                armature,
                bone_names,
                weight_source="disposable voxel heat cage",
            )
        }
        weight_proxy_kind = "voxel_heat_cage"
    elif not use_direct_heat and posed_donor_proxy is not None:
        proxy_reports = {
            report["object"]: report
            for report in transfer_proxy_weights(
                source_meshes,
                posed_donor_proxy,
                armature,
                bone_names,
                weight_source="posed donor anatomical surface transfer",
            )
        }
        weight_proxy_kind = "posed_donor"

    reports = []
    for obj in source_meshes:
        if obj.parent != armature:
            parent_to_armature_keep_world(obj, armature)
        armature_modifiers = [modifier for modifier in obj.modifiers if modifier.type == "ARMATURE"]
        if not armature_modifiers:
            modifier = obj.modifiers.new("Kessel_SovietRig", "ARMATURE")
            modifier.object = armature
            modifier.use_vertex_groups = True
        else:
            for modifier in armature_modifiers:
                modifier.object = armature
                modifier.use_vertex_groups = True
        for bone_name in bone_names:
            if obj.vertex_groups.get(bone_name) is None:
                obj.vertex_groups.new(name=bone_name)
        heat_covered, heat_coverage = heat_coverage_by_object[obj.name]
        if use_direct_heat:
            report = normalize_vertex_weights(obj, armature, bone_names)
            report["fallback_strategy"] = "nearest segment for residual uncovered vertices only"
            report["weight_source"] = "direct source-mesh bone heat"
        elif obj.name in proxy_reports:
            report = proxy_reports[obj.name]
            report["fallback_strategy"] = (
                f"{weight_proxy_kind} surface transfer, then nearest segment for residual "
                "uncovered vertices only"
            )
        else:
            report = compact_anatomical_weights(
                obj,
                armature,
                bone_names,
                height_m=height_m,
            )
            report["distance_fallback_vertices"] = len(obj.data.vertices)
            report["fallback_strategy"] = "full region-constrained anatomical weighting"
            report["weight_source"] = "deterministic anatomical emergency fallback"
        report["bone_heat_covered_vertices"] = heat_covered
        report["bone_heat_coverage"] = round(heat_coverage, 6)
        report["object"] = obj.name
        report["bone_heat_operator_finished"] = heat_succeeded
        report["direct_bone_heat_contributed"] = use_direct_heat
        report["bone_heat_contributed"] = (
            use_direct_heat or weight_proxy_kind == "voxel_heat_cage"
        )
        report["voxel_heat_cage"] = proxy_report
        if posed_donor_report is not None and weight_proxy_kind == "posed_donor":
            report["posed_donor_cage"] = posed_donor_report
        reports.append(report)
    bpy.data.objects.remove(proxy, do_unlink=True)
    if posed_donor_proxy is not None:
        bpy.data.objects.remove(posed_donor_proxy, do_unlink=True)
    return reports


def vertex_bone_weights(
    obj: bpy.types.Object, vertex: bpy.types.MeshVertex, bone_groups: set
) -> Dict[str, float]:
    weights: Dict[str, float] = {}
    for membership in vertex.groups:
        group = obj.vertex_groups[membership.group]
        if group.name in bone_groups and membership.weight > 0.0:
            weights[group.name] = float(membership.weight)
    return weights


def normalize_vertex_weights(
    obj: bpy.types.Object,
    armature: bpy.types.Object,
    bone_names: Sequence[str],
) -> Dict[str, Any]:
    bone_set = set(bone_names)
    segments = {name: bone_world_segment(armature, name) for name in FALLBACK_BONES}
    covered_before = 0
    fallback_vertices = 0
    influence_histogram: Dict[str, int] = {}
    for vertex in obj.data.vertices:
        weights = vertex_bone_weights(obj, vertex, bone_set)
        if sum(weights.values()) > 1.0e-6:
            covered_before += 1
        else:
            fallback_vertices += 1
            point = obj.matrix_world @ vertex.co
            distances = sorted(
                (
                    (point_segment_distance(point, start, end), bone_name)
                    for bone_name, (start, end) in segments.items()
                ),
                key=lambda item: (item[0], item[1]),
            )[:MAX_VERTEX_INFLUENCES]
            weights = {
                bone_name: 1.0 / max(distance, 0.015) ** 2
                for distance, bone_name in distances
            }
        selected = sorted(weights.items(), key=lambda item: (-item[1], item[0]))[
            :MAX_VERTEX_INFLUENCES
        ]
        total = sum(weight for _, weight in selected)
        if total <= 1.0e-12:
            raise RuntimeError(f"failed to weight vertex {vertex.index} on {obj.name}")
        selected_names = {name for name, _ in selected}
        for group_name in bone_names:
            group = obj.vertex_groups.get(group_name)
            if group is not None and group_name not in selected_names:
                try:
                    group.remove([vertex.index])
                except RuntimeError:
                    pass
        for group_name, weight in selected:
            obj.vertex_groups[group_name].add([vertex.index], weight / total, "REPLACE")
        count = len(selected)
        influence_histogram[str(count)] = influence_histogram.get(str(count), 0) + 1
    return {
        "vertices": len(obj.data.vertices),
        "data_transfer_covered_vertices": covered_before,
        "data_transfer_coverage": round(covered_before / max(1, len(obj.data.vertices)), 6),
        "distance_fallback_vertices": fallback_vertices,
        "maximum_influences": MAX_VERTEX_INFLUENCES,
        "influence_histogram": influence_histogram,
    }


def existing_weight_coverage(
    obj: bpy.types.Object, bone_names: Sequence[str]
) -> Tuple[int, float]:
    bone_set = set(bone_names)
    covered = sum(
        1
        for vertex in obj.data.vertices
        if sum(vertex_bone_weights(obj, vertex, bone_set).values()) > 1.0e-6
    )
    return covered, covered / max(1, len(obj.data.vertices))


def replace_vertex_weights(
    obj: bpy.types.Object,
    vertex_index: int,
    bone_names: Sequence[str],
    weights: Dict[str, float],
) -> int:
    selected = sorted(weights.items(), key=lambda item: (-item[1], item[0]))[
        :MAX_VERTEX_INFLUENCES
    ]
    total = sum(weight for _, weight in selected)
    if total <= 1.0e-12:
        raise RuntimeError(f"cannot assign zero total weight on {obj.name} vertex {vertex_index}")
    selected_names = {name for name, _ in selected}
    for group_name in bone_names:
        group = obj.vertex_groups.get(group_name)
        if group is not None and group_name not in selected_names:
            try:
                group.remove([vertex_index])
            except RuntimeError:
                pass
    for group_name, weight in selected:
        obj.vertex_groups[group_name].add([vertex_index], weight / total, "REPLACE")
    return len(selected)


def compact_region_candidates(
    point: Vector,
    *,
    height_m: float,
    force_head: bool,
) -> Tuple[str, ...]:
    if force_head:
        return ("Head",)
    normalized_height = point.z / max(height_m, 1.0e-6)
    side = "Left" if point.x >= 0.0 else "Right"
    outer_upper_body = abs(point.x) > height_m * 0.105 and normalized_height > 0.48
    if normalized_height < 0.12:
        return (f"{side}Foot", f"{side}Leg")
    if normalized_height < 0.34:
        return (f"{side}Leg", f"{side}UpLeg")
    if normalized_height < 0.56:
        if abs(point.x) < height_m * 0.055:
            return ("Hips", f"{side}UpLeg")
        return (f"{side}UpLeg", f"{side}Leg", "Hips")
    if normalized_height >= 0.88:
        return ("Head", "Neck")
    if outer_upper_body:
        if normalized_height >= 0.79:
            return (f"{side}Shoulder", f"{side}Arm")
        if normalized_height >= 0.66:
            return (f"{side}Arm", f"{side}ForeArm")
        return (f"{side}ForeArm", f"{side}Hand")
    if normalized_height < 0.64:
        return ("Hips", "Spine")
    if normalized_height < 0.74:
        return ("Spine", "Spine1")
    if normalized_height < 0.83:
        return ("Spine1", "Spine2")
    return ("Spine2", "Neck", "Head")


def compact_anatomical_weights(
    obj: bpy.types.Object,
    armature: bpy.types.Object,
    bone_names: Sequence[str],
    *,
    height_m: float,
) -> Dict[str, Any]:
    """Assign continuous weights within one anatomical chain at a time.

    An unrestricted nearest-bone fallback can give one coat vertex simultaneous spine, opposite
    arm, and leg influences.  It may keep plausible bounds while tearing triangles apart.  This
    fallback first classifies a vertex into head, central spine, same-side arm, or same-side leg,
    then blends only adjacent bones in that chain.
    """

    segments = {name: bone_world_segment(armature, name) for name in bone_names}
    influence_histogram: Dict[str, int] = {}
    force_head = "helmet" in obj.name.casefold() or "headgear" in obj.name.casefold()
    for vertex in obj.data.vertices:
        point = obj.matrix_world @ vertex.co
        candidates = compact_region_candidates(
            point,
            height_m=height_m,
            force_head=force_head,
        )
        weights = {}
        for bone_name in candidates:
            start, end = segments[bone_name]
            distance = point_segment_distance(point, start, end)
            weights[bone_name] = 1.0 / max(distance, 0.025) ** 2
        count = replace_vertex_weights(obj, vertex.index, bone_names, weights)
        influence_histogram[str(count)] = influence_histogram.get(str(count), 0) + 1
    smoothing_iterations = 10 if not force_head else 0
    if smoothing_iterations:
        smooth_vertex_weights(
            obj,
            bone_names,
            iterations=smoothing_iterations,
            self_weight=0.62,
        )
    return {
        "strategy": "region-constrained adjacent-bone inverse-distance blend",
        "vertices": len(obj.data.vertices),
        "maximum_influences": MAX_VERTEX_INFLUENCES,
        "influence_histogram": influence_histogram,
        "force_head_chain": force_head,
        "topology_smoothing_iterations": smoothing_iterations,
    }


def smooth_vertex_weights(
    obj: bpy.types.Object,
    bone_names: Sequence[str],
    *,
    iterations: int,
    self_weight: float,
) -> None:
    adjacency = [set() for _ in obj.data.vertices]
    for edge in obj.data.edges:
        first, second = edge.vertices
        adjacency[first].add(second)
        adjacency[second].add(first)
    # glTF commonly splits a visual vertex at UV, material, or hard-normal seams.  Those duplicate
    # vertices are not connected by Mesh.edges, so smoothing only topology would give coincident
    # triangle corners different bone chains and produce needle-like gaps.  Weld them logically for
    # weighting while leaving the source topology and UV seams untouched.
    coincident: Dict[Tuple[int, int, int], List[int]] = {}
    for vertex in obj.data.vertices:
        point = obj.matrix_world @ vertex.co
        key = tuple(round(float(point[axis]) * 100000.0) for axis in range(3))
        coincident.setdefault(key, []).append(vertex.index)
    for indices in coincident.values():
        if len(indices) < 2:
            continue
        anchor = indices[0]
        for other in indices[1:]:
            adjacency[anchor].add(other)
            adjacency[other].add(anchor)
    bone_set = set(bone_names)
    for _ in range(iterations):
        previous = [vertex_bone_weights(obj, vertex, bone_set) for vertex in obj.data.vertices]
        updated: List[Dict[str, float]] = []
        for vertex in obj.data.vertices:
            own = previous[vertex.index]
            neighbors = adjacency[vertex.index]
            if not neighbors:
                updated.append(own)
                continue
            neighbor_scale = (1.0 - self_weight) / len(neighbors)
            blended = {name: weight * self_weight for name, weight in own.items()}
            for neighbor in neighbors:
                for name, weight in previous[neighbor].items():
                    blended[name] = blended.get(name, 0.0) + weight * neighbor_scale
            updated.append(blended)
        for vertex, weights in zip(obj.data.vertices, updated):
            replace_vertex_weights(obj, vertex.index, bone_names, weights)


def transfer_skin_weights(
    source_meshes: Sequence[bpy.types.Object],
    donor_mesh: bpy.types.Object,
    armature: bpy.types.Object,
) -> List[Dict[str, Any]]:
    bone_names = [bone.name for bone in armature.data.bones if bone.use_deform]
    reports: List[Dict[str, Any]] = []
    for obj in source_meshes:
        for bone_name in bone_names:
            if obj.vertex_groups.get(bone_name) is None:
                obj.vertex_groups.new(name=bone_name)
        modifier = obj.modifiers.new("Kessel_DonorWeightTransfer", "DATA_TRANSFER")
        modifier.object = donor_mesh
        modifier.use_vert_data = True
        modifier.data_types_verts = {"VGROUP_WEIGHTS"}
        modifier.vert_mapping = "POLYINTERP_NEAREST"
        modifier.layers_vgroup_select_src = "ALL"
        modifier.layers_vgroup_select_dst = "NAME"
        modifier.mix_mode = "REPLACE"
        apply_modifier(obj, modifier.name)
        report = normalize_vertex_weights(obj, armature, bone_names)
        report["object"] = obj.name
        armature_modifier = obj.modifiers.new("Kessel_SovietRig", "ARMATURE")
        armature_modifier.object = armature
        armature_modifier.use_vertex_groups = True
        parent_to_armature_keep_world(obj, armature)
        reports.append(report)
    return reports


def max_rest_displacement(
    before: Dict[str, List[Vector]], after: Dict[str, List[Vector]]
) -> float:
    if set(before) != set(after):
        raise RuntimeError("rest-pose validation object set changed during rigging")
    maximum = 0.0
    for object_name in before:
        if len(before[object_name]) != len(after[object_name]):
            raise RuntimeError(f"rest-pose topology changed on {object_name}")
        for original, evaluated in zip(before[object_name], after[object_name]):
            maximum = max(maximum, (original - evaluated).length)
    return maximum


def material(
    name: str,
    color: Tuple[float, float, float, float],
    roughness: float,
    metallic: float,
):
    result = bpy.data.materials.get(name)
    if result is None:
        result = bpy.data.materials.new(name)
    result.use_nodes = True
    result.diffuse_color = color
    result.roughness = roughness
    result.metallic = metallic
    shader = result.node_tree.nodes.get("Principled BSDF")
    if shader is not None:
        shader.inputs["Base Color"].default_value = color
        shader.inputs["Roughness"].default_value = roughness
        shader.inputs["Metallic"].default_value = metallic
    return result


def cube_component(
    name: str,
    location: Tuple[float, float, float],
    dimensions: Tuple[float, float, float],
    component_material,
) -> bpy.types.Object:
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=location)
    obj = bpy.context.object
    obj.name = name
    obj.dimensions = dimensions
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.data.materials.append(component_material)
    return obj


def build_rifle(
    *,
    armature: Optional[bpy.types.Object],
    weapon_pose: str,
    recoil_m: float,
) -> bpy.types.Object:
    """Build one shared, low-poly rifle silhouette; no third-party weapon asset is implied."""

    wood = material("Kessel_SovietRifle_Wood", (0.20, 0.075, 0.025, 1.0), 0.74, 0.0)
    steel = material("Kessel_SovietRifle_Steel", (0.055, 0.060, 0.058, 1.0), 0.30, 0.72)
    components = [
        cube_component("Rifle_Stock", (0.0, 0.285, 0.0), (0.080, 0.34, 0.105), wood),
        cube_component("Rifle_Receiver", (0.0, 0.055, 0.008), (0.070, 0.18, 0.070), steel),
        cube_component("Rifle_Handguard", (0.0, -0.235, 0.006), (0.062, 0.41, 0.060), wood),
        cube_component("Rifle_Bolt", (-0.058, 0.075, 0.030), (0.055, 0.032, 0.025), steel),
        cube_component("Rifle_Sight", (0.0, -0.510, 0.055), (0.025, 0.030, 0.065), steel),
    ]
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=8,
        radius=0.014,
        depth=0.48,
        location=(0.0, -0.535, 0.018),
        rotation=(math.pi * 0.5, 0.0, 0.0),
    )
    barrel = bpy.context.object
    barrel.name = "Rifle_Barrel"
    barrel.data.materials.append(steel)
    components.append(barrel)
    bpy.ops.object.select_all(action="DESELECT")
    for component in components:
        component.select_set(True)
    bpy.context.view_layer.objects.active = components[0]
    result = bpy.ops.object.join()
    if "FINISHED" not in result:
        raise RuntimeError(f"could not join generated rifle: {result}")
    rifle = bpy.context.object
    rifle.name = "SovietRifle_Generated"
    rifle["kessel_generated_rifle"] = True
    bpy.ops.object.origin_set(type="ORIGIN_GEOMETRY", center="BOUNDS")

    placement = {
        "low_ready": ((0.0, -0.34 + recoil_m, 1.10), radians(-11.0)),
        "run": ((0.0, -0.28 + recoil_m, 1.16), radians(-24.0)),
        "aim": ((0.0, -0.37 + recoil_m, 1.31), 0.0),
        "flinch": ((0.0, -0.25 + recoil_m, 1.18), radians(-31.0)),
        "loose": ((0.05, -0.20 + recoil_m, 1.02), radians(-47.0)),
    }
    rifle.location, pitch = placement[weapon_pose]
    rifle.rotation_euler[0] = pitch
    bpy.ops.object.select_all(action="DESELECT")
    rifle.select_set(True)
    bpy.context.view_layer.objects.active = rifle
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    bpy.context.view_layer.update()
    if armature is not None:
        group = rifle.vertex_groups.new(name="RightHand")
        group.add([vertex.index for vertex in rifle.data.vertices], 1.0, "REPLACE")
        modifier = rifle.modifiers.new("Kessel_RifleHand", "ARMATURE")
        modifier.object = armature
        modifier.use_vertex_groups = True
        parent_to_armature_keep_world(rifle, armature)
    return rifle


def target(name: str, location: Tuple[float, float, float]) -> bpy.types.Object:
    obj = bpy.data.objects.new(name, None)
    obj.empty_display_type = "PLAIN_AXES"
    obj.location = location
    obj["kessel_pose_target"] = True
    bpy.context.scene.collection.objects.link(obj)
    return obj


def add_ik(
    armature: bpy.types.Object,
    end_bone: str,
    target_location: Tuple[float, float, float],
    pole_location: Tuple[float, float, float],
    chain_count: int,
    pole_angle_degrees: float,
    influence: float = 1.0,
) -> None:
    goal = target(f"Kessel_{end_bone}_Goal", target_location)
    pole = target(f"Kessel_{end_bone}_Pole", pole_location)
    constraint = armature.pose.bones[end_bone].constraints.new("IK")
    constraint.name = f"Kessel_{end_bone}_IK"
    constraint.target = goal
    constraint.pole_target = pole
    constraint.chain_count = chain_count
    constraint.use_tail = True
    constraint.use_stretch = False
    constraint.pole_angle = radians(pole_angle_degrees)
    constraint.influence = influence


def remove_pose_targets() -> None:
    for obj in list(bpy.context.scene.objects):
        if obj.get("kessel_pose_target"):
            bpy.data.objects.remove(obj, do_unlink=True)


def reset_pose(armature: bpy.types.Object) -> None:
    for pose_bone in armature.pose.bones:
        for constraint in list(pose_bone.constraints):
            pose_bone.constraints.remove(constraint)
        pose_bone.rotation_mode = "QUATERNION"
        pose_bone.location = (0.0, 0.0, 0.0)
        pose_bone.rotation_quaternion = (1.0, 0.0, 0.0, 0.0)
        pose_bone.scale = (1.0, 1.0, 1.0)
    remove_pose_targets()
    bpy.context.view_layer.update()


def frame_targets(
    frame: recipe.PoseFrame, height_m: float
) -> Dict[str, Tuple[float, float, float]]:
    unit = height_m / recipe.TARGET_HEIGHT_M
    phase = frame.phase * math.tau
    if frame.gait == "walk":
        stride, lift = 0.18 * unit, 0.075 * unit
    elif frame.gait == "run":
        stride, lift = 0.30 * unit, 0.14 * unit
    else:
        stride, lift = 0.0, 0.0
    wave = math.sin(phase)
    left_y = -stride * wave
    right_y = stride * wave
    left_z = lift * max(0.0, wave)
    right_z = lift * max(0.0, -wave)

    hands = {
        "low_ready": ((0.105, -0.445, 1.055), (-0.105, -0.185, 1.145)),
        "run": ((0.120, -0.390, 1.105), (-0.120, -0.135, 1.205)),
        "aim": ((0.105, -0.535, 1.305), (-0.105, -0.185, 1.315)),
        "flinch": ((0.180, -0.325, 1.205), (-0.190, -0.115, 1.115)),
        "loose": ((0.245, -0.105, 0.965), (-0.225, 0.015, 0.900)),
    }
    left_hand, right_hand = hands[frame.weapon_pose]
    recoil = frame.recoil_m
    left_hand = (left_hand[0], left_hand[1] + recoil, left_hand[2] + recoil * 0.16)
    right_hand = (right_hand[0], right_hand[1] + recoil, right_hand[2] + recoil * 0.16)

    def scaled(values):
        return tuple(float(value) * unit for value in values)

    return {
        "LeftFoot": scaled((0.135, left_y / unit, left_z / unit)),
        "RightFoot": scaled((-0.135, right_y / unit, right_z / unit)),
        "LeftKneePole": scaled((0.19, -0.55 + left_y / unit, 0.54)),
        "RightKneePole": scaled((-0.19, -0.55 + right_y / unit, 0.54)),
        "LeftHand": scaled(left_hand),
        "RightHand": scaled(right_hand),
        "LeftElbowPole": scaled((0.47, -0.16, 1.23)),
        "RightElbowPole": scaled((-0.47, -0.14, 1.23)),
    }


def evaluated_pose_matrices(
    armature: bpy.types.Object, frame: recipe.PoseFrame, height_m: float
) -> Dict[str, Matrix]:
    reset_pose(armature)
    targets = frame_targets(frame, height_m)
    compact_standing = bool(armature.get("kessel_compact_standing"))
    hips = armature.pose.bones.get("Hips")
    if hips is not None and not (compact_standing and frame.reaction != "none"):
        hips.location.z += frame.body_bob * (height_m / recipe.TARGET_HEIGHT_M)
    apply_leg_ik = frame.gait in ("walk", "run") and (
        not compact_standing or abs(math.sin(frame.phase * math.tau)) > 1.0e-5
    )
    if apply_leg_ik:
        # Match the established crew bake: solve thigh + shin by constraining the lower-leg tail
        # (the ankle). Targeting the foot with a two-bone chain excludes the thigh and folds the
        # shin/boot over itself. Deriving from real rest tails also makes phase zero exact.
        unit = height_m / recipe.TARGET_HEIGHT_M
        phase_wave = math.sin(frame.phase * math.tau)
        stride = (0.18 if frame.gait == "walk" else 0.30) * unit
        lift = (0.075 if frame.gait == "walk" else 0.14) * unit
        left_rest = armature.matrix_world @ armature.data.bones["LeftLeg"].tail_local
        right_rest = armature.matrix_world @ armature.data.bones["RightLeg"].tail_local
        targets["LeftFoot"] = tuple(
            left_rest
            + Vector(
                (
                    0.0,
                    -stride * phase_wave,
                    lift * max(0.0, phase_wave),
                )
            )
        )
        targets["RightFoot"] = tuple(
            right_rest
            + Vector(
                (
                    0.0,
                    stride * phase_wave,
                    lift * max(0.0, -phase_wave),
                )
            )
        )
        add_ik(armature, "LeftLeg", targets["LeftFoot"], targets["LeftKneePole"], 2, 90.0)
        add_ik(armature, "RightLeg", targets["RightFoot"], targets["RightKneePole"], 2, -90.0)
    use_compact_arm_ik = frame.weapon_pose == "aim"
    if not compact_standing or use_compact_arm_ik:
        arm_influence = 0.72 if compact_standing else 1.0
        add_ik(
            armature,
            "LeftForeArm",
            targets["LeftHand"],
            targets["LeftElbowPole"],
            2,
            90.0,
            arm_influence,
        )
        add_ik(
            armature,
            "RightForeArm",
            targets["RightHand"],
            targets["RightElbowPole"],
            2,
            -90.0,
            arm_influence,
        )
    if frame.reaction != "none":
        upper_spine = armature.pose.bones.get("Spine2")
        if upper_spine is not None:
            upper_spine.rotation_mode = "QUATERNION"
            upper_spine.rotation_quaternion = Quaternion(
                (1.0, 0.0, 0.0),
                radians(-7.0 if frame.reaction == "hit" else -3.0),
            )
    bpy.context.view_layer.update()
    depsgraph = bpy.context.evaluated_depsgraph_get()
    evaluated = armature.evaluated_get(depsgraph)
    return {bone.name: bone.matrix.copy() for bone in evaluated.pose.bones}


def apply_pose_frame(armature: bpy.types.Object, frame: recipe.PoseFrame, height_m: float) -> None:
    matrices = evaluated_pose_matrices(armature, frame, height_m)
    # A PoseBone.matrix copied from Blender's evaluated dependency-graph armature is not a safe
    # channel assignment: on this FBX it changed child transforms after the constraints vanished.
    # Blender's visual-transform operator is the supported constraint-to-channel bake and preserves
    # the exact evaluated armature-space matrices.
    bpy.ops.object.select_all(action="DESELECT")
    armature.select_set(True)
    bpy.context.view_layer.objects.active = armature
    bpy.ops.object.mode_set(mode="POSE")
    bpy.ops.pose.select_all(action="SELECT")
    result = bpy.ops.pose.visual_transform_apply()
    bpy.ops.object.mode_set(mode="OBJECT")
    if "FINISHED" not in result:
        raise RuntimeError(f"could not apply visual pose {frame.name}: {result}")
    for pose_bone in armature.pose.bones:
        for constraint in list(pose_bone.constraints):
            pose_bone.constraints.remove(constraint)
    remove_pose_targets()
    bpy.context.view_layer.update()
    maximum_matrix_error = max(
        abs(float(armature.pose.bones[bone_name].matrix[row][column]) - float(matrix[row][column]))
        for bone_name, matrix in matrices.items()
        for row in range(4)
        for column in range(4)
    )
    if maximum_matrix_error > 1.0e-5:
        raise RuntimeError(
            f"visual pose bake {frame.name} changed an evaluated bone matrix by "
            f"{maximum_matrix_error:.9f}"
        )


def apply_skeletal_post_transform(
    armature: bpy.types.Object,
    frame: recipe.PoseFrame,
    height_m: float,
) -> None:
    """Apply the static-frame death fall to the skeletal root and keep it above the floor."""

    if abs(frame.post_tilt_degrees) <= 1.0e-6:
        return
    hips = armature.pose.bones.get("Hips")
    if hips is None:
        raise RuntimeError("cannot apply skeletal death fall without Hips")
    pivot_height = 0.82 * (height_m / recipe.TARGET_HEIGHT_M)
    pivot = Vector((0.0, 0.0, pivot_height))
    transform = (
        Matrix.Translation(pivot)
        @ Matrix.Rotation(radians(frame.post_tilt_degrees), 4, "X")
        @ Matrix.Translation(-pivot)
    )
    hips.matrix = transform @ hips.matrix
    bpy.context.view_layer.update()
    source_meshes = [
        obj
        for obj in bpy.context.scene.objects
        if obj.type == "MESH" and bool(obj.get("kessel_source_mesh"))
    ]
    evaluated = world_vertex_points(source_meshes, evaluated=True)
    minimum_z = min(point.z for points in evaluated.values() for point in points)
    hips.matrix = Matrix.Translation((0.0, 0.0, -minimum_z)) @ hips.matrix
    bpy.context.view_layer.update()


def pose_channel_snapshot(armature: bpy.types.Object) -> Dict[str, Dict[str, Any]]:
    return {
        pose_bone.name: {
            "location": pose_bone.location.copy(),
            "rotation_quaternion": pose_bone.rotation_quaternion.copy(),
            "scale": pose_bone.scale.copy(),
        }
        for pose_bone in armature.pose.bones
    }


def restore_pose_channel_snapshot(
    armature: bpy.types.Object,
    snapshot: Dict[str, Dict[str, Any]],
) -> None:
    for bone_name, channels in snapshot.items():
        pose_bone = armature.pose.bones[bone_name]
        pose_bone.rotation_mode = "QUATERNION"
        pose_bone.location = channels["location"]
        pose_bone.rotation_quaternion = channels["rotation_quaternion"]
        pose_bone.scale = channels["scale"]


def create_skeletal_actions(armature: bpy.types.Object, height_m: float) -> List[str]:
    if armature.animation_data is None:
        armature.animation_data_create()
    bpy.context.scene.render.fps = ACTION_FPS
    bpy.context.scene.render.fps_base = 1.0
    bpy.context.scene.frame_start = 0
    bpy.context.preferences.edit.keyframe_new_interpolation_type = "LINEAR"
    created = []
    for sequence_name, sequence in recipe.ANIMATION_SEQUENCES.items():
        action_name = f"{recipe.SKELETAL_ACTION_PREFIX}{sequence_name}"
        action = bpy.data.actions.new(action_name)
        action.use_fake_user = True
        timeline = 0.0
        snapshots = []
        for frame_name, hold_frames in sequence["frames"]:
            armature.animation_data.action = None
            apply_pose_frame(armature, recipe.pose_by_name(frame_name), height_m)
            apply_skeletal_post_transform(
                armature,
                recipe.pose_by_name(frame_name),
                height_m,
            )
            snapshots.append((timeline, pose_channel_snapshot(armature)))
            timeline += hold_frames
        if not snapshots:
            raise RuntimeError(f"skeletal action {action_name} has no pose snapshots")
        endpoint_snapshot = snapshots[0][1] if sequence["loop"] else snapshots[-1][1]
        snapshots.append((timeline, endpoint_snapshot))
        armature.animation_data.action = action
        for keyframe, snapshot in snapshots:
            restore_pose_channel_snapshot(armature, snapshot)
            for pose_bone in armature.pose.bones:
                pose_bone.keyframe_insert("location", frame=keyframe, group=pose_bone.name)
                pose_bone.keyframe_insert(
                    "rotation_quaternion", frame=keyframe, group=pose_bone.name
                )
                pose_bone.keyframe_insert("scale", frame=keyframe, group=pose_bone.name)
        bpy.context.scene.frame_end = max(
            bpy.context.scene.frame_end,
            math.ceil(timeline),
        )
        created.append(action.name)
        armature.animation_data.action = None
        reset_pose(armature)
    return created


def set_fractional_scene_frame(frame: float) -> None:
    whole = math.floor(frame)
    bpy.context.scene.frame_set(whole, subframe=float(frame - whole))


def validate_skeletal_actions(armature: bpy.types.Object) -> Dict[str, Any]:
    """Verify authored clip timing plus the death reaction/fall on evaluated pose bones."""

    reports = {}
    expected_durations = recipe.expected_action_durations_s()
    for action_name, expected_duration_s in expected_durations.items():
        action = bpy.data.actions.get(action_name)
        if action is None:
            raise RuntimeError(f"missing authored skeletal action {action_name!r}")
        start_frame, end_frame = (float(value) for value in action.frame_range)
        expected_end_frame = expected_duration_s * ACTION_FPS
        if abs(start_frame) > 1.0e-5 or abs(end_frame - expected_end_frame) > 1.0e-4:
            raise RuntimeError(
                f"skeletal action {action_name!r} frame range "
                f"[{start_frame:.6f}, {end_frame:.6f}] does not match "
                f"[0, {expected_end_frame:.6f}]"
            )
        armature.animation_data.action = action
        set_fractional_scene_frame(start_frame)
        bpy.context.view_layer.update()
        hips_start = armature.pose.bones["Hips"].matrix.to_quaternion()
        spine_start = armature.pose.bones["Spine2"].rotation_quaternion.copy()
        start_matrices = {
            bone.name: bone.matrix.copy() for bone in armature.pose.bones
        }
        set_fractional_scene_frame(end_frame)
        bpy.context.view_layer.update()
        hips_end = armature.pose.bones["Hips"].matrix.to_quaternion()
        end_matrices = {bone.name: bone.matrix.copy() for bone in armature.pose.bones}
        raw_rotation_degrees = math.degrees(
            hips_start.rotation_difference(hips_end).angle
        )
        hips_rotation_degrees = min(
            raw_rotation_degrees,
            360.0 - raw_rotation_degrees,
        )
        spine_start_degrees = math.degrees(spine_start.angle)
        loop_closure_matrix_error = max(
            abs(
                float(start_matrices[bone_name][row][column])
                - float(end_matrices[bone_name][row][column])
            )
            for bone_name in start_matrices
            for row in range(4)
            for column in range(4)
        )
        reports[action_name] = {
            "start_frame": round(start_frame, 6),
            "end_frame": round(end_frame, 6),
            "duration_s": round((end_frame - start_frame) / ACTION_FPS, 9),
            "hips_start_to_end_rotation_degrees": round(hips_rotation_degrees, 6),
            "spine_start_channel_rotation_degrees": round(spine_start_degrees, 6),
            "start_to_end_matrix_error": round(loop_closure_matrix_error, 9),
        }
        sequence_name = action_name.removeprefix(recipe.SKELETAL_ACTION_PREFIX)
        if (
            recipe.ANIMATION_SEQUENCES[sequence_name]["loop"]
            and loop_closure_matrix_error > 1.0e-5
        ):
            raise RuntimeError(
                f"looping skeletal action {action_name!r} does not close: "
                f"matrix error {loop_closure_matrix_error:.9f}"
            )
    death_name = f"{recipe.SKELETAL_ACTION_PREFIX}death"
    death = reports[death_name]
    if death["hips_start_to_end_rotation_degrees"] < 80.0:
        raise RuntimeError(
            "skeletal death action did not retain the authored root fall: "
            f"{death['hips_start_to_end_rotation_degrees']:.6f} degrees"
        )
    if death["spine_start_channel_rotation_degrees"] < 2.0:
        raise RuntimeError(
            "skeletal death action did not retain the authored Spine2 hit reaction"
        )
    armature.animation_data.action = None
    set_fractional_scene_frame(0.0)
    reset_pose(armature)
    return reports


def supported_operator_keywords(operator, keywords: Dict[str, Any]) -> Dict[str, Any]:
    available = {property_.identifier for property_ in operator.get_rna_type().properties}
    return {name: value for name, value in keywords.items() if name in available}


def export_glb(path: Path, *, skins: bool, animations: bool) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    keywords = {
        "filepath": str(path),
        "export_format": "GLB",
        "use_selection": False,
        "export_apply": not skins,
        "export_texcoords": True,
        "export_normals": True,
        "export_materials": "EXPORT",
        "export_image_format": "AUTO",
        "export_extras": True,
        "export_skins": skins,
        "export_all_influences": False,
        "export_animations": animations,
        "export_animation_mode": "ACTIONS",
        "export_force_sampling": True,
        "export_yup": True,
    }
    result = bpy.ops.export_scene.gltf(
        **supported_operator_keywords(bpy.ops.export_scene.gltf, keywords)
    )
    if "FINISHED" not in result or not path.is_file():
        raise RuntimeError(f"GLB export failed for {path}: {result}")


def bake_evaluated_static_meshes(exclude_generated_rifle: bool) -> List[bpy.types.Object]:
    source_meshes = [
        obj
        for obj in bpy.context.scene.objects
        if obj.type == "MESH"
        and bool(obj.get("kessel_source_mesh"))
        and not (
            exclude_generated_rifle
            and (
                obj.get("kessel_generated_rifle")
                or obj.name.startswith("SovietRifle_Generated")
            )
        )
    ]
    if not source_meshes:
        raise RuntimeError("pose contained no source meshes to bake")
    depsgraph = bpy.context.evaluated_depsgraph_get()
    baked: List[bpy.types.Object] = []
    for source_obj in source_meshes:
        evaluated = source_obj.evaluated_get(depsgraph)
        mesh = bpy.data.meshes.new_from_object(
            evaluated,
            preserve_all_data_layers=True,
            depsgraph=depsgraph,
        )
        mesh.transform(evaluated.matrix_world)
        mesh.update()
        obj = bpy.data.objects.new(f"{source_obj.name}_Static", mesh)
        obj["kessel_static_pose"] = True
        bpy.context.scene.collection.objects.link(obj)
        baked.append(obj)
    for obj in list(bpy.context.scene.objects):
        if obj not in baked:
            bpy.data.objects.remove(obj, do_unlink=True)
    bpy.context.view_layer.update()
    return baked


def transform_static_vertices(meshes: Sequence[bpy.types.Object], transform: Matrix) -> None:
    for obj in meshes:
        for vertex in obj.data.vertices:
            vertex.co = transform @ vertex.co
        obj.data.update()


def tilt_and_ground_static(
    meshes: Sequence[bpy.types.Object],
    degrees: float,
    *,
    height_m: float,
) -> None:
    if abs(degrees) > 1.0e-6:
        pivot = Vector(
            (0.0, 0.0, 0.82 * (height_m / recipe.TARGET_HEIGHT_M))
        )
        transform = (
            Matrix.Translation(pivot)
            @ Matrix.Rotation(radians(degrees), 4, "X")
            @ Matrix.Translation(-pivot)
        )
        transform_static_vertices(meshes, transform)
    minimum, _ = exact_world_bounds(meshes)
    transform_static_vertices(meshes, Matrix.Translation((0.0, 0.0, -minimum.z)))
    bpy.context.view_layer.update()


def validate_pose_extent(
    frame: recipe.PoseFrame,
    minimum: Vector,
    maximum: Vector,
    *,
    height_m: float,
) -> None:
    dimensions = maximum - minimum
    if abs(minimum.z) > 1.0e-5:
        raise RuntimeError(f"pose {frame.name} is not grounded: minimum Z={minimum.z}")
    if frame.post_tilt_degrees < 1.0:
        limits = Vector((height_m * 1.20, height_m * 1.35, height_m * 1.45))
    else:
        limits = Vector((height_m * 1.45, height_m * 1.90, height_m * 1.55))
    failures = [
        (axis, dimensions[index], limits[index])
        for index, axis in enumerate(("X", "Y", "Z"))
        if dimensions[index] > limits[index]
    ]
    if failures:
        formatted = ", ".join(
            f"{axis}={actual:.3f} m > {limit:.3f} m"
            for axis, actual, limit in failures
        )
        raise RuntimeError(
            f"pose {frame.name} exceeds humanoid extent guard ({formatted}); source rest pose "
            "and selected rigging mode are probably incompatible"
        )


def write_json(path: Path, data: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(recipe.stable_json(data), encoding="utf-8")


def build_rigged_master(
    source_glb: Path,
    donor_fbx: Path,
    output_path: Path,
    *,
    source_forward: str,
    height_m: float,
    rigging_mode: str,
) -> Dict[str, Any]:
    clear_scene()
    before_images = set(bpy.data.images)
    source_objects = imported_objects(bpy.ops.import_scene.gltf, filepath=str(source_glb))
    source_images = [image for image in bpy.data.images if image not in before_images]
    source_meshes = validate_source_import(source_objects, source_images)
    source_mesh_record = mesh_snapshot(source_meshes)
    source_mesh_fingerprint = mesh_authoring_fingerprint(source_meshes)
    source_image_record = image_snapshot(source_images)
    pack_source_images(source_images)
    source_root = align_object_set(
        source_objects,
        name="SovietSource_Normalization",
        height_m=height_m,
        rotation_z=source_forward_rotation(source_forward),
    )
    posture = posture_from_bounds(source_meshes)
    before_rigging = world_vertex_points(source_meshes)

    donor_objects = imported_objects(
        bpy.ops.import_scene.fbx,
        filepath=str(donor_fbx),
        use_anim=False,
        use_custom_normals=True,
        use_image_search=False,
        automatic_bone_orientation=False,
    )
    donor_meshes = [obj for obj in donor_objects if obj.type == "MESH"]
    donor_armatures = [obj for obj in donor_objects if obj.type == "ARMATURE"]
    if len(donor_armatures) != 1 or not donor_meshes:
        raise RuntimeError(
            f"expected one donor armature and at least one mesh; got "
            f"{len(donor_armatures)} armatures/{len(donor_meshes)} meshes"
        )
    armature = donor_armatures[0]
    missing_bones = set(recipe.REQUIRED_DONOR_BONES) - {bone.name for bone in armature.data.bones}
    if missing_bones:
        raise RuntimeError(f"donor armature is missing required bones: {sorted(missing_bones)}")
    donor_root = align_object_set(
        donor_objects,
        name="GermanDonor_Normalization",
        height_m=height_m,
        rotation_z=0.0,
    )
    for obj in donor_meshes + [armature]:
        detach_keep_world(obj)
    apply_object_world_transforms(donor_meshes + [armature])
    bpy.data.objects.remove(donor_root, do_unlink=True)
    armature.name = "SovietInfantry_Rig"
    armature.data.name = "SovietInfantry_49BoneRig"
    # FBX files may carry stored pose-channel transforms even when use_anim=False.  Weight transfer
    # and bind validation must happen in the armature's declared rest pose; otherwise the modifier
    # can visibly deform the source before a single authored action is applied.
    armature.data.pose_position = "REST"
    reset_pose(armature)
    selected_mode = rigging_mode
    if selected_mode == "auto":
        selected_mode = posture["recommended_rigging_mode"]
    deform_bones = configure_deform_bones(
        armature,
        retain_all_donor_weights=selected_mode == "donor-transfer",
    )
    refit_report = None
    if selected_mode == "a-pose":
        armature.data.pose_position = "REST"
        refit_report = refit_a_pose_armature(armature, height_m=height_m)
        armature["kessel_compact_standing"] = False
        weight_reports = automatic_compact_weights(
            source_meshes,
            armature,
            height_m=height_m,
        )
        weighting_method = (
            "separated A-pose donor-joint refit plus direct source bone heat where valid; a "
            "disposable voxel heat cage and deterministic nearest-bone fallbacks handle "
            "layered/non-manifold sources; four influences maximum"
        )
    elif selected_mode == "compact-standing":
        compact_donor_mesh = max(donor_meshes, key=lambda obj: len(obj.data.vertices))
        posed_donor_proxy, posed_donor_report = build_compact_donor_weight_proxy(
            compact_donor_mesh,
            armature,
            height_m=height_m,
        )
        armature.data.pose_position = "REST"
        refit_report = refit_compact_standing_armature(armature, height_m=height_m)
        armature["kessel_compact_standing"] = True
        weight_reports = automatic_compact_weights(
            source_meshes,
            armature,
            height_m=height_m,
            posed_donor_proxy=posed_donor_proxy,
            posed_donor_report=posed_donor_report,
        )
        weighting_method = (
            "compact standing donor-joint refit plus direct source bone heat where valid; a "
            "disposable voxel heat cage, posed-donor surface, and deterministic nearest-bone "
            "fallbacks handle layered/non-manifold sources; four influences maximum"
        )
    else:
        armature["kessel_compact_standing"] = False
        donor_mesh = max(donor_meshes, key=lambda obj: len(obj.data.vertices))
        weight_reports = transfer_skin_weights(source_meshes, donor_mesh, armature)
        weighting_method = (
            "nearest-polygon interpolated donor vertex-group transfer; deterministic "
            "nearest-bone segment fallback for uncovered vertices; four influences maximum"
        )
    for obj in donor_meshes:
        bpy.data.objects.remove(obj, do_unlink=True)
    if source_root.name not in bpy.context.scene.objects:
        raise RuntimeError("source normalization hierarchy was unexpectedly removed")

    build_rifle(armature=armature, weapon_pose="low_ready", recoil_m=0.0)
    bpy.context.view_layer.update()
    after_rigging = world_vertex_points(source_meshes, evaluated=True)
    rest_displacement = max_rest_displacement(before_rigging, after_rigging)
    if rest_displacement > 1.0e-5:
        raise RuntimeError(
            f"rigging changed source rest geometry by {rest_displacement:.9f} m; refusing export"
        )
    final_mesh_fingerprint = mesh_authoring_fingerprint(source_meshes)
    if final_mesh_fingerprint != source_mesh_fingerprint:
        raise RuntimeError(
            "rigging changed source topology, local positions, UV loops, or material slots; "
            "refusing export"
        )
    armature.data.pose_position = "POSE"
    reset_pose(armature)
    action_names = create_skeletal_actions(armature, height_m)
    expected_actions = list(recipe.expected_action_names())
    if action_names != expected_actions:
        raise RuntimeError(f"skeletal action order mismatch: {action_names}")
    skeletal_action_quality = validate_skeletal_actions(armature)
    export_glb(output_path, skins=True, animations=True)
    contract = recipe.validate_glb_contract(
        output_path,
        require_skin=True,
        require_animations=True,
        expected_animation_names=expected_actions,
        expected_animation_durations_s=recipe.expected_action_durations_s(),
    )
    minimum, maximum = exact_world_bounds(source_meshes)
    return {
        "source_meshes_before_rigging": source_mesh_record,
        "source_images": source_image_record,
        "normalization": {
            "source_forward": source_forward,
            "target_height_m": height_m,
            "root_transform_only_before_skinning": True,
            "source_mesh_authoring_sha256_before": source_mesh_fingerprint,
            "source_mesh_authoring_sha256_after": final_mesh_fingerprint,
            "source_topology_positions_uvs_and_material_slots_unchanged": True,
            "rest_pose_max_displacement_m": rest_displacement,
            "bounds_min_m": round_vector(minimum),
            "bounds_max_m": round_vector(maximum),
        },
        "rigging": {
            "requested_mode": rigging_mode,
            "selected_mode": selected_mode,
            "source_posture": posture,
            "rest_pose_refit": refit_report,
            "donor_bone_count": len(armature.data.bones),
            "donor_bones": [bone.name for bone in armature.data.bones],
            "deforming_bones": deform_bones,
            "method": weighting_method,
            "mesh_reports": weight_reports,
            "generated_rifle": {
                "source": "Kessel procedural boxes plus an eight-sided barrel",
                "third_party_weapon_asset": False,
                "attachment": "100% RightHand skin weight in the rigged master",
            },
        },
        "skeletal_actions": action_names,
        "skeletal_action_quality": skeletal_action_quality,
        "glb_contract": contract,
    }


def bake_static_pose(
    rigged_path: Path,
    output_path: Path,
    frame: recipe.PoseFrame,
    *,
    height_m: float,
) -> Dict[str, Any]:
    clear_scene()
    imported_objects(bpy.ops.import_scene.gltf, filepath=str(rigged_path))
    armatures = [obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE"]
    if len(armatures) != 1:
        raise RuntimeError(f"rigged master reimport produced {len(armatures)} armatures")
    armature = armatures[0]
    if armature.animation_data is not None:
        armature.animation_data.action = None
        for track in armature.animation_data.nla_tracks:
            track.mute = True
    armature.data.pose_position = "POSE"
    reset_pose(armature)
    source_meshes = [
        obj
        for obj in bpy.context.scene.objects
        if obj.type == "MESH" and bool(obj.get("kessel_source_mesh"))
    ]
    if not source_meshes:
        raise RuntimeError("rigged master reimport lost source-mesh provenance markers")
    rest_snapshot = evaluated_deformation_snapshot(source_meshes)
    apply_pose_frame(armature, frame, height_m)
    posed_snapshot = evaluated_deformation_snapshot(source_meshes)
    deformation_report = deformation_quality_between(
        rest_snapshot,
        posed_snapshot,
        height_m=height_m,
    )
    if not deformation_report["passed"]:
        failures = "; ".join(deformation_report["violations"])
        raise RuntimeError(
            f"pose {frame.name} failed local deformation validation ({failures}); refusing to "
            "export a torn or spiked character"
        )
    meshes = bake_evaluated_static_meshes(exclude_generated_rifle=True)
    rifle = build_rifle(
        armature=None,
        weapon_pose=frame.weapon_pose,
        recoil_m=frame.recoil_m,
    )
    meshes.append(rifle)
    tilt_and_ground_static(
        meshes,
        frame.post_tilt_degrees,
        height_m=height_m,
    )
    minimum, maximum = exact_world_bounds(meshes)
    validate_pose_extent(frame, minimum, maximum, height_m=height_m)
    export_glb(output_path, skins=False, animations=False)
    contract = recipe.validate_glb_contract(
        output_path,
        require_skin=False,
        require_animations=False,
    )
    return {
        "pose": frame.name,
        "post_tilt_degrees": frame.post_tilt_degrees,
        "bounds_min_m": round_vector(minimum),
        "bounds_max_m": round_vector(maximum),
        "dimensions_m": round_vector(maximum - minimum),
        "deformation_quality": deformation_report,
        "glb_contract": contract,
        **recipe.file_record(output_path, logical_path=f"baked/{output_path.name}"),
    }


def main() -> None:
    arguments = parse_arguments()
    recipe.validate_recipe()
    asset_root = arguments.output_directory
    source_directory = asset_root / "source"
    authoring_directory = asset_root / "authoring"
    baked_directory = asset_root / "baked"
    source_directory.mkdir(parents=True, exist_ok=True)
    authoring_directory.mkdir(parents=True, exist_ok=True)
    baked_directory.mkdir(parents=True, exist_ok=True)
    # A failed rebake must not leave a prior receipt that falsely certifies the new source/output
    # combination. Generated GLBs may remain useful for diagnosis, but release policy requires this
    # report, so remove it before any mutable bake work and write it only after every frame passes.
    report_path = asset_root / "bake.report.json"
    if report_path.is_file():
        report_path.unlink()

    preserved_source = source_directory / "ww2_soviet_soldier_original.glb"
    recipe.preserve_source_bytes(arguments.source_glb, preserved_source)
    attribution = recipe.attribution_record(
        preserved_source,
        logical_source_path="source/ww2_soviet_soldier_original.glb",
        retrieved=arguments.retrieved,
    )
    asset_record_path = asset_root / "asset.json"
    source_notice_path = asset_root / "SOURCE.md"
    license_path = asset_root / "LICENSE.txt"
    write_json(asset_record_path, attribution)
    source_notice_path.write_text(
        recipe.attribution_markdown(attribution), encoding="utf-8"
    )
    license_path.write_text(recipe.license_notice(), encoding="utf-8")

    manifest = recipe.animation_manifest()
    manifest_path = baked_directory / "animation_manifest.v1.json"
    write_json(manifest_path, manifest)

    rigged_path = authoring_directory / "soviet_ww2_soldier_rigged.glb"
    rigging_report = build_rigged_master(
        preserved_source,
        arguments.donor_fbx,
        rigged_path,
        source_forward=arguments.source_forward,
        height_m=arguments.height_m,
        rigging_mode=arguments.rigging_mode,
    )
    pose_reports = []
    for frame in recipe.POSE_FRAMES:
        pose_reports.append(
            bake_static_pose(
                rigged_path,
                baked_directory / f"soviet_ww2_soldier_{frame.name}.glb",
                frame,
                height_m=arguments.height_m,
            )
        )

    donor_texture = arguments.donor_fbx.with_name("LowpolySoldier_Texture.png")
    donor_record: Dict[str, Any] = recipe.file_record(
        arguments.donor_fbx,
        logical_path=str(arguments.donor_fbx),
    )
    if donor_texture.is_file():
        donor_record["sibling_texture"] = recipe.file_record(
            donor_texture,
            logical_path=str(donor_texture),
        )
    report = {
        "schema": recipe.BAKE_SCHEMA,
        "version": 1,
        "asset": attribution,
        "bake": {
            "blender_version": bpy.app.version_string,
            "script": Path(__file__).name,
            "script_sha256": recipe.sha256_file(Path(__file__).resolve()),
            "recipe_module": "soviet_character_recipe.py",
            "recipe_sha256": recipe.sha256_file(
                Path(recipe.__file__).resolve()
            ),
            "completed_utc": datetime.now(timezone.utc).isoformat(),
        },
        "sources": {
            "preserved_soviet_glb": recipe.file_record(
                preserved_source,
                logical_path="source/ww2_soviet_soldier_original.glb",
            ),
            "german_rig_donor": donor_record,
        },
        "metadata_files": [
            recipe.file_record(asset_record_path, logical_path="asset.json"),
            recipe.file_record(source_notice_path, logical_path="SOURCE.md"),
            recipe.file_record(license_path, logical_path="LICENSE.txt"),
        ],
        "scope": {
            "preserved_source_is_byte_exact": True,
            "rigged_output_reexports_source_mesh_material_and_atlas": True,
            "rigged_output_texture_bytes_may_be_repacked_by_blender": True,
            "static_frames_are_runtime_animation_authority": True,
            "engine_skeletal_playback_claimed": False,
            "source_identity_verification": (
                "operator attribution plus recorded local bytes; no authentication or remote "
                "identity inference is performed by this offline script"
            ),
        },
        "rigged_master": {
            **recipe.file_record(
                rigged_path,
                logical_path="authoring/soviet_ww2_soldier_rigged.glb",
            ),
            **rigging_report,
        },
        "animation_manifest": recipe.file_record(
            manifest_path,
            logical_path="baked/animation_manifest.v1.json",
        ),
        "static_poses": pose_reports,
    }
    recipe.validate_hash_records(report)
    report["release_verification"] = recipe.verify_bake_receipt(asset_root, report)
    write_json(report_path, report)
    print(recipe.stable_json(report))


if __name__ == "__main__":
    main()
