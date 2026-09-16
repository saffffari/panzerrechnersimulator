"""Bake a library of frozen-dead poses from the rigged infantry GLBs.

    blender --background --python tools/worldgen/bake_corpse_poses.py -- assets/warprops/dead

`assets/flavor/Characters/*/baked/` ships exactly one dead pose (`deadman_prone.glb`). A field of
several hundred identical prone bodies reads as a tiling error rather than a battlefield, so the
rigged source characters are posed here into a small library of distinct postures and exported as
static meshes (armature applied, skin collapsed) for cheap instanced placement.

The postures are the ones the winter Eastern Front actually produced, and each is chosen because
it is legible in silhouette at 640x360 after posterisation:

  sprawl_back   on the back, arms flung wide — blast or a burst taken standing.
  face_down     prone, arms beneath the body — shot moving forward.
  curled        drawn into a foetal curl. The terminal posture of hypothermia, and the one
                survivors describe most often in the Kessel: men sat down out of the wind and
                did not get up.
  on_side       fallen sideways, knees slightly drawn.
  seated_slump  died sitting, back against a wall or a wheel; common in dugouts and aid posts.
  arm_raised    rigor with one arm fixed above the body — recurrent in Eastern Front photographs
                because limbs froze in whatever attitude death left them.

Provenance note (CLAUDE.md first principle): these are *postures*, not depictions of any named
person or identified atrocity. They are placed by `gen_world.py` into sourced tableau types which
carry their own provenance.
"""

import bpy
from mathutils import Euler, Matrix
import math
import os
import sys

#: source path -> extra uniform scale on top of the source's own units. The rigged soldier is
#: already authored in metres (a T-posed figure measures ~2.0 m across), so it needs 1.0. The
#: factor exists because a source in centimetres would otherwise bake a 175 m body, and that
#: failure is caught by the span gate in `lay_and_ground` rather than discovered in-engine.
SOURCES = {
    "sol": ("assets/flavor/Characters/german-ww2-soldier-cc0/baked/"
            "german_ww2_soldier_rigged.glb", 1.0),
}

#: A laid-out human body: longest dimension in metres. Used as a sanity gate on every bake.
BODY_SPAN_M = (1.15, 2.35)

D = math.radians

# Per-pose bone rotations, in the rig's local bone space. Values are deliberately coarse: the
# target is a readable silhouette at low resolution, not anatomical simulation.
POSES = {
    "sprawl_back": {
        "_lay": ("back", 0.0),
        "LeftArm": (0, 0, D(-62)), "RightArm": (0, 0, D(62)),
        "LeftForeArm": (D(-18), 0, D(-24)), "RightForeArm": (D(-18), 0, D(24)),
        "LeftUpLeg": (D(-9), 0, D(13)), "RightUpLeg": (D(4), 0, D(-19)),
        "LeftLeg": (D(21), 0, 0), "RightLeg": (D(9), 0, 0),
        "Neck": (D(-16), D(19), 0), "Head": (0, D(24), 0),
        "Spine": (D(-6), 0, 0), "Spine1": (D(-5), 0, D(6)),
    },
    "face_down": {
        "_lay": ("front", 0.0),
        "LeftArm": (0, 0, D(-34)), "RightArm": (0, 0, D(41)),
        "LeftForeArm": (D(-64), 0, 0), "RightForeArm": (D(-52), 0, 0),
        "LeftUpLeg": (0, 0, D(7)), "RightUpLeg": (0, 0, D(-11)),
        "LeftLeg": (D(15), 0, 0), "RightLeg": (D(26), 0, 0),
        "Neck": (D(7), D(-31), 0), "Head": (0, D(-18), 0),
    },
    "curled": {
        "_lay": ("side", 0.0),
        "Spine": (D(23), 0, 0), "Spine1": (D(19), 0, 0), "Spine2": (D(14), 0, 0),
        "Neck": (D(26), 0, 0), "Head": (D(17), 0, 0),
        "LeftArm": (0, 0, D(-21)), "RightArm": (0, 0, D(24)),
        "LeftForeArm": (D(-96), 0, 0), "RightForeArm": (D(-104), 0, 0),
        "LeftUpLeg": (D(-74), 0, D(6)), "RightUpLeg": (D(-83), 0, D(-5)),
        "LeftLeg": (D(96), 0, 0), "RightLeg": (D(88), 0, 0),
    },
    "on_side": {
        "_lay": ("side", 0.0),
        "LeftArm": (0, 0, D(-38)), "RightArm": (0, 0, D(17)),
        "LeftForeArm": (D(-46), 0, 0), "RightForeArm": (D(-28), 0, 0),
        "LeftUpLeg": (D(-31), 0, 0), "RightUpLeg": (D(-16), 0, 0),
        "LeftLeg": (D(52), 0, 0), "RightLeg": (D(37), 0, 0),
        "Neck": (D(11), 0, 0), "Head": (D(8), 0, 0),
        "Spine": (D(8), 0, 0),
    },
    "seated_slump": {
        "_lay": ("seated", 0.0),
        "Spine": (D(19), 0, D(7)), "Spine1": (D(13), 0, D(5)), "Spine2": (D(8), 0, 0),
        "Neck": (D(31), 0, D(9)), "Head": (D(19), 0, 0),
        "LeftArm": (0, 0, D(-24)), "RightArm": (0, 0, D(21)),
        "LeftForeArm": (D(-31), 0, 0), "RightForeArm": (D(-24), 0, 0),
        "LeftUpLeg": (D(-79), 0, D(11)), "RightUpLeg": (D(-74), 0, D(-9)),
        "LeftLeg": (D(28), 0, 0), "RightLeg": (D(19), 0, 0),
    },
    "arm_raised": {
        "_lay": ("back", 0.0),
        "LeftArm": (0, 0, D(-48)), "RightArm": (D(-96), 0, D(31)),
        "LeftForeArm": (D(-21), 0, 0), "RightForeArm": (D(-41), 0, 0),
        "LeftUpLeg": (D(-6), 0, D(16)), "RightUpLeg": (D(8), 0, D(-8)),
        "LeftLeg": (D(14), 0, 0), "RightLeg": (D(31), 0, 0),
        "Neck": (D(-9), D(-26), 0), "Head": (0, D(-14), 0),
        "Spine1": (0, 0, D(-8)),
    },
}

# Object-space rotation that lays the standing figure down, applied after posing.
LAY = {
    "back":   (D(-90), 0.0, 0.0),
    "front":  (D(90), 0.0, 0.0),
    "side":   (D(-90), 0.0, D(90)),
    "seated": (D(-22), 0.0, 0.0),
}


def clear():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def import_rig(path):
    bpy.ops.import_scene.gltf(filepath=path)
    arm = next((o for o in bpy.data.objects if o.type == "ARMATURE"), None)
    # Only the skinned meshes under the armature are the character. The soldier source also
    # carries a loose 2 m `Icosphere` proxy with no parent and no armature modifier, which is
    # not part of the body and must not be baked into the corpse.
    meshes = [o for o in bpy.data.objects
              if o.type == "MESH" and o.parent is arm and arm is not None]
    return arm, meshes


def apply_pose(arm, spec):
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="POSE")
    for name, rot in spec.items():
        if name.startswith("_"):
            continue
        pb = arm.pose.bones.get(name)
        if pb is None:
            print(f"[corpse]   (no bone {name})")
            continue
        pb.rotation_mode = "XYZ"
        pb.rotation_euler = rot
    bpy.ops.object.mode_set(mode="OBJECT")


#: A corpse is rendered at 640x360 and usually seen at 10-80 m, so the source character's
#: ~5.2k triangles buy nothing. Collapse hard: these are placed by the thousand, and the posterised frame
#: cannot resolve more than the silhouette at any realistic viewing distance.
DECIMATE_RATIO = 0.12


def collapse(arm, meshes):
    """Apply the armature deformation at the current pose and drop the rig."""
    out = []
    for m in meshes:
        bpy.ops.object.select_all(action="DESELECT")
        m.select_set(True)
        bpy.context.view_layer.objects.active = m
        # convert() evaluates the full modifier stack, including the armature at its posed state
        bpy.ops.object.convert(target="MESH")
        dec = m.modifiers.new("dec", "DECIMATE")
        dec.ratio = DECIMATE_RATIO
        bpy.ops.object.modifier_apply(modifier=dec.name)
        out.append(m)
    # The meshes are children of the armature, which carries the source's unit scale. Deleting
    # the armature would silently drop that scale from the children and bake a body two orders
    # of magnitude too large, so unparent keeping the world transform first.
    bpy.ops.object.select_all(action="DESELECT")
    for m in out:
        m.select_set(True)
    bpy.context.view_layer.objects.active = out[0]
    bpy.ops.object.parent_clear(type="CLEAR_KEEP_TRANSFORM")
    bpy.ops.object.select_all(action="DESELECT")
    arm.select_set(True)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.delete()
    return out


def lay_and_ground(meshes, mode, scale):
    """Scale to metres, rotate into the lying attitude, then sit the lowest vertex on z = 0.

    The transform is *composed onto* each mesh's existing world matrix rather than assigned to
    its loc/rot/scale channels. Those channels already carry the glTF importer's Y-up->Z-up
    rotation and the armature's unit scale (baked in by the unparent above), and assigning over
    either one silently undoes it — which is how this bake first produced 178 m bodies, and then
    correctly-sized bodies that were still standing upright.
    """
    lay = Euler(LAY[mode], "XYZ").to_matrix().to_4x4()
    scl = Matrix.Diagonal((scale, scale, scale, 1.0))
    xform = lay @ scl
    for m in meshes:
        m.matrix_world = xform @ m.matrix_world
    bpy.context.view_layer.update()

    bpy.ops.object.select_all(action="DESELECT")
    for m in meshes:
        m.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)

    lo = min((m.matrix_world @ v.co).z for m in meshes for v in m.data.vertices)
    cx = sum((m.matrix_world @ v.co).x for m in meshes for v in m.data.vertices)
    cy = sum((m.matrix_world @ v.co).y for m in meshes for v in m.data.vertices)
    n = sum(len(m.data.vertices) for m in meshes)
    cx, cy = cx / n, cy / n
    for m in meshes:
        m.location = (m.location.x - cx, m.location.y - cy, m.location.z - lo)
    bpy.ops.object.transform_apply(location=True, rotation=False, scale=False)

    # Gate the result: a body that came out in source units instead of metres is off by 100x
    # and is instantly obvious in-engine, so fail the bake here rather than ship it.
    pts = [m.matrix_world @ v.co for m in meshes for v in m.data.vertices]
    span = max(max(p[i] for p in pts) - min(p[i] for p in pts) for i in range(3))
    if not (BODY_SPAN_M[0] <= span <= BODY_SPAN_M[1]):
        raise SystemExit(
            f"[corpse] FATAL: baked body spans {span:.2f} m, outside {BODY_SPAN_M} — "
            f"check the source's unit scale in SOURCES")
    return span


def main():
    argv = sys.argv
    outdir = argv[argv.index("--") + 1] if "--" in argv else "assets/warprops/dead"
    os.makedirs(outdir, exist_ok=True)

    made = []
    for src_key, (src_path, unit_scale) in SOURCES.items():
        if not os.path.exists(src_path):
            print(f"[corpse] MISSING source {src_path}")
            continue
        for pose_name, spec in POSES.items():
            clear()
            arm, meshes = import_rig(src_path)
            if arm is None or not meshes:
                print(f"[corpse] {src_path}: no armature/mesh")
                break
            apply_pose(arm, spec)
            meshes = collapse(arm, meshes)
            span = lay_and_ground(meshes, spec["_lay"][0], unit_scale)

            bpy.ops.object.select_all(action="DESELECT")
            for m in meshes:
                m.select_set(True)
            bpy.context.view_layer.objects.active = meshes[0]
            out = os.path.join(outdir, f"dead_{src_key}_{pose_name}.glb")
            bpy.ops.export_scene.gltf(
                filepath=out, export_format="GLB", use_selection=True,
                export_yup=True, export_apply=True,
            )
            tris = sum(sum(len(p.vertices) - 2 for p in m.data.polygons) for m in meshes)
            made.append((pose_name, tris))
            print(f"[corpse] {pose_name}: {tris} tris, {span:.2f} m -> {out}")

    print(f"[corpse] baked {len(made)} poses into {outdir}")


if __name__ == "__main__":
    main()
