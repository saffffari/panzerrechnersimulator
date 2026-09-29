# kit_variant_sheet.py — render a base wall bay and its damage, soot and snow variants side by side
# (headless bpy), from the meshes the engine built (`cargo run -- urban-kit-variants`).
#
#   cargo run -- urban-kit-variants captures urban.brick.window
#   blender --background --python tools/worldgen/kit_variant_sheet.py -- \
#       captures/urban_kit_variants.json captures/urban_kit_sheet_window.png [filter ...]
#
# Each bay stands at the module scale the damage layouts assume (building_kit_damage.rs
# TYPICAL_SCALE: a 3.0 m bay, 3.6 m storey, 0.52 m wall), in rows of up to eight, labelled by the
# part of its id after ".v."; one image looks at the outer faces from the street under a low sun,
# a second at the inner faces. An Eevee preview for judging form, not the shipping renderer.
# Deterministic: fixed cameras, sun and sampling.
import json
import math
import os
import sys

import bpy
from mathutils import Matrix, Vector

argv = sys.argv[sys.argv.index("--") + 1:]
INDEX, OUT = argv[0], argv[1]
FILTERS = argv[2:]
HERE = os.path.dirname(os.path.abspath(INDEX))
SCALE = (0.75, 1.2, 1.3)
PER_ROW = 8
GAP_X, ROW_H = 1.2, 5.2

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
entries = json.load(open(INDEX))
if FILTERS:
    entries = [e for e in entries if all(f in e["id"] for f in FILTERS)]
materials = {}


def material(texture):
    if texture in materials:
        return materials[texture]
    mat = bpy.data.materials.new(texture or "untextured")
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.9
    if texture:
        node = mat.node_tree.nodes.new("ShaderNodeTexImage")
        node.image = bpy.data.images.load(os.path.join(HERE, texture))
        node.interpolation = "Closest"
        mat.node_tree.links.new(node.outputs["Color"], bsdf.inputs["Base Color"])
    materials[texture] = mat
    return mat


def build(entry):
    data = json.load(open(os.path.join(HERE, entry["mesh"])))
    p = data["positions"]
    # Engine (x, y, z) → Blender (x, -z, y).
    verts = [(p[i] * SCALE[0], -p[i + 2] * SCALE[2], p[i + 1] * SCALE[1]) for i in range(0, len(p), 3)]
    idx = data["indices"]
    faces = [(idx[i], idx[i + 1], idx[i + 2]) for i in range(0, len(idx), 3)]
    mesh = bpy.data.meshes.new(entry["id"])
    mesh.from_pydata(verts, [], faces)
    uv = mesh.uv_layers.new(name="UVMap")
    uvs = data["uvs"]
    for loop in mesh.loops:
        k = loop.vertex_index
        uv.data[loop.index].uv = (uvs[2 * k], 1.0 - uvs[2 * k + 1])
    mesh.materials.append(material(data.get("texture")))
    mesh.update()
    obj = bpy.data.objects.new(entry["id"], mesh)
    scene.collection.objects.link(obj)
    return obj


width = 4.0 * SCALE[0]
objects = []
for n, entry in enumerate(entries):
    row, col = divmod(n, PER_ROW)
    obj = build(entry)
    # Rows stack downward in the image (Blender −Z); every bay stands at the same depth.
    obj.location = Vector((col * (width + GAP_X), 0.0, -row * ROW_H))
    label = entry["id"].split(".v.")[-1] if ".v." in entry["id"] else "base"
    curve = bpy.data.curves.new(f"label{n}", "FONT")
    curve.body = label
    curve.size = 0.26
    text = bpy.data.objects.new(f"label{n}", curve)
    text.location = obj.location + Vector((-width / 2, -0.8, -0.5))
    text.rotation_euler = (math.radians(90), 0.0, 0.0)
    scene.collection.objects.link(text)
    objects.append(obj)

rows = (len(entries) + PER_ROW - 1) // PER_ROW
cols = min(len(entries), PER_ROW)
x1 = cols * (width + GAP_X)

world = bpy.data.worlds.new("sky")
world.use_nodes = True
world.node_tree.nodes["Background"].inputs[0].default_value = (0.62, 0.66, 0.72, 1.0)
world.node_tree.nodes["Background"].inputs[1].default_value = 0.9
scene.world = world
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 3.2
sun.data.angle = math.radians(1.0)
sun.rotation_euler = (math.radians(58), 0.0, math.radians(-28))
scene.collection.objects.link(sun)
scene.render.engine = "BLENDER_EEVEE"
span_x, span_z = x1 + 1.0, rows * ROW_H + 0.6
scene.render.resolution_x = 2400
scene.render.resolution_y = int(2400 * span_z / span_x)
scene.view_settings.view_transform = "Standard"

cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
cam.data.type = "ORTHO"
cam.data.ortho_scale = span_x
scene.collection.objects.link(cam)
scene.camera = cam
centre = Vector((x1 / 2 - (width + GAP_X) / 2, 0.0, 3.6 * 0.5 - (rows - 1) * ROW_H / 2 - 0.3))
paths = []
for side, dy in {"outer": 1.0, "inner": -1.0}.items():
    # Outer faces look toward engine +Z = Blender −Y: the camera stands on −Y looking +Y, 12°
    # above the horizontal so sills and treads show.
    cam.location = centre + Vector((0.0, -60.0 * dy, 60.0 * math.tan(math.radians(12))))
    direction = centre - cam.location
    cam.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()
    # Labels read from the street side only (the inner view is mirrored left to right).
    for obj in bpy.data.objects:
        if obj.type == "FONT":
            obj.hide_render = dy < 0
    path = os.path.splitext(OUT)[0] + f"__{side}.png"
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    paths.append(path)

import numpy as np  # noqa: E402

images = [bpy.data.images.load(p) for p in paths]
w, h = images[0].size
canvas = np.zeros((h * len(images), w, 4), dtype=np.float32)
for i, img in enumerate(images):
    px = np.empty(w * h * 4, dtype=np.float32)
    img.pixels.foreach_get(px)
    canvas[(len(images) - 1 - i) * h:(len(images) - i) * h] = px.reshape(h, w, 4)
out = bpy.data.images.new("sheet", w, h * len(images), alpha=False)
out.pixels.foreach_set(canvas.ravel())
out.filepath_raw = os.path.abspath(OUT)
out.file_format = "PNG"
out.save()
for p in paths:
    os.remove(p)
print(f"[kit-sheet] {OUT}: {len(entries)} bays")
