"""Running gear of the gen2 Tiger rig: swing arms, road wheels, sprockets, idlers, final drives,
track joints and spare links (destructibles rows A1 to A7). Positions are taken from the kit
assemblies in the open file at build time; how each is read is on the control as `source`.

    sys.path.append(<this dir>); import running_gear_rig as rg
    rg.build_stations('L'); rg.build_stations('R'); rg.build_drive(); rg.build_tracks(); rg.fasten(census)

What the kit shows (read 2026-11-08):
- 16 stations, eight a side, assemblies 0B17 and 0B18 alternating: one arm 18D57 (the arm and its
  torsion bar across the hull in one part, the assembly's origin on the bar's axis), three wheel
  discs on one axle and a washer. Stations are numbered 1 to 8 from the front on each side; discs
  are `in`, `mid`, `out` from the hull outward.
- Sprockets 0B19 (two discs, a hub cap, a ring), idlers 0B20 (two discs and a ring), final drive
  boxes 29D56 behind covers 18-22 / 18-23.
- 96 track units a side (0B28: link 15N90, pin 37N01, two guide horns 15N03), in loop order.

Named approximations:
- An arm and its torsion bar turn as one rigid part about the bar's axis (the real bar twists).
- The idler has no crank arm in the kit: track tension is a straight slide of the idler along the
  hull, 0 to 0.08 m rearward, travel not verified. The adjuster behind the rear plate's conical
  cap is a socket at the cap's middle; the cap is still part of the plate.
- The sprocket's tooth ring is not cut from its disc.
- One joint a side is rigged as the place a track is broken (the rearmost link, on the idler):
  two pins that slide out and the link between them. Every other link has the same layout and is
  tagged with its side and place in the loop for the game.
"""
import bpy
import numpy as np
from mathutils import Vector

import rig_build
import rig_lib
import split_lib

PRE = 'TG_RM5010-'
ROOT = '0A_Tiger I'


def _centre(o):
    w = split_lib.world_verts(o)
    return Vector((w.min(0) + w.max(0)) / 2.0)


def _short(o):
    return o.name.replace(PRE, '').replace('_îAÔO', '')


def stations():
    """Every wheel station: side, number from the front, its assembly, arm, discs in to out, washer,
    the bar's axis point and the axle point."""
    out = []
    asms = [o for o in bpy.data.objects if o.type == 'EMPTY' and o.name.startswith((PRE + '0B17', PRE + '0B18'))]
    for side, sign in (('L', 1.0), ('R', -1.0)):
        mine = sorted([a for a in asms if a.matrix_world.translation.x * sign > 0], key=lambda a: a.matrix_world.translation.y)
        for i, a in enumerate(mine):
            kids = [c for c in a.children_recursive if c.type == 'MESH' and not c.name.startswith('FASTENER')]
            arm = next(c for c in kids if '18D57' in c.name)
            washer = next(c for c in kids if '30D48' in c.name)
            discs = sorted([c for c in kids if 'ROLLOR' in c.name], key=lambda c: abs(_centre(c).x))
            axle = _centre(discs[1])
            out.append({'id': '%s%d' % (side, i + 1), 'side': side, 'sign': sign, 'asm': a, 'arm': arm, 'washer': washer, 'discs': discs,
                        'pivot': a.matrix_world.translation.copy(), 'axle': Vector((a.matrix_world.translation.x, axle.y, axle.z))})
    return out


def build_stations(side, only=None):
    reports = []
    for s in stations():
        if s['side'] != side or (only and s['id'] not in only):
            continue
        sid = s['id']
        lead = s['axle'].y < s['pivot'].y           # the wheel sits ahead of the bar
        up_axis = (-1.0, 0.0, 0.0) if lead else (1.0, 0.0, 0.0)   # positive angle lifts the wheel
        arm = rig_build.build_one(dict(
            name='Arm_' + sid, kind='hinge', pivot=tuple(s['pivot']), axis=up_axis, parent=None, parts=[_short(s['arm'])],
            source='axis = the torsion bar\'s (origin of kit assembly %s), across the hull; positive lifts the wheel; arm and bar turn as one rigid part (named approximation)' % _short(s['asm']),
            interaction='the station rides up and down on its torsion bar', damage='arm bent or the bar snapped: the station sags',
            repair='chain the arm up clear of the track (concession)'))
        reports.append(arm)
        spin = rig_build.build_one(dict(
            name='Wheel_' + sid, kind='hinge', pivot=tuple(s['axle']), axis=(1, 0, 0), parent='CTRL.Arm_' + sid, parts=[_short(s['washer'])],
            continuous=True, source='axle = middle of the station\'s discs',
            interaction='the three discs of the station turn together', damage='a disc shattered or its tyre burnt off', repair='jack the arm, unbolt and drop the wrecked disc'))
        reports.append(spin)
        names = ('in', 'mid', 'out')
        for k, disc in enumerate(s['discs']):
            held = ('fastener set wheel_%s_%s' % (sid, names[k])) + ('' if k == 2 else '; Wheel_%s_%s off first' % (sid, names[k + 1]))
            c = _centre(disc)
            reports.append(rig_build.build_one(dict(
                name='Wheel_%s_%s' % (sid, names[k]), kind='stow', pivot=tuple(c), parent='CTRL.Wheel_' + sid, parts=[_short(disc)],
                released_by=held, source='seat = disc %s on its axle in the kit pose' % _short(disc).split('.')[0],
                props={'draw_axis': 'x', 'draw_sign': s['sign']},
                interaction='undo the nuts, draw the disc off the axle outward', damage='disc shattered, tyre burnt or torn off', repair='drop it and run without it, or fit one from a wreck')))
        # the arm's travel, with its wheels on it, against everything but the track it stands on
        e = bpy.data.objects['CTRL.Arm_' + sid]
        ignore = [m for a in bpy.data.objects if a.type == 'EMPTY' and a.name.startswith(PRE + '0B28') for m in rig_build.under(a)]
        got = rig_lib.sweep(e, -25.0, 35.0, step=1.0, radius=0.5, ignore=ignore)
        why = []
        for d in ('min', 'max'):
            hit = got.get(d + '_hit', (None, []))
            why.append('%s %g (%s)' % (d, got[d], 'stopped by ' + ', '.join(hit[1]) if hit[0] is not None else 'nothing in the way'))
        rig_lib.limits(e, got['min'], got['max'], 'zero = the kit pose, wheel on the track; %s; 1 deg sweep with the wheels on, the track ignored' % '; '.join(why))
        arm['sweep'] = {k: got[k] for k in got if k != 'ignored_rest_contacts'}
    return reports


def build_drive():
    """Sprockets, idlers with their tension slide, final drive boxes and covers."""
    reports = []
    D = bpy.data.objects
    for side, sign in (('L', 1.0), ('R', -1.0)):
        sp = next(a for a in D if a.type == 'EMPTY' and a.name.startswith(PRE + '0B19') and a.matrix_world.translation.x * sign > 0)
        kids = [c for c in sp.children_recursive if c.type == 'MESH' and not c.name.startswith('FASTENER')]
        cap = next(c for c in kids if '29D52' in c.name)
        axis_pt = sp.matrix_world.translation.copy()
        reports.append(rig_build.build_one(dict(
            name='Sprocket_' + side, kind='hinge', pivot=tuple(axis_pt), axis=(1, 0, 0), parent=None, parts=[_short(c) for c in kids if c is not cap],
            continuous=True, source='axis = origin of kit assembly 0B19, across the hull; tooth ring not cut from its disc',
            interaction='drives the track', damage='tooth ring cracked or teeth sheared: the track slips', repair='unbolt the ring and turn it to unworn teeth (concession; ring not separate yet)')))
        reports.append(rig_build.build_one(dict(
            name='Sprocket_Cap_' + side, kind='stow', pivot=tuple(_centre(cap)), parent='CTRL.Sprocket_' + side, parts=[_short(cap)],
            released_by='fastener set sprocket_cap_' + side, source='seat = hub cap 29D52 in the kit pose', props={'draw_axis': 'x', 'draw_sign': sign},
            interaction='undo the screws, draw the hub cap', damage='cap shot off', repair='none needed')))
        idl = next(a for a in D if a.type == 'EMPTY' and a.name.startswith(PRE + '0B20') and a.matrix_world.translation.x * sign > 0)
        ikids = [c for c in idl.children_recursive if c.type == 'MESH' and not c.name.startswith('FASTENER')]
        ipt = idl.matrix_world.translation.copy()
        reports.append(rig_build.build_one(dict(
            name='Idler_Tension_' + side, kind='slide', pivot=tuple(ipt), axis=(0, 1, 0), parent=ROOT, parts=[],
            limits=(0.0, 0.08, 'zero = the kit pose; 0.08 m rearward is chosen: the kit has no idler crank and the real travel is not verified (named approximation)'),
            source='the idler slides along the hull in place of swinging on its crank', interaction='track tension: the idler moves rearward as the adjuster is turned',
            damage='adjuster bent, idler cracked: slack track', repair='re-tension at the rear plate with the track wrench')))
        reports.append(rig_build.build_one(dict(
            name='Idler_' + side, kind='hinge', pivot=tuple(ipt), axis=(1, 0, 0), parent='CTRL.Idler_Tension_' + side, parts=[_short(c) for c in ikids],
            continuous=True, source='axis = origin of kit assembly 0B20', interaction='turns with the track')))
        box = next(o for o in D if o.type == 'MESH' and '29D56_DRIVER' in o.name and _centre(o).x * sign > 0)
        cover = next(o for o in D if o.type == 'MESH' and ('18-22_COVER' in o.name or '18-23_COVER' in o.name) and _centre(o).x * sign > 0)
        reports.append(rig_build.build_one(dict(
            name='Final_Drive_' + side, kind='stow', pivot=tuple(_centre(box)), parent=None, parts=[_short(box)],
            released_by='fastener set final_drive_%s (15 bolts); Sprocket_%s off first' % (side, side), source='seat = final drive box 29D56 in the kit pose',
            interaction='a workshop job: the box comes off its 15 bolts', damage='housing cracked, gear stripped: no drive on that side', repair='none in the field: tow')))
        reports.append(rig_build.build_one(dict(
            name='Final_Drive_Cover_' + side, kind='stow', pivot=tuple(_centre(cover)), parent=None, parts=[_short(cover)],
            released_by='not read: no heads found on the cover', source='seat = cover %s in the kit pose' % _short(cover),
            interaction='armour cover over the final drive', damage='holed or torn off', repair='none')))
    return reports


def tensioner_sockets():
    """The conical caps low on the rear plate: a socket at each cap's tip for the track wrench."""
    plate = next(o for o in bpy.data.objects if o.type == 'MESH' and '30Z02_COVER' in o.name)
    w = split_lib.world_verts(plate)
    n = np.array((0.0, 0.9901, -0.1402))
    reports = []
    for side, sign in (('L', 1.0), ('R', -1.0)):
        m = (w[:, 0] * sign > 0.6) & (w[:, 2] < 1.0)
        pts = w[m]
        h = pts @ n
        tip = pts[h > h.max() - 0.004].mean(0)
        reports.append(rig_build.build_one(dict(
            name='Track_Tensioner_' + side, kind='socket', pivot=tuple(tip), axis=tuple(-n), accepts='TOOL.Track_Wrench', parent='0B24_Back Cover Unit',
            source='tip of the conical cap low on the rear plate, axis along the plate\'s normal; the cap is still part of plate 30Z02 and the adjuster behind it is not in the kit',
            props={'drives': 'Idler_Tension_' + side, 'metres_per_turn': 0.004}, requires='cap off (not cut yet)',
            interaction='turn the adjuster with the track wrench: the idler moves and the track tightens (4 mm a turn is a placeholder, not verified)',
            damage='adjuster bent by a hit low on the rear', repair='hammer and wrench')))
        reports[-1]['tip'] = [round(float(x), 3) for x in tip]
    return reports


def track_links():
    """Per side, the track units in loop order."""
    units = sorted([o for o in bpy.data.objects if o.type == 'EMPTY' and o.name.startswith(PRE + '0B28_Track Unit')], key=lambda o: o.name)
    return {'L': [u for u in units if u.matrix_world.translation.x > 0], 'R': [u for u in units if u.matrix_world.translation.x < 0]}


def build_tracks():
    reports = []
    D = bpy.data.objects
    root = next(o for o in D if o.type == 'EMPTY' and o.name.startswith(PRE + ROOT))
    for side, units in track_links().items():
        sign = 1.0 if side == 'L' else -1.0
        for i, u in enumerate(units):
            u['track_side'], u['track_index'], u['track_count'] = side, i, len(units)
        name = 'GUIDE.Track_' + side
        old = D.get(name)
        if old is not None:
            cu = old.data
            D.remove(old, do_unlink=True)
            bpy.data.curves.remove(cu)
        cu = bpy.data.curves.new(name, 'CURVE')
        cu.dimensions = '3D'
        sp = cu.splines.new('POLY')
        sp.points.add(len(units) - 1)
        for p, u in zip(sp.points, units):
            t = u.matrix_world.translation
            p.co = (t.x, t.y, t.z, 1.0)
        sp.use_cyclic_u = True
        ob = D.new(name, cu)
        (bpy.data.collections.get('CABLE_GUIDES') or bpy.context.scene.collection).objects.link(ob)
        ob['links'] = len(units)
        ob['pitch_m'] = float(np.median([(units[i].matrix_world.translation - units[i - 1].matrix_world.translation).length for i in range(1, len(units))]))
        ob['source'] = 'through the origins of the 0B28 track units in loop order (their pin ends)'
        # the joint to break: the rearmost unit and the next one in the loop
        k = max(range(len(units)), key=lambda i: units[i].matrix_world.translation.y)
        a, b = units[k], units[(k + 1) % len(units)]
        for tag, u in (('a', a), ('b', b)):
            pin = next(c for c in u.children if 'PIN' in c.name)
            w = split_lib.world_verts(pin)
            length = float(w[:, 0].max() - w[:, 0].min())
            reports.append(rig_build.build_one(dict(
                name='Track_Pin_%s_%s' % (side, tag), kind='slide', pivot=tuple(_centre(pin)), axis=(sign, 0, 0), parent=ROOT, parts=[_short(pin)],
                limits=(0.0, round(length + 0.03, 3), 'zero = the kit pose; fully drawn = the pin\'s own length (%.3f m) and 30 mm' % length),
                source='pin 37N01 of track unit %d (%s side), drawn outward along its own axis' % (u['track_index'], side),
                props={'track_index': int(u['track_index'])},
                interaction='drive the pin out with hammer and drift', damage='pin sheared or bent', repair='drive it out, fit a spare')))
        link_parts = [_short(c) for c in a.children if c.type == 'MESH' and 'PIN' not in c.name and not c.name.startswith('FASTENER')]
        link = next(c for c in a.children if 'TRACK' in c.name)
        reports.append(rig_build.build_one(dict(
            name='Track_Link_' + side, kind='stow', pivot=tuple(_centre(link)), parent=ROOT, parts=link_parts,
            released_by='Track_Pin_%s_a and Track_Pin_%s_b drawn' % (side, side), source='track unit %d (%s side): link 15N90 and its two guide horns' % (a['track_index'], side),
            props={'track_index': int(a['track_index'])},
            interaction='lift the broken link out of the run', damage='link cracked or broken by a hit or a mine', repair='fit a spare link, drive both pins home')))
        reports[-1]['unit'] = int(a['track_index'])
    return reports


def build_spare_links():
    """Spare links on the turret and bow (0C10): each link lifts out of its two brackets."""
    reports = []
    asm = [a for a in bpy.data.objects if a.type == 'EMPTY' and a.name.startswith(PRE + '0C10')]
    links = sorted([c for a in asm for c in a.children_recursive if c.type == 'MESH' and '_TRACK' in c.name], key=lambda c: (round(_centre(c).x, 1), _centre(c).y))
    for i, link in enumerate(links):
        reports.append(rig_build.build_one(dict(
            name='Spare_Link_%d' % (i + 1), kind='stow', pivot=tuple(_centre(link)), parent=None, parts=[link.name],
            released_by='lifted out of its two brackets 15F35 (the brackets do not open in the kit)', source='spare link %s in its kit pose' % _short(link),
            interaction='lift the spare link off its brackets', damage='shot off the turret side', repair='none: pick links up from the ground or a wreck')))
    return reports


def _set_of(o):
    """A fastener's set from the control its host hangs under: 'CTRL.Wheel_L3_out' -> 'wheel_L3_out'."""
    import re
    p = o
    while p is not None:
        if p.name.startswith('CTRL.'):
            return '_'.join(t if re.fullmatch(r'[LR]\d*', t) else t.lower() for t in p.name[5:].split('_'))
        p = p.parent
    return ''


def fasten(census_path):
    """Stand fastener objects on the running gear from a census file (fastener_census.py)."""
    import json

    import fasteners

    def hexes(lo, hi):
        return lambda r: ('bolt', True) if r['shape'] == 'hex' and lo <= r['size'] * 1000.0 <= hi else None

    def rounds(lo, hi, h):
        return lambda r: ('screw', True) if r['shape'] == 'round' and lo <= r['size'] * 1000.0 <= hi and r['height'] * 1000.0 <= h else None

    policy = {'29C02_ROLLOR': hexes(17, 21), '29C03_ROLLOR': hexes(26, 30), '30C05_ROLLOR': hexes(26, 30), '30C06_ROLLOR': hexes(26, 30),
              '29D56_DRIVER': hexes(26, 30), '18D57_ROLLAR': hexes(30, 34), '29D52_PIN': rounds(19, 23, 6), '29D02_GUIDE': rounds(16, 23, 9)}
    data = json.load(open(census_path))
    reports, cut = [], set()
    for key, kind_of in policy.items():
        rows = next((v['rows'] for v in data.values() if (PRE + key) in v['object']), None)
        objs = sorted([o for o in bpy.data.objects if o.type == 'MESH' and (PRE + key) in o.name], key=lambda o: o.name)
        made = cuts = 0
        for o in objs:
            if any(c.name.startswith('FASTENER.') for c in o.children):
                continue
            rep = fasteners.apply_rows(o, rows, kind_of, set_name=_set_of(o), cut=o.data.name not in cut)
            cut.add(o.data.name)
            made += rep['objects_made']
            cuts += rep.get('cut', 0)
            if rep.get('ring_open') or rep.get('not_found'):
                reports.append({'object': o.name, 'ring_open': rep.get('ring_open'), 'not_found': rep.get('not_found')})
        reports.append({'part': key, 'objects': len(objs), 'heads_cut_per_mesh': cuts, 'fasteners_made': made})
    return reports
