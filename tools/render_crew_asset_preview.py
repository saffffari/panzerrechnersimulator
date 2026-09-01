#!/usr/bin/env python3
"""Render a neutral asset-QA preview of a crew GLB with Blender Eevee."""

from __future__ import annotations

from pathlib import Path
import sys

import bpy
from mathutils import Vector


def arguments() -> tuple[Path, Path]:
    try:
        separator = sys.argv.index("--")
    except ValueError as error:
        raise SystemExit("expected: -- <input.glb> <output.png>") from error
    values = [Path(value).expanduser().resolve() for value in sys.argv[separator + 1 :]]
    if len(values) != 2 or not values[0].is_file():
        raise SystemExit("expected: -- <existing-input.glb> <output.png>")
    return values[0], values[1]


def look_at(obj: bpy.types.Object, point: Vector) -> None:
    obj.rotation_euler = (point - obj.location).to_track_quat("-Z", "Y").to_euler()


def add_area(name: str, location: Vector, target: Vector, energy: float, size: float) -> None:
    data = bpy.data.lights.new(name, "AREA")
    data.energy = energy
    data.shape = "DISK"
    data.size = size
    obj = bpy.data.objects.new(name, data)
    obj.location = location
    bpy.context.scene.collection.objects.link(obj)
    look_at(obj, target)


def main() -> None:
    source, output = arguments()
    output.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    bpy.ops.import_scene.gltf(filepath=str(source))

    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    points = [obj.matrix_world @ vertex.co for obj in meshes for vertex in obj.data.vertices]
    minimum = Vector(tuple(min(point[axis] for point in points) for axis in range(3)))
    maximum = Vector(tuple(max(point[axis] for point in points) for axis in range(3)))
    center = (minimum + maximum) * 0.5
    dimensions = maximum - minimum

    camera_data = bpy.data.cameras.new("CrewPreviewCamera")
    camera_data.type = "ORTHO"
    camera = bpy.data.objects.new("CrewPreviewCamera", camera_data)
    bpy.context.scene.collection.objects.link(camera)
    if dimensions.z >= dimensions.y:
        camera.location = Vector((0.0, minimum.y - 4.0, center.z))
        camera_data.ortho_scale = max(dimensions.x, dimensions.z) * 1.22
    else:
        camera.location = Vector((0.0, center.y, maximum.z + 4.0))
        camera_data.ortho_scale = max(dimensions.x, dimensions.y) * 1.22
    look_at(camera, center)
    bpy.context.scene.camera = camera

    add_area("Key", camera.location + Vector((-1.5, -0.5, 1.8)), center, 950.0, 3.0)
    add_area("Rim", Vector((1.6, 1.8, maximum.z + 1.5)), center, 650.0, 2.2)

    world = bpy.context.scene.world
    world.color = (0.012, 0.012, 0.012)
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE_NEXT"
    scene.render.resolution_x = 640
    scene.render.resolution_y = 640
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.filepath = str(output)
    scene.render.film_transparent = False
    scene.view_settings.look = "AgX - Medium High Contrast"
    bpy.ops.render.render(write_still=True)


if __name__ == "__main__":
    main()
