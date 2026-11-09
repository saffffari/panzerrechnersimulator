"""Pioneer tools, their clamps and the tow cables on the hull roof (destructibles rows F1, F2).

    sys.path.append(<this dir>); import tools_rig as tr; tr.build()

What the kit shows (read 2026-11-08): axe 32O12, shovel 34O26, sledge hammer 35O21, a 1.79 m bar
32O06 (taken for the crowbar), wire cutters 32O20 and the extinguisher 34B67 lie on the roof in
seven clamps B76 (two-coloured: a dark strap over a light base with a wire loop). Each tow cable
35C45CM-L / -R runs in a U along its side with both end eyes 34B62 on posts 35D17 at the front
and two clamps 35D12 along the run.

Named approximations:
- A clamp's strap is cut from its base where the kit changes colour and swings on a pin put at
  the end away from the wire loop, about the tool's own direction. The wire loop stays on the base.
- A tow cable is one rigid part with its two eyes. It does not bend in the rig: the guide curves
  (GUIDE.*_cable) are what the game's rope starts from. Tow points are sockets at the shackles.
- The cable posts and the cable clamps do not open.
"""
import bpy
import numpy as np
from mathutils import Matrix, Vector

import rig_build
import rig_lib
import split_lib

PRE = 'TG_RM5010-'
TOOLS = [  # control, kit key, what it is
    ('Tool_Axe', '32O12_AXE', 'axe'), ('Tool_Shovel', '34O26_SHOVEL', 'shovel'), ('Tool_Hammer', '35O21_HAMMER', 'sledge hammer'),
    ('Tool_Crowbar', '32O06_TOOL', 'crowbar'), ('Tool_Wire_Cutters', '32O20_PLIERS', 'wire cutters')]


def _centre(o):
    w = split_lib.world_verts(o)
    return Vector((w.min(0) + w.max(0)) / 2.0)


def _short(o):
    return o.name.replace(PRE, '').replace('_îAÔO', '')


def _bbox(o):
    w = split_lib.world_verts(o)
    return w.min(0), w.max(0)


def build_tools():
    D = bpy.data.objects
    reports = []
    clamps = sorted([o for o in D if o.type == 'MESH' and (PRE + 'B76_LOCK') in o.name], key=lambda o: o.name)
    straps, rep = split_lib.split_shared(clamps, split_lib.by_material('mat_3'), 'B76S_STRAP', 'tool clamp base with its wire loop', 'tool clamp strap')
    reports.append({'clamp_split': rep})
    tools = {}
    for ctrl, key, what in TOOLS:
        o = next(x for x in D if x.type == 'MESH' and (PRE + key) in x.name)
        tools[ctrl] = o
    # each clamp belongs to the tool whose box it sits in
    # each clamp belongs to the tool whose handle line it sits on (two tools can overlap in plan)
    owner = {}
    lines = {}
    for ctrl, o in tools.items():
        w = split_lib.world_verts(o)
        lo, hi = w.min(0), w.max(0)
        long = 0 if (hi - lo)[0] > (hi - lo)[1] else 1
        lines[ctrl] = (long, lo[long], hi[long], float(np.median(w[:, 1 - long])))
    for c, s in zip(clamps, straps):
        p = np.array(_centre(c))
        best = None
        for ctrl, (long, a, b, across) in lines.items():
            d = abs(p[1 - long] - across) + max(0.0, a - p[long], p[long] - b) * 10.0
            if best is None or d < best[0]:
                best = (d, ctrl)
        owner.setdefault(best[1], []).append((c, s))
    for ctrl, key, what in TOOLS:
        o = tools[ctrl]
        mine = sorted(owner.get(ctrl, []), key=lambda cs: (round(_centre(cs[0]).x, 2), _centre(cs[0]).y))
        names = ['%s_Clamp_%d' % (ctrl, i + 1) for i in range(len(mine))]
        reports.append(rig_build.build_one(dict(
            name=ctrl, kind='stow', pivot=tuple(_centre(o)), parent=None, parts=[_short(o)],
            released_by=', '.join(names) if names else 'nothing: it lies loose in the kit',
            source='seat = the %s %s in its kit pose on the roof' % (what, key.split('_')[0]),
            props={'tool_kind': what}, interaction='open its clamps and lift the %s off the roof' % what,
            damage='shot off, burnt or left behind', repair='take one from a wreck or the depot')))
        for (c, s), nm in zip(mine, names):
            m = c.matrix_world
            pin = m @ Vector((0.0, -0.062, -0.005))
            axis = (m.to_3x3() @ Vector((1.0, 0.0, 0.0))).normalized()
            reports.append(rig_build.build_one(dict(
                name=nm, kind='hinge', pivot=tuple(pin), axis=tuple(axis), parent=None, parts=[s.name],
                sweep=(0.0, 150.0, 2.0), radius=0.25, ignore=['=' + o.name],
                source='strap of clamp B76 cut from its base at the colour change; pin put at the end away from the wire loop, along the tool (named approximation)',
                interaction='slip the wire loop, swing the strap up', damage='bent shut or torn off', repair='lever it open with another tool')))
    jar = next(x for x in D if x.type == 'MESH' and (PRE + '34B67_JAR') in x.name)
    reports.append(rig_build.build_one(dict(
        name='Extinguisher', kind='stow', pivot=tuple(_centre(jar)), parent=None, parts=[_short(jar)],
        released_by='its bracket strap (not separate in the kit)', source='seat = the bottle 34B67 on the engine deck in its kit pose',
        props={'tool_kind': 'extinguisher'}, interaction='pull the hand extinguisher off its bracket', damage='bottle spent or holed', repair='swap in a full bottle')))
    return reports


def build_cables():
    D = bpy.data.objects
    reports = []
    eyes = [o for o in D if o.type == 'MESH' and (PRE + '34B62_Wire Hook') in o.name]
    for side, sign, key in (('L', 1.0, '35C45CM-L_WIRE'), ('R', -1.0, '35C45CM-R_WIRE')):
        cable = next(o for o in D if o.type == 'MESH' and (PRE + key) in o.name)
        mine = sorted([e for e in eyes if _centre(e).x * sign > 0], key=lambda e: abs(_centre(e).x))
        guide = next((o for o in D if o.name.startswith('GUIDE.') and key.split('_')[0] in o.name), None)
        parts = [_short(cable)] + [_short(e) for e in mine]
        reports.append(rig_build.build_one(dict(
            name='Tow_Cable_' + side, kind='stow', pivot=tuple(_centre(cable)), parent=None, parts=parts,
            released_by='both eyes lifted off their posts 35D17; out of the two clamps 35D12 (posts and clamps do not open in the kit)',
            source='tow cable %s with its two end eyes 34B62, rigid in the rig' % key.split('_')[0],
            props={'rope': True, 'guide': guide.name if guide else '', 'eye_inner': mine[0].name, 'eye_outer': mine[1].name},
            interaction='lift the eyes off their posts and draw the cable off the roof; in the game it is a rope from the guide curve',
            damage='cut by fire or torn off', repair='splice or take one from another tank')))
        if guide is not None:
            rig_lib.reparent(guide, D['CTRL.Tow_Cable_' + side])
    # tow points: the shackles
    for i, sh in enumerate(sorted([o for o in D if o.type == 'MESH' and (PRE + '18D06_RING') in o.name], key=lambda o: (round(_centre(o).y), _centre(o).x))):
        c = _centre(sh)
        tag = ('Front' if c.y < 0 else 'Rear') + ('_L' if c.x > 0 else '_R')
        reports.append(rig_build.build_one(dict(
            name='Tow_Point_' + tag, kind='socket', pivot=tuple(c), axis=(0.0, -1.0 if c.y < 0 else 1.0, 0.0), accepts='Tow_Cable eye', parent='0A_Tiger I',
            source='middle of shackle 18D06', interaction='drop a cable eye over the shackle pin', damage='shackle torn off', repair='none')))
        rig_lib.reparent(bpy.data.objects['FRAME.Tow_Point_' + tag], sh.parent)
    return reports


def build():
    return {'tools': build_tools(), 'cables': build_cables()}
