"""Export the gen2 Tiger rig file for the game, at the kit's true size (2026-11-09, the owner:
"True size instead").

    blender -b tiger_gen2_live.blend --python tools/tiger_production/export_gen2_glb.py -- OUT_DIR

Writes OUT_DIR/tiger_gen2.glb (every part, fastener, game addition and rig empty, in the rest pose
of frame 0, materials as flat colours, custom properties as extras), OUT_DIR/tiger_gen2_tools.glb
(the repair tools and hands) and OUT_DIR/tiger_gen2_export.json (counts, bounds, frames).

Frames. The rig file is metres, +Z up, -Y forward, +X the tank's left. glTF is +Y up, so a point
(x, y, z) there is (x, z, -y) here: +Z forward, +X left, +Y up, the same axes as the game's
present Tiger. The rig and repair JSON files stay in the rig file's frame; a loader applies the
same mapping. Nothing is scaled: the game's present Tiger is 1.112 times this.

Left out: TO_REMOVE (gun basket floor and its fittings), the repair player and character rigs,
kit parts that overlap game additions, the cable guide curves.
"""
import json
import os
import sys

import bpy
from mathutils import Vector

SKIP = ('TO_REMOVE', 'REPAIR_PLAYER', 'CHARACTERS', 'GAME_ADDITIONS_OVERLAP_KIT', 'CABLE_GUIDES', 'REPAIR_TOOLS')
OLD_GAME_FIT = {'scale': 1.1119990859940547, 'translate_rig_frame': [-0.00874, -1.12276, -0.10601]}


def members(names):
    out = set()
    for n in names:
        c = bpy.data.collections.get(n)
        if c is not None:
            out.update(c.all_objects)
    return out


def bounds(objs):
    lo, hi, tris = Vector((1e9,) * 3), Vector((-1e9,) * 3), 0
    for o in objs:
        if o.type != 'MESH':
            continue
        o.data.calc_loop_triangles()
        tris += len(o.data.loop_triangles)
        for c in o.bound_box:
            p = o.matrix_world @ Vector(c)
            lo = Vector(map(min, lo, p))
            hi = Vector(map(max, hi, p))
    return [round(v, 4) for v in lo], [round(v, 4) for v in hi], tris


def export(path, objs):
    bpy.ops.object.select_all(action='DESELECT')
    for o in bpy.data.objects:
        o.hide_set(False)
        o.hide_viewport = False
    n = 0
    for o in objs:
        if o.name in bpy.context.view_layer.objects:
            o.select_set(True)
            n += 1
    bpy.ops.export_scene.gltf(filepath=path, export_format='GLB', use_selection=True, export_yup=True, export_apply=False,
                              export_animations=False, export_extras=True, export_materials='EXPORT', export_cameras=False,
                              export_lights=True, export_normals=True, export_texcoords=False, export_tangents=False,
                              export_skins=False, export_morph=False)
    return n


def main():
    out = sys.argv[sys.argv.index('--') + 1]
    os.makedirs(out, exist_ok=True)
    sc = bpy.context.scene
    sc.frame_set(0)
    for lc in bpy.context.view_layer.layer_collection.children:
        lc.exclude = False
    for c in bpy.data.collections:
        c.hide_viewport = False
    bpy.context.view_layer.update()
    skip = members(SKIP)
    tank = [o for o in sc.objects if o not in skip and o.type in ('MESH', 'EMPTY', 'LIGHT') and not o.name.startswith(('PLAYER.', 'HAND.', 'TOOL.'))]
    tools = [o for o in members(('REPAIR_TOOLS',)) if o.type in ('MESH', 'EMPTY')] + [o for o in sc.objects if o.name.startswith('HAND.') and o.type == 'MESH']
    lo, hi, tris = bounds(tank)
    kit = [o for o in tank if o.type == 'MESH' and 'kit_name' in o]
    klo, khi, ktris = bounds(kit)
    report = {'format': 'tiger_gen2_export.v1', 'source': bpy.data.filepath, 'true_size': True,
              'rig_frame': 'metres, +Z up, -Y forward, +X the tank\'s left',
              'glb_frame': 'metres, +Y up, +Z forward, +X the tank\'s left; glb (x, y, z) = rig (x, z, -y)',
              'old_game_tiger_from_rig': OLD_GAME_FIT,
              'tank': {'file': 'tiger_gen2.glb', 'objects': export(os.path.join(out, 'tiger_gen2.glb'), tank),
                       'meshes': sum(1 for o in tank if o.type == 'MESH'), 'triangles': tris, 'rig_min': lo, 'rig_max': hi,
                       'kit_parts_rig_min': klo, 'kit_parts_rig_max': khi, 'kit_triangles': ktris,
                       'controls': sum(1 for o in tank if o.name.startswith('CTRL.')),
                       'fasteners': sum(1 for o in tank if o.name.startswith('FASTENER.'))},
              'tools': {'file': 'tiger_gen2_tools.glb', 'objects': export(os.path.join(out, 'tiger_gen2_tools.glb'), tools),
                        'names': sorted(o.name for o in tools)},
              'left_out': {n: len(members((n,))) for n in SKIP}}
    with open(os.path.join(out, 'tiger_gen2_export.json'), 'w') as fh:
        json.dump(report, fh, indent=1)
    print('EXPORT_DONE', json.dumps(report['tank']))


main()
