"""Build rig controls from plain specs (one dict per mechanism), so that every control in the gen2
Tiger file can be rebuilt from the repository and read in one place.

    sys.path.append(<this dir>); import rig_build, rig_specs; rig_build.build(rig_specs.DECK)

A spec:
    name      'Deck_Hatch' -> FRAME.Deck_Hatch + CTRL.Deck_Hatch
    kind      'hinge' | 'slide' | 'stow' | 'socket'
    pivot     world point, metres (how it was read goes in `source`)
    axis      world direction of the hinge or slide (ignored for stow)
    parts     short kit names ('34Z03_COVER', '34D28_ARM.001') and/or 'CTRL.<Name>' of controls
              that ride on this one (their FRAME is re-parented)
    parent    'CTRL.<Name>', a kit assembly key ('0B26_Cover Unit'), or omitted (= where the first
              part hangs now)
    sweep     (lo, hi, step_deg_or_m) to find limits by collision, optional `ignore` keys
    limits    (lo, hi, 'why') to set them outright
    continuous, accepts, props {...}, interaction, damage, repair, released_by, locked_by, requires
"""
import bpy
from mathutils import Vector

import rig_lib

PRE = 'TG_RM5010-'
SUF = '_îAÔO'


def obj(short):
    """'34D28_ARM.001' -> the object TG_RM5010-34D28_ARM_îAÔO.001; 'CTRL.x' and full names pass."""
    if short in bpy.data.objects:
        return bpy.data.objects[short]
    stem, dot, num = short.partition('.')
    if dot and not num.isdigit():
        stem, num, dot = short, '', ''
    name = PRE + stem + SUF + (dot + num if dot else '')
    o = bpy.data.objects.get(name)
    if o is None:
        raise KeyError('no object %s (%s)' % (short, name))
    return o


def assembly(key):
    return next(o for o in bpy.data.objects if o.type == 'EMPTY' and (PRE + key) in o.name)


def under(root):
    return [o for o in root.children_recursive if o.type == 'MESH']


def build_one(s):
    parts, riders = [], []
    for p in s.get('parts', ()):
        if p.startswith('CTRL.'):
            riders.append(bpy.data.objects['FRAME.' + p[5:]])
        else:
            parts.append(obj(p))
    par = s.get('parent')
    if par is None:
        first = (parts or riders)[0]
        parent = first.parent
        # a part already under this very control: keep the control's present parent
        if parent is not None and parent.name == 'CTRL.' + s['name']:
            parent = bpy.data.objects['FRAME.' + s['name']].parent
    elif par.startswith('CTRL.'):
        parent = bpy.data.objects[par]
    else:
        parent = assembly(par)
    kind = s['kind']
    src = s.get('source', '')
    if kind == 'stow':
        e = rig_lib.make_stow(s['name'], s['pivot'], parts, parent=parent, source=src, released_by=s.get('released_by', ''))
    elif kind == 'socket':
        e = rig_lib.make_socket(s['name'], s['pivot'], s['axis'], s['accepts'], parent=parent, source=src)
    else:
        e = rig_lib.make_joint(s['name'], kind, s['pivot'], s['axis'], parts, parent=parent, source=src, size=s.get('size', 0.12))
    for fr in riders:
        rig_lib.reparent(fr, e)
    for k in ('continuous', 'locked_by', 'requires', 'released_by'):
        if k in s and not (kind == 'stow' and k == 'released_by'):
            e[k] = s[k]
    for k, v in s.get('props', {}).items():
        e[k] = v
    rig_lib.gameplay(e, s.get('interaction', ''), s.get('damage', ''), s.get('repair', ''))
    report = {'name': e.name, 'kind': kind, 'parts': len(parts), 'riders': len(riders)}
    if 'sweep' in s:
        lo, hi, step = s['sweep']
        ignore = []
        for k in s.get('ignore', ()):
            if k.startswith('='):
                ignore.append(obj(k[1:]))
            else:  # every assembly whose kit number starts with the key
                for a in bpy.data.objects:
                    if a.type == 'EMPTY' and a.name.startswith(PRE + k):
                        ignore += under(a)
        got = rig_lib.sweep(e, lo, hi, step=step, radius=s.get('radius', 0.6), ignore=ignore)
        why = []
        for d in ('min', 'max'):
            hit = got.get(d + '_hit', (None, []))
            why.append('%s %g%s' % (d, got[d], ' (stopped by %s)' % ', '.join(hit[1]) if hit[0] is not None else ' (nothing in the way to %g)' % (lo if d == 'min' else hi)))
        note = 'zero = the kit pose; %s; %g step sweep' % ('; '.join(why), step)
        if s.get('ignore'):
            note += '; ignoring ' + ', '.join(s['ignore'])
        if s.get('continuous'):
            e['stowed_min'], e['stowed_max'], e['limit_source'] = float(got['min']), float(got['max']), note
        else:
            rig_lib.limits(e, got['min'], got['max'], note)
        report['sweep'] = {k: got[k] for k in got if k != 'ignored_rest_contacts'}
    if 'limits' in s:
        lo, hi, why = s['limits']
        rig_lib.limits(e, lo, hi, why)
        report['limits'] = (lo, hi)
    return report


def build(specs):
    return [build_one(s) for s in specs]


def tag_fastener(short, kind, set_name, removable=True, tool='', shape=''):
    """An existing kit part that is itself a fastener (a wing nut, a cap): put it with the fastener
    objects and give it the same properties. Its origin and local +Z must already be the seat and
    the way out."""
    o = obj(short)
    col = bpy.data.collections.get('FASTENERS')
    if col is None:
        col = bpy.data.collections.new('FASTENERS')
        bpy.context.scene.collection.children.link(col)
    if o.name not in col.objects:
        col.objects.link(o)
    o['fastener_kind'] = kind
    o['fastener_shape'] = shape or kind
    o['fastener_removable'] = bool(removable)
    o['fastener_set'] = set_name
    o['fastener_host'] = o.parent.name if o.parent else ''
    o['fastener_is_kit_part'] = True
    if tool:
        o['fastener_tool'] = tool
    d = o.dimensions
    o['fastener_size_m'] = float(max(d.x, d.y))
    o['fastener_height_m'] = float(d.z)
    return o.name
