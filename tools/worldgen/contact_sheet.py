"""Render one frame containing every given GLB, laid out on a labelled grid.

    blender --background --python tools/worldgen/contact_sheet.py -- OUT.png [--cell M] GLB [GLB ...]

Authoring check only: it verifies silhouette, scale and the ground-contact origin convention for a
batch of props without booting the engine. Models are imported into a single scene, spaced on a
grid with a metre reference cube at each station, and shot from one three-quarter camera.
"""

import bpy
import math
import os
import sys
from mathutils import Vector

COLS = 6


def clear():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def flat_mat(name, rgb, rough=0.95):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    b = m.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (*rgb, 1.0)
    b.inputs["Roughness"].default_value = rough
    return m


def main():
    argv = sys.argv
    args = argv[argv.index("--") + 1:]
    out = args[0]
    args = args[1:]
    cell = 4.0
    if args and args[0] == "--cell":
        cell = float(args[1])
        args = args[2:]
    paths = args

    clear()
    sc = bpy.context.scene

    ground_m = flat_mat("ground", (0.40, 0.42, 0.45))
    ref_m = flat_mat("ref", (0.62, 0.20, 0.16))

    n = len(paths)
    cols = COLS
    rows = (n + cols - 1) // cols
    span_x = cols * cell
    span_y = rows * cell

    bpy.ops.mesh.primitive_plane_add(size=max(span_x, span_y) * 1.6,
                                     location=(span_x / 2, -span_y / 2, 0))
    bpy.context.active_object.data.materials.append(ground_m)

    order = []
    for i, p in enumerate(paths):
        cx = (i % cols) * cell + cell * 0.5
        cy = -((i // cols) * cell + cell * 0.5)
        name = os.path.splitext(os.path.basename(p))[0]
        before = set(bpy.data.objects)
        try:
            bpy.ops.import_scene.gltf(filepath=p)
        except Exception as exc:  # noqa: BLE001
            print(f"[sheet] FAILED {name}: {exc}")
            continue
        new = [o for o in bpy.data.objects if o not in before]
        # glTF import parents everything under a rotated root empty (Y-up -> Z-up). Offsetting a
        # child applies in that rotated parent space and throws the prop off the grid, so move only
        # the roots, whose location is world space.
        roots = [o for o in new if o.parent is None]
        for o in roots:
            o.location = (o.location.x + cx, o.location.y + cy, o.location.z)
        if not any(o.type == "MESH" for o in new):
            print(f"[sheet] WARNING no mesh imported for {name}")
        # 1 m reference cube at the back-left of every station
        bpy.ops.mesh.primitive_cube_add(size=1.0,
                                        location=(cx - cell * 0.38, cy - cell * 0.38, 0.5))
        bpy.context.active_object.data.materials.append(ref_m)
        order.append((i, name))

    bpy.ops.object.light_add(type="SUN", location=(span_x, -span_y, 60))
    sun = bpy.context.active_object
    sun.data.energy = 5.0
    sun.rotation_euler = (math.radians(50), 0.0, math.radians(35))

    # Orthographic three-quarter view: framing is then exactly ortho_scale wide, with no
    # perspective FOV arithmetic to get wrong, and every cell is shown at the same scale.
    ctr = Vector((span_x / 2, -span_y / 2, 0.0))
    d = max(span_x, span_y)
    cam_pos = ctr + Vector((0.0, -d * 1.1, d * 1.1))
    bpy.ops.object.camera_add(location=cam_pos)
    cam = bpy.context.active_object
    cam.rotation_euler = (ctr - cam_pos).to_track_quat("-Z", "Y").to_euler()
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = max(span_x, span_y * 1.45) * 1.08
    sc.camera = cam

    # Workbench renders reliably in a headless context (EEVEE needs a GPU surface and comes back
    # black on this box) and shows material base colour and silhouette, which is all this check is.
    sc.render.engine = "BLENDER_WORKBENCH"
    sh = sc.display.shading
    sh.light = "STUDIO"
    sh.color_type = "MATERIAL"
    sh.show_shadows = True
    sh.show_cavity = True
    sc.render.resolution_x = 1920
    sc.render.resolution_y = 1280
    sc.render.filepath = out
    sc.render.image_settings.file_format = "PNG"
    if sc.world is None:  # use_empty=True starts with no world datablock
        sc.world = bpy.data.worlds.new("sheet_world")
    sc.world.use_nodes = True
    sc.world.node_tree.nodes["Background"].inputs[0].default_value = (0.52, 0.57, 0.63, 1.0)
    sc.world.node_tree.nodes["Background"].inputs[1].default_value = 1.6
    # Authoring check: keep the response linear so prop albedo is read as authored, not graded.
    sc.view_settings.view_transform = "Standard"
    sc.view_settings.look = "None"
    from mathutils import Vector as _V
    pts = [o.matrix_world @ _V(c) for o in bpy.data.objects if o.type == "MESH"
           for c in o.bound_box]
    if pts:
        print("[sheet] scene bbox x[%.1f,%.1f] y[%.1f,%.1f] ortho=%.1f cam=%s" % (
            min(p.x for p in pts), max(p.x for p in pts),
            min(p.y for p in pts), max(p.y for p in pts),
            cam.data.ortho_scale, tuple(round(v, 1) for v in cam.location)))
    bpy.ops.render.render(write_still=True)

    print(f"[sheet] wrote {out}  ({cols} cols, {cell} m cells, red cube = 1 m)")
    for i, name in order:
        print(f"[sheet]   r{i // cols}c{i % cols}  {name}")


if __name__ == "__main__":
    main()
