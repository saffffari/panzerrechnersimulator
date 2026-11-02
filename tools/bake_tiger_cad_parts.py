#!/usr/bin/env python3
"""Tessellate the owner's Tiger CAD (1/35 kit STEP, SolidWorks 2018) into per-assembly meshes.

The Base Unit STEP (``TG_RM5010-0B16_Base Unit.STEP``, 190 solids) holds the hull, running gear,
engine, transmission, fuel tanks and cooling; the groups below were measured on 2 November 2026
(kit millimetres, z along the hull with the rear negative, y up). Each group is registered to the
game's hull frame by the measured transform of the damage session's registration pass
(``assets/fea/tiger_cad/registration.json`` in the fea-damage worktree, 2 November 2026: the
game's Tiger GLBs are this kit CAD at 0.038919827 m per kit millimetre, identity rotation,
translation (0, 0.570671, 2.657689) m; the game's Tiger is 11.2 % larger than the kit's stated
1:35), tessellated by FreeCAD's MeshPart and written as Wavefront OBJ under the asset root
(``assets/tanks/tiger/cad/<group>.obj``, not committed) with a JSON report of solids, bounds
and triangle count; ``tools/bake_tiger_cad_parts_glb.py`` turns them into runtime GLBs with
Blender. These are the repair manual's and the repair gameplay's physical parts and the damage
solver's component layers; the dimensions are the kit's, not a measured vehicle's.

Run with FreeCAD's own Python (positional arguments are opened as documents, so none are used)::

    KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem FreeCADCmd tools/bake_tiger_cad_parts.py < /dev/null
"""
import json
import os
import time

import FreeCAD
import MeshPart
import Part

CAD_ROOT = os.environ.get("CAD_ROOT", "/windows/shop/ts_assets/_archive/_extracted/_Reference_CAD_Tigers/"
                          "panzer-vi-tiger-1-35-1.snapshot.2/panzer-vi-tiger-1-35-1.snapshot.2")
ASSET_ROOT = os.environ.get("KESSEL_ASSET_ROOT", ".")
OUT_DIR = os.path.join(ASSET_ROOT, "assets", "tanks", "tiger", "cad")
BASE_UNIT = os.path.join(CAD_ROOT, "TG_RM5010-0B16_Base Unit.STEP")
# The measured registration kit mm -> game hull metres (see the module doc).
SCALE_TO_METRES = 0.038919827
HULL_OFFSET_M = (0.0, 0.570671, 2.657689)

GROUPS = {
    "hull_tub": [0],
    "engine": [152],
    "gearbox": [68],
    "steering_unit": [69],
    "forward_drive": [70],
    "final_drive_left": [72],
    "final_drive_right": [73],
    "fuel_tank_left": [132],
    "fuel_tank_right": [133],
    "radiators": [130, 131, 147, 148],
    "fans": [136, 137],
    "sprockets": [22, 23],
    "idlers": [26, 27],
    "side_plates": [20, 21],
    "sponsons": [169, 170],
    "under_floor_pair": [116, 117],
}

t0 = time.time()
shape = Part.Shape()
shape.read(BASE_UNIT)
m = FreeCAD.Matrix()
m.scale(SCALE_TO_METRES, SCALE_TO_METRES, SCALE_TO_METRES)
m.move(FreeCAD.Vector(*HULL_OFFSET_M))
shape = shape.transformGeometry(m)
solids = list(shape.Solids)
os.makedirs(OUT_DIR, exist_ok=True)
report = {"schema": "kessel.tiger-cad-parts.v2", "source": BASE_UNIT, "solids_in_source": len(solids),
          "frame": "game hull-local metres (y up, +z forward): kit mm x 0.038919827 + (0, 0.570671, 2.657689), the damage session's measured registration of 2 November 2026; the game's Tiger is 1.112 x the kit's 1:35",
          "read_seconds": round(time.time() - t0, 1), "groups": {}}
for name, indices in GROUPS.items():
    t1 = time.time()
    compound = Part.makeCompound([solids[i] for i in indices if i < len(solids)])
    mesh = MeshPart.meshFromShape(Shape=compound, LinearDeflection=0.004, AngularDeflection=0.35, Relative=False)
    path = os.path.join(OUT_DIR, f"{name}.obj")
    mesh.write(path)
    b = compound.BoundBox
    report["groups"][name] = {"solids": indices, "triangles": mesh.CountFacets, "obj": path,
                              "bbox_m": [round(v, 4) for v in (b.XMin, b.YMin, b.ZMin, b.XMax, b.YMax, b.ZMax)],
                              "seconds": round(time.time() - t1, 1)}
    print(f"[cad-parts] {name}: {mesh.CountFacets} triangles, bbox {[round(v, 3) for v in (b.XLength, b.YLength, b.ZLength)]} m")
json.dump(report, open(os.path.join(OUT_DIR, "report.json"), "w"), indent=1)
print("[cad-parts] done in", round(time.time() - t0, 1), "s ->", OUT_DIR)
