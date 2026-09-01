#!/usr/bin/env python3
"""Bake engine-friendly static poses from the rigged CC0 LowpolySoldier source.

The engine's present GLB path consumes static geometry, so these variants complement (and do not
replace) ``german_ww2_soldier_rigged.glb``.  They are evaluated from the original skin and armature
inside Blender, then exported with the deformed vertex positions baked into ordinary meshes.

Run with Blender::

    blender --background --factory-startup \
      --python tools/bake_crew_soldier_variants.py -- \
      assets/flavor/Characters/german-ww2-soldier-cc0/source/LowpolySoldier.fbx \
      assets/flavor/Characters/german-ww2-soldier-cc0/baked

Outputs are authored at 1.53 m, Blender Z-up/glTF Y-up, standing on local Y=0 in glTF and facing
glTF +Z.  The relaxed pose remains upright so runtime code can apply its casualty rotation.
"""

from __future__ import annotations

import json
from math import radians
from pathlib import Path
import sys

import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import bake_crew_soldier as source_bake  # noqa: E402


def parse_arguments() -> tuple[Path, Path]:
    try:
        separator = sys.argv.index("--")
    except ValueError as error:
        raise SystemExit("expected: -- <source.fbx> <output-directory>") from error
    arguments = [Path(value).expanduser().resolve() for value in sys.argv[separator + 1 :]]
    if len(arguments) != 2:
        raise SystemExit("expected: -- <source.fbx> <output-directory>")
    source, output = arguments
    if not source.is_file():
        raise SystemExit(f"FBX source does not exist: {source}")
    output.mkdir(parents=True, exist_ok=True)
    return source, output


def import_character(source: Path) -> bpy.types.Object:
    source_bake.clear_scene()
    result = bpy.ops.import_scene.fbx(
        filepath=str(source),
        use_anim=False,
        use_custom_normals=True,
        use_image_search=True,
        automatic_bone_orientation=False,
    )
    if "FINISHED" not in result:
        raise RuntimeError(f"FBX import failed: {result}")
    source_bake.relink_missing_images(source)
    source_bake.ensure_texture_material(source)
    armatures = [obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE"]
    if len(armatures) != 1:
        raise RuntimeError(f"expected one armature, found {len(armatures)}")
    for image in bpy.data.images:
        if image.source == "FILE" and Path(bpy.path.abspath(image.filepath)).is_file():
            image.pack()
    return armatures[0]


def target(name: str, location: tuple[float, float, float]) -> bpy.types.Object:
    obj = bpy.data.objects.new(name, None)
    obj.empty_display_type = "PLAIN_AXES"
    obj.location = location
    bpy.context.scene.collection.objects.link(obj)
    return obj


def add_limb_ik(
    armature: bpy.types.Object,
    end_bone: str,
    target_location: tuple[float, float, float],
    pole_location: tuple[float, float, float],
    pole_angle_degrees: float = 0.0,
) -> None:
    pose_bone = armature.pose.bones[end_bone]
    goal = target(f"{end_bone}_goal", target_location)
    pole = target(f"{end_bone}_pole", pole_location)
    constraint = pose_bone.constraints.new("IK")
    constraint.name = f"Kessel_{end_bone}_IK"
    constraint.target = goal
    constraint.pole_target = pole
    constraint.chain_count = 2
    constraint.use_tail = True
    constraint.pole_angle = radians(pole_angle_degrees)


def pose_seated(armature: bpy.types.Object) -> None:
    # Lower the pelvis into a compact Tiger seat, knees/feet forward toward the front (-Y).
    armature.location.z -= 0.15
    add_limb_ik(armature, "LeftLeg", (0.14, -0.38, 0.055), (0.16, -0.78, 0.62), 90.0)
    add_limb_ik(armature, "RightLeg", (-0.14, -0.38, 0.055), (-0.16, -0.78, 0.62), -90.0)

    # Hands rest on two nearby controls rather than retaining the FBX's wide T-pose.
    add_limb_ik(armature, "LeftForeArm", (0.27, -0.24, 1.03), (0.56, -0.18, 1.20), 90.0)
    add_limb_ik(armature, "RightForeArm", (-0.27, -0.24, 1.03), (-0.56, -0.18, 1.20), -90.0)
    bpy.context.view_layer.update()


def pose_relaxed(armature: bpy.types.Object) -> None:
    # Upright casualty source with arms close to the body and a little asymmetry.  Runtime can
    # rotate this around +X into a face-down pose without exposing a wide T-pose silhouette.
    add_limb_ik(armature, "LeftForeArm", (0.24, -0.05, 1.02), (0.50, -0.17, 1.22), 90.0)
    add_limb_ik(armature, "RightForeArm", (-0.22, 0.01, 0.97), (-0.49, -0.12, 1.16), -90.0)
    bpy.context.view_layer.update()


def bake_skin_to_static() -> list[bpy.types.Object]:
    source_meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    if not source_meshes:
        raise RuntimeError("pose produced no meshes")
    depsgraph = bpy.context.evaluated_depsgraph_get()
    baked_meshes: list[bpy.types.Object] = []
    for source_obj in source_meshes:
        evaluated = source_obj.evaluated_get(depsgraph)
        mesh = bpy.data.meshes.new_from_object(
            evaluated,
            preserve_all_data_layers=True,
            depsgraph=depsgraph,
        )
        # Evaluated mesh coordinates are object-local; bake the complete FBX/armature transform so
        # the static output stays in meters rather than inheriting the FBX's centimeter scale.
        mesh.transform(evaluated.matrix_world)
        mesh.update()
        obj = bpy.data.objects.new(f"{source_obj.name}_Static", mesh)
        obj.vertex_groups.clear()
        bpy.context.scene.collection.objects.link(obj)
        baked_meshes.append(obj)

    for obj in list(bpy.context.scene.objects):
        if obj not in baked_meshes:
            bpy.data.objects.remove(obj, do_unlink=True)
    bpy.context.view_layer.update()
    return baked_meshes


def transform_vertices(meshes: list[bpy.types.Object], transform: Matrix) -> None:
    for obj in meshes:
        for vertex in obj.data.vertices:
            vertex.co = transform @ vertex.co
        obj.data.update()


def exact_vertex_bounds(meshes: list[bpy.types.Object]) -> tuple[list[float], list[float]]:
    points = [obj.matrix_world @ vertex.co for obj in meshes for vertex in obj.data.vertices]
    if not points:
        raise RuntimeError("static pose contains no vertices")
    minimum = [min(point[axis] for point in points) for axis in range(3)]
    maximum = [max(point[axis] for point in points) for axis in range(3)]
    return minimum, maximum


def ground_and_center(meshes: list[bpy.types.Object]) -> None:
    minimum, maximum = exact_vertex_bounds(meshes)
    offset = Vector((-(minimum[0] + maximum[0]) * 0.5, 0.0, -minimum[2]))
    transform_vertices(meshes, Matrix.Translation(offset))
    bpy.context.view_layer.update()


def normalize_height(meshes: list[bpy.types.Object], target_height_m: float = 1.53) -> None:
    minimum, maximum = exact_vertex_bounds(meshes)
    height = maximum[2] - minimum[2]
    if height <= 0.0:
        raise RuntimeError("static pose has no positive authored height")
    transform_vertices(meshes, Matrix.Scale(target_height_m / height, 4))
    ground_and_center(meshes)


def export_static(source: Path, output: Path, variant: str) -> None:
    result = bpy.ops.export_scene.gltf(
        filepath=str(output),
        export_format="GLB",
        use_selection=False,
        export_apply=True,
        export_texcoords=True,
        export_normals=True,
        export_materials="EXPORT",
        export_image_format="AUTO",
        export_skins=False,
        export_animations=False,
        export_yup=True,
    )
    if "FINISHED" not in result or not output.is_file():
        raise RuntimeError(f"{variant} GLB export failed: {result}")
    report = source_bake.build_report(source)
    minimum, maximum = exact_vertex_bounds(
        [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    )
    dimensions = [maximum[index] - minimum[index] for index in range(3)]
    report["scene"]["bounds_min_m"] = source_bake.round_vec(minimum)
    report["scene"]["bounds_max_m"] = source_bake.round_vec(maximum)
    report["scene"]["dimensions_m"] = source_bake.round_vec(dimensions)
    report["pose"] = {
        "name": variant,
        "geometry": "evaluated armature deformation baked to static vertices",
        "engine_skinning_required": False,
        "gltf_up_axis": "+Y",
        "gltf_forward_axis": "+Z",
        "gltf_floor_axis_value": {"axis": "Y", "value_m": 0.0},
        "authored_height_m": 1.53,
    }
    report["output"] = {
        "filename": output.name,
        "bytes": output.stat().st_size,
        "sha256": source_bake.sha256(output),
        "self_contained": True,
        "rig_and_skin_retained": False,
        "static_pose_companion_to_rigged_source": True,
    }
    report_path = output.with_suffix(".report.json")
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2, sort_keys=True))


def bake_variant(source: Path, output_directory: Path, variant: str) -> None:
    armature = import_character(source)
    if variant == "seated_static":
        pose_seated(armature)
    elif variant == "relaxed_static":
        pose_relaxed(armature)
    else:
        raise RuntimeError(f"unknown static crew variant: {variant}")
    meshes = bake_skin_to_static()
    ground_and_center(meshes)
    normalize_height(meshes)
    export_static(source, output_directory / f"german_ww2_soldier_{variant}.glb", variant)


def main() -> None:
    source, output_directory = parse_arguments()
    for variant in ("seated_static", "relaxed_static"):
        bake_variant(source, output_directory, variant)


if __name__ == "__main__":
    main()
