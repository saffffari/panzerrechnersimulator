# render_shot.py — render a single camera shot from an assembled Kessel .blend (headless bpy).
# The verification loop for the world→Blender collaboration: after assemble_blend.py, point this
# at any spot in ENGINE coordinates (x=east, y=up, z=south) to check dressing at ground level.
#
# Usage: blender --background <world.blend> --python tools/worldgen/render_shot.py -- \
#            <out.png> <pos_x> <pos_y> <pos_z> <tgt_x> <tgt_y> <tgt_z> [lens_mm]
import math
import os
import sys

import bpy
from mathutils import Vector

argv = sys.argv[sys.argv.index("--") + 1:]
OUT = argv[0]
px, py, pz, tx, ty, tz = (float(v) for v in argv[1:7])
lens = float(argv[7]) if len(argv) > 7 else 32.0

# engine → blender: (x, -z, y)
pos = Vector((px, -pz, py))
tgt = Vector((tx, -tz, ty))

scene = bpy.context.scene
cam = bpy.data.objects.new("ShotCam", bpy.data.cameras.new("ShotCam"))
cam.location = pos
cam.rotation_euler = (tgt - pos).to_track_quat("-Z", "Y").to_euler()
cam.data.clip_end = 15000.0
cam.data.lens = lens
scene.collection.objects.link(cam)
scene.camera = cam

scene.render.engine = "BLENDER_EEVEE"
scene.render.resolution_x = 1280
scene.render.resolution_y = 720
scene.render.filepath = os.path.abspath(OUT)
bpy.ops.render.render(write_still=True)
print(f"[shot] rendered {OUT}")
