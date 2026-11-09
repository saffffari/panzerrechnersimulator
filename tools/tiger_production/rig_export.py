"""Write the rig's controls and fastener objects as one table the game can read.

    import rig_export; rig_export.write('/path/tiger_gen2_rig.json')

Controls: name, kind (hinge, slide, stow), the control it hangs under, its frame in the tank's
space at rest (position, and for a hinge or slide the axis), limits, the play notes, and the kit
parts it carries. Fasteners: name, set, kind, head shape and size, seat position and the way out,
host part. Track guides: the loop of unit origins per side. Space: the rig file's world (metres,
+Z up, -Y forward, +X the tank's left).
"""
import json
import math

import bpy
from mathutils import Vector

KEEP = ('rig_kind', 'unit', 'min', 'max', 'closed', 'continuous', 'accepts', 'released_by', 'locked_by', 'requires', 'source', 'limit_source',
        'interaction', 'damage', 'repair', 'drives', 'stowed_min', 'stowed_max', 'max_turret_forward', 'throw_m', 'tool_kind', 'track_index',
        'unscrew', 'metres_per_turn', 'kit_sweep_min', 'kit_sweep_max', 'draw_axis', 'draw_sign', 'rope', 'guide')


def _v(x):
    return [round(float(c), 5) for c in x]


def _owner(o):
    p = o.parent
    while p is not None:
        if p.name.startswith('CTRL.'):
            return p.name[5:]
        p = p.parent
    return ''


def table():
    D = bpy.data.objects
    bpy.context.scene.frame_set(1)
    controls = []
    for e in sorted([o for o in D if o.name.startswith('CTRL.')], key=lambda o: o.name):
        fr = e.parent if e.parent is not None and e.parent.name.startswith('FRAME.') else e
        m = fr.matrix_world
        row = {'name': e.name[5:], 'under': _owner(fr), 'position': _v(m.translation)}
        if e.get('rig_kind') in ('hinge', 'slide'):
            row['axis'] = _v((m.to_3x3() @ Vector((1.0, 0.0, 0.0))).normalized())
        for k in KEEP:
            if k in e:
                v = e[k]
                row[k] = v if isinstance(v, (int, float, str, bool)) else list(v)
        parts = []
        stack = list(e.children)
        while stack:
            c = stack.pop()
            if c.name.startswith(('FRAME.', 'CTRL.')):
                continue
            if c.type == 'MESH' and not c.name.startswith('FASTENER.'):
                parts.append(c.name)
            stack.extend(c.children)
        row['parts'] = sorted(parts)
        controls.append(row)
    fasteners = []
    col = bpy.data.collections.get('FASTENERS')
    for o in sorted(col.objects if col else [], key=lambda o: o.name):
        m = o.matrix_world
        fasteners.append({'name': o.name, 'set': o.get('fastener_set', ''), 'kind': o.get('fastener_kind', ''), 'shape': o.get('fastener_shape', ''),
                          'size_m': round(float(o.get('fastener_size_m', 0.0)), 5), 'height_m': round(float(o.get('fastener_height_m', 0.0)), 5),
                          'removable': bool(o.get('fastener_removable', False)), 'tool': o.get('fastener_tool', ''), 'thread_guess': o.get('fastener_thread_guess', ''),
                          'seat': _v(m.translation), 'out': _v((m.to_3x3() @ Vector((0.0, 0.0, 1.0))).normalized()), 'host': o.get('fastener_host', ''), 'under': _owner(o)})
    guides = {}
    for o in D:
        if o.name.startswith('GUIDE.') and o.type == 'CURVE':
            sp = o.data.splines[0]
            guides[o.name[6:]] = {'cyclic': bool(sp.use_cyclic_u), 'points': [_v((o.matrix_world @ Vector(p.co[:3]))) for p in sp.points],
                                  'props': {k: (o[k] if isinstance(o[k], (int, float, str, bool)) else str(o[k])) for k in o.keys() if not k.startswith('_')}}
    return {'format': 'tiger_rig.v1', 'source': bpy.data.filepath, 'frame': 'metres, +Z up, -Y forward, +X the tank\'s left',
            'controls': controls, 'fasteners': fasteners, 'guides': guides,
            # readable object name -> the kit's own name (rename_parts.py), so a part can be traced to the kit
            'kit_names': {o.name: o['kit_name'] for o in D if 'kit_name' in o}}


def write(path):
    t = table()
    with open(path, 'w') as fh:
        json.dump(t, fh, separators=(',', ':'))
    kinds = {}
    for c in t['controls']:
        kinds[c.get('rig_kind', '?')] = kinds.get(c.get('rig_kind', '?'), 0) + 1
    return {'path': path, 'controls': len(t['controls']), 'by_kind': kinds, 'fasteners': len(t['fasteners']), 'guides': len(t['guides'])}
