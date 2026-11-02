#!/usr/bin/env python3
"""Turn the tessellated Tiger CAD assemblies (tools/bake_tiger_cad_parts.py, OBJ in metres) into
runtime GLBs with one grey painted-steel material each, under the same folder
(``assets/tanks/tiger/cad/<group>.glb``, not committed), and extend the bake report.

Run with Blender from the repository root::

    KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem ~/.local/bin/blender --background \\
      --factory-startup --python-exit-code 1 --python tools/bake_tiger_cad_parts_glb.py

The OBJ axes are the kit's (y up, z along the hull with the rear negative); the glTF export keeps
y up. Placement in the hull is the game's business (src/interior_rig.rs), not this bake's.
"""
import json
import os
import sys

import bpy

ASSET_ROOT = os.environ.get("KESSEL_ASSET_ROOT", ".")
CAD_DIR = os.path.join(ASSET_ROOT, "assets", "tanks", "tiger", "cad")
report_path = os.path.join(CAD_DIR, "report.json")
report = json.load(open(report_path))
bpy.ops.wm.read_factory_settings(use_empty=True)
material = bpy.data.materials.new("tiger_cad_steel")
material.use_nodes = True
bsdf = material.node_tree.nodes.get("Principled BSDF")
bsdf.inputs["Base Color"].default_value = (0.36, 0.34, 0.30, 1.0)
bsdf.inputs["Metallic"].default_value = 0.35
bsdf.inputs["Roughness"].default_value = 0.62
for name, entry in report["groups"].items():
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj, do_unlink=True)
    bpy.ops.wm.obj_import(filepath=entry["obj"], forward_axis="NEGATIVE_Z", up_axis="Y")
    meshes = [o for o in bpy.data.objects if o.type == "MESH"]
    if not meshes:
        print(f"[cad-glb] {name}: no mesh imported", file=sys.stderr)
        continue
    for o in meshes:
        o.data.materials.clear()
        o.data.materials.append(material)
        o.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    bpy.ops.object.join()
    joined = bpy.context.view_layer.objects.active
    joined.name = name
    glb = os.path.join(CAD_DIR, f"{name}.glb")
    bpy.ops.export_scene.gltf(filepath=glb, export_format="GLB", use_selection=True, export_apply=True,
                              export_yup=True, export_materials="EXPORT", export_normals=True)
    entry["glb"] = glb
    entry["glb_triangles"] = sum(len(p.vertices) - 2 for p in joined.data.polygons)
    print(f"[cad-glb] {name}: {entry['glb_triangles']} triangles -> {glb}")
json.dump(report, open(report_path, "w"), indent=1)
print("[cad-glb] report updated", report_path)
