"""Blender-only helpers for deterministic reviewed static-asset bakes."""

from __future__ import annotations

from array import array
import json
from pathlib import Path
from typing import Callable, Iterable, Mapping, Sequence

import bpy
from mathutils import Matrix, Vector

import reviewed_static_asset_recipe as recipe


def supported_operator_keywords(operator, keywords: Mapping[str, object]) -> dict[str, object]:
    properties = operator.get_rna_type().properties.keys()
    return {name: value for name, value in keywords.items() if name in properties}


def clear_scene() -> None:
    if bpy.context.object is not None and bpy.context.object.mode != "OBJECT":
        bpy.ops.object.mode_set(mode="OBJECT")
    bpy.ops.wm.read_factory_settings(use_empty=True)


def import_reviewed_fbx(source_path: Path) -> list[bpy.types.Object]:
    clear_scene()
    result = bpy.ops.import_scene.fbx(
        **supported_operator_keywords(
            bpy.ops.import_scene.fbx,
            {
                "filepath": str(source_path),
                "use_anim": False,
                "use_custom_normals": True,
                "use_image_search": False,
                "automatic_bone_orientation": False,
            },
        )
    )
    if "FINISHED" not in result:
        raise RuntimeError(f"FBX import failed: {result}")
    for obj in list(bpy.context.scene.objects):
        if obj.type != "MESH":
            bpy.data.objects.remove(obj, do_unlink=True)
    meshes = sorted(
        (obj for obj in bpy.context.scene.objects if obj.type == "MESH"),
        key=lambda obj: obj.name,
    )
    if not meshes:
        raise RuntimeError(f"FBX contains no mesh geometry: {source_path}")
    return meshes


def select_only(obj: bpy.types.Object) -> None:
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj


def apply_object_transforms(meshes: Sequence[bpy.types.Object]) -> None:
    for obj in meshes:
        select_only(obj)
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    bpy.context.view_layer.update()


def triangle_count(meshes: Iterable[bpy.types.Object]) -> int:
    return sum(
        max(0, len(polygon.vertices) - 2)
        for obj in meshes
        for polygon in obj.data.polygons
    )


def vertex_count(meshes: Iterable[bpy.types.Object]) -> int:
    return sum(len(obj.data.vertices) for obj in meshes)


def triangulate(meshes: Sequence[bpy.types.Object]) -> None:
    for obj in meshes:
        modifier = obj.modifiers.new(name="Kessel_Deterministic_Triangulate", type="TRIANGULATE")
        modifier.quad_method = "BEAUTY"
        modifier.ngon_method = "BEAUTY"
        if hasattr(modifier, "keep_custom_normals"):
            modifier.keep_custom_normals = True
        select_only(obj)
        bpy.ops.object.modifier_apply(modifier=modifier.name)
    bpy.context.view_layer.update()


def decimate_to_fraction_with_floor(
    meshes: Sequence[bpy.types.Object], *, fraction: float, minimum_triangles_per_part: int
) -> list[dict[str, object]]:
    if not 0.0 < fraction <= 1.0:
        raise ValueError(f"invalid decimation fraction: {fraction}")
    records: list[dict[str, object]] = []
    for obj in meshes:
        before = triangle_count((obj,))
        target = min(before, max(minimum_triangles_per_part, round(before * fraction)))
        if before > target:
            modifier = obj.modifiers.new(name="Kessel_Reviewed_Collapse", type="DECIMATE")
            modifier.decimate_type = "COLLAPSE"
            modifier.ratio = target / before
            modifier.use_collapse_triangulate = True
            select_only(obj)
            bpy.ops.object.modifier_apply(modifier=modifier.name)
        after = triangle_count((obj,))
        if after <= 0:
            raise RuntimeError(f"decimation erased reviewed object {obj.name!r}")
        records.append(
            {
                "object": obj.name,
                "source_triangles": before,
                "target_triangles": target,
                "baked_triangles": after,
            }
        )
    bpy.context.view_layer.update()
    return records


def exact_world_bounds(meshes: Iterable[bpy.types.Object]) -> tuple[Vector, Vector]:
    points = [
        obj.matrix_world @ vertex.co
        for obj in meshes
        for vertex in obj.data.vertices
    ]
    if not points:
        raise RuntimeError("mesh selection has no vertices")
    minimum = Vector(tuple(min(point[axis] for point in points) for axis in range(3)))
    maximum = Vector(tuple(max(point[axis] for point in points) for axis in range(3)))
    return minimum, maximum


def translate_mesh_vertices(meshes: Iterable[bpy.types.Object], offset: Vector) -> None:
    transform = Matrix.Translation(offset)
    for obj in meshes:
        if not obj.matrix_world.is_identity:
            raise RuntimeError(f"cannot vertex-translate non-identity object {obj.name!r}")
        obj.data.transform(transform)
        obj.data.update()
    bpy.context.view_layer.update()


def center_xy_and_ground(meshes: Sequence[bpy.types.Object]) -> Vector:
    minimum, maximum = exact_world_bounds(meshes)
    offset = Vector(
        (
            -(minimum.x + maximum.x) * 0.5,
            -(minimum.y + maximum.y) * 0.5,
            -minimum.z,
        )
    )
    translate_mesh_vertices(meshes, offset)
    return offset


def center_xy_and_set_deck(
    meshes: Sequence[bpy.types.Object], deck_world_z: float
) -> Vector:
    minimum, maximum = exact_world_bounds(meshes)
    offset = Vector(
        (
            -(minimum.x + maximum.x) * 0.5,
            -(minimum.y + maximum.y) * 0.5,
            -deck_world_z,
        )
    )
    translate_mesh_vertices(meshes, offset)
    return offset


def material_name_for_polygon(obj: bpy.types.Object, material_index: int) -> str:
    if material_index < 0 or material_index >= len(obj.material_slots):
        raise RuntimeError(
            f"polygon on {obj.name!r} references missing material slot {material_index}"
        )
    material = obj.material_slots[material_index].material
    if material is None:
        raise RuntimeError(
            f"polygon on {obj.name!r} references an empty material slot {material_index}"
        )
    return material.name


def remap_material_uvs(
    meshes: Sequence[bpy.types.Object],
    material_to_tile: Callable[[str], int],
) -> dict[str, int]:
    polygon_counts: dict[str, int] = {}
    for obj in meshes:
        uv_layer = obj.data.uv_layers.active
        if uv_layer is None:
            raise RuntimeError(f"reviewed mesh {obj.name!r} has no UV layer")
        for polygon in obj.data.polygons:
            material_name = material_name_for_polygon(obj, polygon.material_index)
            tile_index = material_to_tile(material_name)
            polygon_counts[material_name] = polygon_counts.get(material_name, 0) + 1
            for loop_index in polygon.loop_indices:
                original = uv_layer.data[loop_index].uv.copy()
                if (
                    original.x < -1.0e-5
                    or original.x > 1.00001
                    or original.y < -1.0e-5
                    or original.y > 1.00001
                ):
                    raise RuntimeError(
                        f"reviewed UV outside [0,1] on {obj.name!r}: "
                        f"({original.x:.7f}, {original.y:.7f})"
                    )
                uv_layer.data[loop_index].uv = recipe.remap_uv(original, tile_index)
            polygon.material_index = 0
    return dict(sorted(polygon_counts.items()))


def join_meshes(meshes: Sequence[bpy.types.Object], name: str) -> bpy.types.Object:
    if not meshes:
        raise RuntimeError("cannot join an empty mesh selection")
    if len(meshes) == 1:
        joined = meshes[0]
        select_only(joined)
    else:
        bpy.ops.object.select_all(action="DESELECT")
        for obj in meshes:
            obj.select_set(True)
        bpy.context.view_layer.objects.active = meshes[0]
        bpy.ops.object.join()
        joined = bpy.context.view_layer.objects.active
    joined.name = name
    if joined.type != "MESH":
        raise RuntimeError("joined reviewed asset is not a mesh")
    joined.data.name = name
    return joined


def assign_only_material(obj: bpy.types.Object, material: bpy.types.Material) -> None:
    obj.data.materials.clear()
    obj.data.materials.append(material)
    for polygon in obj.data.polygons:
        polygon.material_index = 0


def _load_scaled_rgba(path: Path, *, colorspace: str) -> array:
    image = bpy.data.images.load(str(path), check_existing=False)
    image.colorspace_settings.name = colorspace
    image.scale(recipe.ATLAS_TILE_SIZE_PX, recipe.ATLAS_TILE_SIZE_PX)
    pixel_count = recipe.ATLAS_TILE_SIZE_PX * recipe.ATLAS_TILE_SIZE_PX * 4
    pixels = array("f", [0.0]) * pixel_count
    image.pixels.foreach_get(pixels)
    bpy.data.images.remove(image)
    return pixels


def load_base_color_tile(path: Path) -> array:
    return _load_scaled_rgba(path, colorspace="sRGB")


def load_metallic_roughness_tile(roughness_path: Path, metallic_path: Path) -> array:
    roughness = _load_scaled_rgba(roughness_path, colorspace="Non-Color")
    metallic = _load_scaled_rgba(metallic_path, colorspace="Non-Color")
    pixel_count = recipe.ATLAS_TILE_SIZE_PX * recipe.ATLAS_TILE_SIZE_PX
    result = array("f", [1.0]) * (pixel_count * 4)
    for pixel in range(pixel_count):
        channel = pixel * 4
        result[channel] = 1.0
        result[channel + 1] = roughness[channel]
        result[channel + 2] = metallic[channel]
        result[channel + 3] = 1.0
    return result


def load_opengl_normal_tile(directx_normal_path: Path) -> array:
    result = _load_scaled_rgba(directx_normal_path, colorspace="Non-Color")
    pixel_count = recipe.ATLAS_TILE_SIZE_PX * recipe.ATLAS_TILE_SIZE_PX
    for pixel in range(pixel_count):
        channel = pixel * 4
        result[channel + 1] = 1.0 - result[channel + 1]
        result[channel + 3] = 1.0
    return result


def constant_tile(color: Sequence[float]) -> array:
    if len(color) != 4:
        raise ValueError("constant atlas tile requires RGBA")
    return array("f", (float(value) for value in color)) * (
        recipe.ATLAS_TILE_SIZE_PX * recipe.ATLAS_TILE_SIZE_PX
    )


def create_atlas(
    name: str,
    tiles: Mapping[int, array],
    *,
    colorspace: str,
    default_color: Sequence[float],
) -> bpy.types.Image:
    width = recipe.ATLAS_SIZE_PX
    tile_size = recipe.ATLAS_TILE_SIZE_PX
    pixels = constant_tile(default_color) * (
        (width * width) // (tile_size * tile_size)
    )
    expected_tile_values = tile_size * tile_size * 4
    for tile_index, tile_pixels in sorted(tiles.items()):
        if len(tile_pixels) != expected_tile_values:
            raise RuntimeError(
                f"atlas tile {tile_index} has {len(tile_pixels)} values, "
                f"expected {expected_tile_values}"
            )
        offset_u, offset_v = recipe.atlas_offset(tile_index)
        offset_x = round(offset_u * width)
        offset_y = round(offset_v * width)
        row_values = tile_size * 4
        for row in range(tile_size):
            source_start = row * row_values
            destination_start = ((offset_y + row) * width + offset_x) * 4
            pixels[destination_start : destination_start + row_values] = tile_pixels[
                source_start : source_start + row_values
            ]

    image = bpy.data.images.new(
        name=name,
        width=width,
        height=width,
        alpha=True,
        float_buffer=False,
    )
    image.colorspace_settings.name = colorspace
    image.file_format = "PNG"
    image.pixels.foreach_set(pixels)
    image.update()
    image.pack()
    return image


def new_base_color_material(
    name: str, base_color_atlas: bpy.types.Image, *, roughness: float
) -> bpy.types.Material:
    material = bpy.data.materials.new(name)
    material.use_nodes = True
    nodes = material.node_tree.nodes
    links = material.node_tree.links
    nodes.clear()
    output = nodes.new("ShaderNodeOutputMaterial")
    shader = nodes.new("ShaderNodeBsdfPrincipled")
    texture = nodes.new("ShaderNodeTexImage")
    texture.image = base_color_atlas
    texture.interpolation = "Linear"
    shader.inputs["Metallic"].default_value = 0.0
    shader.inputs["Roughness"].default_value = roughness
    links.new(texture.outputs["Color"], shader.inputs["Base Color"])
    links.new(shader.outputs["BSDF"], output.inputs["Surface"])
    return material


def new_pbr_material(
    name: str,
    base_color_atlas: bpy.types.Image,
    metallic_roughness_atlas: bpy.types.Image,
    normal_atlas: bpy.types.Image,
) -> bpy.types.Material:
    material = bpy.data.materials.new(name)
    material.use_nodes = True
    nodes = material.node_tree.nodes
    links = material.node_tree.links
    nodes.clear()
    output = nodes.new("ShaderNodeOutputMaterial")
    shader = nodes.new("ShaderNodeBsdfPrincipled")
    base_texture = nodes.new("ShaderNodeTexImage")
    base_texture.image = base_color_atlas
    base_texture.interpolation = "Linear"
    mr_texture = nodes.new("ShaderNodeTexImage")
    mr_texture.image = metallic_roughness_atlas
    mr_texture.interpolation = "Linear"
    separate = nodes.new("ShaderNodeSeparateColor")
    normal_texture = nodes.new("ShaderNodeTexImage")
    normal_texture.image = normal_atlas
    normal_texture.interpolation = "Linear"
    normal_map = nodes.new("ShaderNodeNormalMap")
    shader.inputs["Metallic"].default_value = 1.0
    shader.inputs["Roughness"].default_value = 1.0
    links.new(base_texture.outputs["Color"], shader.inputs["Base Color"])
    links.new(mr_texture.outputs["Color"], separate.inputs["Color"])
    links.new(separate.outputs["Green"], shader.inputs["Roughness"])
    links.new(separate.outputs["Blue"], shader.inputs["Metallic"])
    links.new(normal_texture.outputs["Color"], normal_map.inputs["Color"])
    links.new(normal_map.outputs["Normal"], shader.inputs["Normal"])
    links.new(shader.outputs["BSDF"], output.inputs["Surface"])
    return material


def export_glb(obj: bpy.types.Object, output_path: Path) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    select_only(obj)
    result = bpy.ops.export_scene.gltf(
        **supported_operator_keywords(
            bpy.ops.export_scene.gltf,
            {
                "filepath": str(output_path),
                "export_format": "GLB",
                "use_selection": True,
                "export_apply": True,
                "export_texcoords": True,
                "export_normals": True,
                "export_tangents": True,
                "export_materials": "EXPORT",
                "export_image_format": "AUTO",
                "export_skins": False,
                "export_animations": False,
                "export_cameras": False,
                "export_lights": False,
                "export_yup": True,
                "export_extras": False,
            },
        )
    )
    if "FINISHED" not in result or not output_path.is_file():
        raise RuntimeError(f"GLB export failed: {result}")


def round_vector(vector: Iterable[float]) -> list[float]:
    return [round(float(value), 6) for value in vector]


def blender_version_record() -> dict[str, str]:
    return {
        "version": bpy.app.version_string,
        "build_hash": bpy.app.build_hash.decode("ascii")
        if isinstance(bpy.app.build_hash, bytes)
        else str(bpy.app.build_hash),
    }


def output_record(path: Path) -> dict[str, object]:
    return {
        "filename": path.name,
        "bytes": path.stat().st_size,
        "sha256": recipe.sha256(path),
        "self_contained_glb": True,
    }


def write_report(path: Path, report: Mapping[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
