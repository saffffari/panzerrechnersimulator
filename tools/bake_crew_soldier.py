#!/usr/bin/env python3
"""Convert the CC0 LowpolySoldier FBX into a self-contained rigged GLB.

Run with Blender, not the system Python::

    blender --background --factory-startup \
      --python tools/bake_crew_soldier.py -- \
      assets/flavor/Characters/german-ww2-soldier-cc0/source/LowpolySoldier.fbx \
      assets/flavor/Characters/german-ww2-soldier-cc0/baked/german_ww2_soldier_rigged.glb \
      assets/flavor/Characters/german-ww2-soldier-cc0/baked/german_ww2_soldier_rigged.report.json

The source skeleton and skin weights are deliberately retained.  The report is an authoring
receipt: it records mesh counts, scene bounds, bones, actions, source hashes, and the Blender
version used for the conversion.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sys

import bpy
from mathutils import Vector


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def parse_arguments() -> tuple[Path, Path, Path]:
    try:
        separator = sys.argv.index("--")
    except ValueError as error:
        raise SystemExit("expected: -- <source.fbx> <output.glb> <report.json>") from error
    arguments = [Path(value).expanduser().resolve() for value in sys.argv[separator + 1 :]]
    if len(arguments) != 3:
        raise SystemExit("expected: -- <source.fbx> <output.glb> <report.json>")
    source, output, report = arguments
    if not source.is_file() or source.suffix.casefold() != ".fbx":
        raise SystemExit(f"FBX source does not exist: {source}")
    return source, output, report


def clear_scene() -> None:
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for datablocks in (
        bpy.data.meshes,
        bpy.data.armatures,
        bpy.data.materials,
        bpy.data.images,
        bpy.data.actions,
    ):
        for datablock in list(datablocks):
            if datablock.users == 0:
                datablocks.remove(datablock)


def relink_missing_images(source: Path) -> None:
    """Keep the conversion deterministic when the FBX stores an obsolete absolute texture path."""
    texture = source.with_name("LowpolySoldier_Texture.png")
    if not texture.is_file():
        return
    replacement = None
    for image in bpy.data.images:
        if image.source != "FILE":
            continue
        resolved = Path(bpy.path.abspath(image.filepath))
        if image.name == texture.name or not resolved.is_file():
            replacement = image
            break
    if replacement is None:
        return
    replacement.filepath = str(texture)
    replacement.reload()


def ensure_texture_material(source: Path) -> None:
    """The original FBX carries UVs but no bound material; build the missing albedo material."""
    texture = source.with_name("LowpolySoldier_Texture.png")
    if not texture.is_file():
        raise RuntimeError(f"soldier texture is missing: {texture}")
    image = next((candidate for candidate in bpy.data.images if candidate.name == texture.name), None)
    if image is None:
        image = bpy.data.images.load(str(texture), check_existing=True)
    image.colorspace_settings.name = "sRGB"

    material = bpy.data.materials.get("LowpolySoldier_CC0")
    if material is None:
        material = bpy.data.materials.new("LowpolySoldier_CC0")
    material.use_nodes = True
    material.diffuse_color = (1.0, 1.0, 1.0, 1.0)
    material.metallic = 0.0
    material.roughness = 0.9
    nodes = material.node_tree.nodes
    nodes.clear()
    output = nodes.new("ShaderNodeOutputMaterial")
    shader = nodes.new("ShaderNodeBsdfPrincipled")
    shader.inputs["Metallic"].default_value = 0.0
    shader.inputs["Roughness"].default_value = 0.9
    albedo = nodes.new("ShaderNodeTexImage")
    albedo.name = "LowpolySoldier_Texture"
    albedo.image = image
    albedo.interpolation = "Closest"
    material.node_tree.links.new(albedo.outputs["Color"], shader.inputs["Base Color"])
    material.node_tree.links.new(shader.outputs["BSDF"], output.inputs["Surface"])

    for obj in bpy.context.scene.objects:
        if obj.type != "MESH":
            continue
        if not obj.data.uv_layers:
            raise RuntimeError(f"mesh {obj.name!r} has no UV layer for the source texture")
        obj.data.materials.clear()
        obj.data.materials.append(material)


def world_bounds(mesh_objects: list[bpy.types.Object]) -> tuple[list[float], list[float]]:
    points = [obj.matrix_world @ Vector(corner) for obj in mesh_objects for corner in obj.bound_box]
    if not points:
        raise RuntimeError("FBX import contained no mesh bounds")
    minimum = [min(point[axis] for point in points) for axis in range(3)]
    maximum = [max(point[axis] for point in points) for axis in range(3)]
    return minimum, maximum


def round_vec(values) -> list[float]:
    return [round(float(value), 6) for value in values]


def build_report(source: Path) -> dict:
    meshes = sorted((obj for obj in bpy.context.scene.objects if obj.type == "MESH"), key=lambda obj: obj.name)
    armatures = sorted(
        (obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE"), key=lambda obj: obj.name
    )
    minimum, maximum = world_bounds(meshes)
    dimensions = [maximum[index] - minimum[index] for index in range(3)]
    return {
        "schema": "kessel.crew-source-conversion",
        "version": 1,
        "source": {
            "filename": source.name,
            "sha256": sha256(source),
            "texture": source.with_name("LowpolySoldier_Texture.png").name,
            "texture_sha256": sha256(source.with_name("LowpolySoldier_Texture.png")),
        },
        "converter": {
            "name": "Blender",
            "version": bpy.app.version_string,
            "gltf_exporter": "io_scene_gltf2",
        },
        "scene": {
            "object_count": len(bpy.context.scene.objects),
            "mesh_count": len(meshes),
            "armature_count": len(armatures),
            "material_count": len(bpy.data.materials),
            "image_count": len(bpy.data.images),
            "bounds_min_m": round_vec(minimum),
            "bounds_max_m": round_vec(maximum),
            "dimensions_m": round_vec(dimensions),
        },
        "meshes": [
            {
                "object": obj.name,
                "mesh": obj.data.name,
                "vertices": len(obj.data.vertices),
                "edges": len(obj.data.edges),
                "polygons": len(obj.data.polygons),
                "triangles": sum(max(1, len(polygon.vertices) - 2) for polygon in obj.data.polygons),
                "materials": [slot.material.name if slot.material else None for slot in obj.material_slots],
                "armature_modifiers": [
                    modifier.object.name if modifier.object else None
                    for modifier in obj.modifiers
                    if modifier.type == "ARMATURE"
                ],
                "vertex_groups": len(obj.vertex_groups),
            }
            for obj in meshes
        ],
        "armatures": [
            {
                "object": obj.name,
                "armature": obj.data.name,
                "bone_count": len(obj.data.bones),
                "bones": [bone.name for bone in obj.data.bones],
            }
            for obj in armatures
        ],
        "actions": [
            {
                "name": action.name,
                "frame_range": round_vec(action.frame_range),
                "slots": len(action.slots),
            }
            for action in sorted(bpy.data.actions, key=lambda item: item.name)
        ],
    }


def main() -> None:
    source, output, report_path = parse_arguments()
    output.parent.mkdir(parents=True, exist_ok=True)
    report_path.parent.mkdir(parents=True, exist_ok=True)

    clear_scene()
    result = bpy.ops.import_scene.fbx(
        filepath=str(source),
        use_anim=True,
        use_custom_normals=True,
        use_image_search=True,
        automatic_bone_orientation=False,
    )
    if "FINISHED" not in result:
        raise RuntimeError(f"FBX import failed: {result}")
    relink_missing_images(source)
    ensure_texture_material(source)

    mesh_objects = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    armature_objects = [obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE"]
    if not mesh_objects:
        raise RuntimeError("FBX import did not produce a mesh")
    if not armature_objects:
        raise RuntimeError("FBX import did not produce an armature; refusing a rigless conversion")
    if not any(
        modifier.type == "ARMATURE" and modifier.object in armature_objects
        for obj in mesh_objects
        for modifier in obj.modifiers
    ):
        raise RuntimeError("FBX mesh is not bound to its armature; refusing an unskinned conversion")

    for image in bpy.data.images:
        if image.source == "FILE" and Path(bpy.path.abspath(image.filepath)).is_file():
            image.pack()

    result = bpy.ops.export_scene.gltf(
        filepath=str(output),
        export_format="GLB",
        use_selection=False,
        export_apply=False,
        export_texcoords=True,
        export_normals=True,
        export_materials="EXPORT",
        export_image_format="AUTO",
        export_skins=True,
        export_all_influences=True,
        export_animations=True,
        export_yup=True,
    )
    if "FINISHED" not in result or not output.is_file():
        raise RuntimeError(f"GLB export failed: {result}")

    report = build_report(source)
    report["output"] = {
        "filename": output.name,
        "bytes": output.stat().st_size,
        "sha256": sha256(output),
        "self_contained": True,
        "rig_and_skin_retained": True,
    }
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
