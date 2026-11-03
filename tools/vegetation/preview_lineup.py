"""Blender preview of the generated vegetation (shape review only; the real look is the engine's).

    blender --background --python tools/vegetation/preview_lineup.py -- ASSET_DIR OUT.png [PREFIXES]

Imports every GLB in ASSET_DIR (or those whose names start with one of the comma-separated
PREFIXES), stands them in rows by kind with 2 m between bounding boxes, and renders a side view
under a sun and a plain sky with Cycles at low samples.
"""
import math
import os
import sys

import bpy

argv = sys.argv[sys.argv.index('--') + 1:]
src, out = argv[0], argv[1]
prefixes = tuple(argv[2].split(',')) if len(argv) > 2 else None
bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
files = sorted(f for f in os.listdir(src) if f.endswith('.glb') and (not prefixes or f.startswith(prefixes)))
rows = {}
for f in files:
    kind = f.split('_')[0]
    rows.setdefault(kind, []).append(f)
order = ['tree', 'fallen', 'stump', 'shrub']
y = 0.0
xmax = 0.0
ztop = 1.0
for kind in order:
    x = 0.0
    depth = 0.0
    for f in rows.get(kind, []):
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=os.path.join(src, f))
        objs = [o for o in bpy.data.objects if o not in before and o.type == 'MESH']
        xs = [(o.matrix_world @ v.co).x for o in objs for v in o.data.vertices]
        ys = [(o.matrix_world @ v.co).y for o in objs for v in o.data.vertices]
        w = max(xs) - min(xs)
        zs = [(o.matrix_world @ v.co).z for o in objs for v in o.data.vertices]
        ztop = max(ztop, max(zs))
        for o in [o for o in bpy.data.objects if o not in before]:
            if o.parent is None:
                o.location.x += x - min(xs)
                o.location.y += y
        depth = max(depth, max(ys) - min(ys))
        x += w + 2.0
    xmax = max(xmax, x)
    y += depth + 6.0
bpy.ops.mesh.primitive_plane_add(size=1, location=(xmax / 2, y / 2, 0))
plane = bpy.context.object
plane.scale = (xmax * 1.5, y * 3 + 60, 1)
mat = bpy.data.materials.new('ground')
mat.use_nodes = True
mat.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value = (0.08, 0.075, 0.06, 1)
plane.data.materials.append(mat)
sun = bpy.data.objects.new('sun', bpy.data.lights.new('sun', 'SUN'))
sun.data.energy = 4.0
sun.rotation_euler = (math.radians(50), 0, math.radians(30))
scene.collection.objects.link(sun)
world = bpy.data.worlds.new('w')
world.use_nodes = True
world.node_tree.nodes['Background'].inputs['Color'].default_value = (0.45, 0.6, 0.85, 1)
world.node_tree.nodes['Background'].inputs['Strength'].default_value = 0.8
scene.world = world
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam'))
cam.data.type = 'ORTHO'
aspect = min(max(xmax / (ztop + 3.0), 1.0), 6.0)
cam.data.ortho_scale = max(xmax * 1.02, (ztop + 3.0) * aspect)
cam.location = (xmax / 2, -200, (ztop + 3.0) / 2 - 1.0)
cam.rotation_euler = (math.radians(90), 0, 0)
scene.collection.objects.link(cam)
scene.camera = cam
scene.render.engine = 'CYCLES'
scene.cycles.samples = 24
scene.cycles.device = 'CPU'
scene.render.resolution_x = 3000
scene.render.resolution_y = int(3000 / aspect)
scene.render.filepath = out
scene.view_settings.view_transform = 'Standard'
bpy.ops.render.render(write_still=True)
