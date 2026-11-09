"""Rear plate mechanisms of the gen2 Tiger rig (owner, 2026-11-08: rig the things on the repair
list, starting at the rear plate; split the latch from the handle; a manual crank start from the
back). Run inside the open rig file, one step at a time or all:

    sys.path.append(<this dir>); import rear_plate_rig as rp; rp.build()

What the kit shows, as read on 2026-11-08 (kit numbers under TG_RM5010-):
- 28A-O38/O32/O33 (JACK Unit 0B30) is the 15 t jack lying along the plate: a toothed rack in a
  riveted body. 28A-O39 is its gear case, 28A-O31 the crank arm with the pinion, 28A-O34 the hand
  grip standing on the arm's tip with a strap round it. The geared crank winds the jack; it is
  not the engine's starting gear.
- 28A-B69 is a domed cover with one hex head at its middle, low on the plate's centre line. It is
  taken here for the cover of the inertia starter's port. 28A-Z16 is a 0.72 m shaft with a square
  end, held in two clips low on the plate, taken here for the starting crank's shaft. Neither
  reading has been checked against a drawing or a photograph; the kit has no crank throw for it.

Named approximations (each also on its control as `source` or `limit_source`):
- The handle strap is one fused closed ring in the kit. It is cut by the plane through the grip's
  axis that faces the plate into a fixed rear jaw and a front jaw that swings on a pin at the
  strap's solid end; the catch loop beyond the grip goes whole with the front jaw. The kit shows
  no pin and no bracket.
- The starter port cover has no hinge in the kit: it is a part that comes off whole once its one
  bolt is out.
- The shaft's clips are left closed and in one piece; the shaft is simply lifted out of them.
"""
import math

import bpy
import numpy as np
from mathutils import Vector

import rig_lib
import split_lib

PRE = 'TG_RM5010-'
SUF = '_îAÔO'


def part(key):
    return next(o for o in bpy.data.objects if o.type == 'MESH' and (PRE + key) in o.name)


def empty(key):
    return next(o for o in bpy.data.objects if o.type == 'EMPTY' and (PRE + key) in o.name)


def _mat_points(obj, name):
    idx = {i for i, m in enumerate(obj.data.materials) if m.name == name}
    vs = sorted({v for p in obj.data.polygons if p.material_index in idx for v in p.vertices})
    return split_lib.world_verts(obj)[vs]


def _repaint(obj, name):
    idx = [i for i, m in enumerate(obj.data.materials) if m.name == name][0]
    for p in obj.data.polygons:
        p.material_index = idx


def crank_geometry():
    """Axis and centre of the jack's pinion, and the grip's axis, from the meshes."""
    arm, grip = part('28A-O31_ROLLAR'), part('28A-O34_ARM')
    axis = split_lib.flat_axis(arm, ('mat_0',))
    if axis.z < 0.0:
        axis = -axis
    pin = _mat_points(arm, 'mat_3')
    ax = np.array(axis)
    c0 = pin.mean(0)
    rad = np.linalg.norm((pin - c0) - np.outer((pin - c0) @ ax, ax), axis=1)
    centre, tip_r = split_lib.circle_centre(pin[rad > np.quantile(rad, 0.75)], axis)
    g = split_lib.world_verts(grip)
    gc = g.mean(0)
    _, _, vt = np.linalg.svd(g - gc)
    grip_axis = Vector(vt[0])
    if grip_axis.dot(axis) < 0.0:
        grip_axis = -grip_axis
    throw = Vector(gc) - centre
    throw -= axis * throw.dot(axis)
    return {'axis': axis, 'centre': centre, 'pinion_tip_radius_m': tip_r, 'grip_centre': Vector(gc),
            'grip_axis': grip_axis, 'throw_m': throw.length}


def split_handle():
    """Grip off the strap, then the strap into its two jaws."""
    grip = part('28A-O34_ARM')
    out = {}
    latch, out['grip_strap'] = split_lib.split(grip, split_lib.by_material('mat_0'), PRE + '28A-O34L_LATCH' + SUF,
                                               'crank handle grip', 'handle strap, front jaw with the catch loop')
    if 'skipped' not in out['grip_strap']:
        _repaint(grip, 'mat_2')
        _repaint(latch, 'mat_0')
    g = crank_geometry()
    normal = g['grip_axis'].cross(Vector((1.0, 0.0, 0.0))).normalized()
    if normal.y < 0.0:
        normal = -normal
    # the front jaw (away from the plate) is the side the normal points to; it has to keep the
    # name O34L, so the cut hands the plate side to the new object
    rear, out['jaws'] = split_lib.split_plane(latch, g['grip_centre'], -normal, PRE + '28A-O34R_LATCH' + SUF,
                                              'handle strap, front jaw with the catch loop', 'handle strap, fixed rear jaw')
    if 'skipped' not in out['jaws']:
        # the catch loop beyond the grip goes whole to the front jaw: cut it off the rear jaw
        # just past the bore and fuse it back on
        bore_r = 0.0178
        x0 = g['grip_centre'].x - bore_r - 0.006
        at = Vector((x0, g['grip_centre'].y, g['grip_centre'].z))
        split_lib.score(latch, at, Vector((-1.0, 0.0, 0.0)))  # so its closing face ends where the loop's begins
        loop, out['loop_cut'] = split_lib.split_plane(rear, at, Vector((-1.0, 0.0, 0.0)),
                                                      PRE + '28A-O34X_LOOP' + SUF, 'handle strap, fixed rear jaw', 'catch loop, rear half')
        out['loop_join'] = split_lib.join(latch, loop)
        del rear['split_into']
        latch['split_into'] = rear.name
    for o in (latch, rear):
        _repaint(o, 'mat_0')  # the closing faces take the strap's paint
    return out


def rig_jack():
    g = crank_geometry()
    back = empty('0B24_Back Cover Unit')
    jack_unit = empty('0B30_JACK Unit')
    arm, case, grip = part('28A-O31_ROLLAR'), part('28A-O39_TURN'), part('28A-O34_ARM')
    front, rear = part('28A-O34L_LATCH'), part('28A-O34R_LATCH')
    body = split_lib.world_verts(part('28A-O38_BLACKET'))
    seat = Vector((body.min(0) + body.max(0)) / 2.0)
    jack = rig_lib.make_stow('Jack', seat, [jack_unit, case], parent=back, released_by='Jack_Crank_Latch, the two holder clamps 28A_HOLDER (not split yet)',
                             source='seat = middle of the jack body 28A-O38 in its kit pose on the rear plate')
    rig_lib.gameplay(jack, interaction='the 15 t jack: free the crank strap and the two holder clamps, lift it off the plate, carry it to the wheel or track job',
                     damage='shot off the plate or its rack bent by a hit on the rear', repair='none in the field: take one from a wreck or the depot')
    crank = rig_lib.make_joint('Jack_Crank', 'hinge', g['centre'], g['axis'], [arm, grip], parent=jack,
                               source='axis = normal of the crank arm 28A-O31; centre = circle through the pinion tooth tips (radius %.1f mm)' % (g['pinion_tip_radius_m'] * 1000.0))
    crank['continuous'] = True
    crank['throw_m'] = g['throw_m']
    crank['pinion_tip_radius_m'] = g['pinion_tip_radius_m']
    crank['locked_by'] = 'Jack_Crank_Latch'
    rig_lib.gameplay(crank, interaction='wind the jack: one hand on the grip, the pinion drives the gear case and the rack',
                     damage='arm bent or pinion teeth stripped', repair='straighten the arm with the hammer; a stripped pinion ends the jack')
    lf = split_lib.world_verts(front)
    end = lf[lf[:, 0] > lf[:, 0].max() - 0.012]
    pin = Vector(end.mean(0))
    # axis turned so that a positive angle swings the free end away from the plate
    latch = rig_lib.make_joint('Jack_Crank_Latch', 'hinge', pin, -g['grip_axis'], [front], parent=back,
                               source='pin = middle of the strap\'s solid end, axis along the grip; the kit shows no pin (named approximation)')
    rig_lib.gameplay(latch, interaction='lift the catch loop and swing the front jaw off the grip; the crank is then free to wind',
                     damage='strap bent shut or torn off by a hit on the rear plate', repair='lever it open with the crowbar; the crank works without it')
    rig_lib.reparent(rear, back)
    return {'jack': jack.name, 'crank': crank.name, 'latch': latch.name, 'throw_m': round(g['throw_m'], 4),
            'pin': [round(x, 4) for x in pin], 'axis': [round(x, 4) for x in g['axis']]}


def crank_stowed_range(step=2.0):
    """The crank turns right round only with the jack off the tank: on the plate its throw is longer
    than the gap to the plate. Record how far it swings each way while stowed."""
    e = bpy.data.objects['CTRL.Jack_Crank']
    got = rig_lib.sweep(e, -180.0, 180.0, step=step, radius=0.3)
    e['stowed_min'] = float(got['min'])
    e['stowed_max'] = float(got['max'])
    hits = [', '.join(got.get(k + '_hit', (None, []))[1]) or 'nothing' for k in ('min', 'max')]
    e['limit_source'] = ('continuous off the tank; stowed on the plate it is free from %.0f to %.0f deg (%.0f deg step sweep), '
                         'stopped by %s and by %s' % (got['min'], got['max'], step, hits[0], hits[1]))
    return got


def latch_limits(step=2.0):
    """Which way the front jaw opens and how far: sweep both ways against everything near except
    the grip it lets go of."""
    e = bpy.data.objects['CTRL.Jack_Crank_Latch']
    got = rig_lib.sweep(e, 0.0, 170.0, step=step, radius=0.3)
    hit = got.get('max_hit', (None, []))
    rig_lib.limits(e, 0.0, got['max'], 'closed = the kit pose; open limit = last free angle of a %.0f deg step sweep, stopped by %s' % (
        step, ', '.join(hit[1]) if hit[0] is not None else 'nothing within 170 deg'))
    return got


def split_shaft():
    """Starting shaft 28A-Z16 out of its two clips: everything between a clip's side faces belongs
    to the clip; the shaft is carried through on a straight run."""
    rod = part('28A-Z16_ROLLAR')
    w = split_lib.world_verts(rod)
    ends = w[w[:, 0] < w[:, 0].min() + 0.01].mean(0), w[w[:, 0] > w[:, 0].max() - 0.01].mean(0)
    a, b = np.array(ends[0]), np.array(ends[1])
    d = (b - a) / np.linalg.norm(b - a)
    t = (w - a) @ d
    r = np.linalg.norm((w - a) - np.outer(t, d), axis=1)
    shaft_r = float(np.median(r))
    big = np.sort(t[r > shaft_r * 1.8])
    spans, start, last = [], big[0], big[0]
    for x in big[1:]:
        if x - last > 0.03:
            spans.append((start, last))
            start = x
        last = x
    spans.append((start, last))

    def pick(f, obj):
        pts = np.array([obj.matrix_world @ v.co for v in f.verts])
        ft = (pts - a) @ d
        return any(ft.min() >= lo - 1.0e-4 and ft.max() <= hi + 1.0e-4 for lo, hi in spans)

    clips, rep = split_lib.split(rod, pick, PRE + '28A-Z16C_CLIPS' + SUF, 'starting crank shaft', 'the shaft\'s two clips')
    rep['clip_spans_m'] = [(round(float(lo), 3), round(float(hi), 3)) for lo, hi in spans]
    rep['axis'] = [round(float(x), 4) for x in d]
    return rep


def rig_starter():
    back = empty('0B24_Back Cover Unit')
    cover, rod = part('28A-B69_COVER'), part('28A-Z16_ROLLAR')
    n = split_lib.flat_axis(cover)
    if n.y < 0.0:
        n = -n
    c = split_lib.world_verts(cover)
    mid = Vector((c.min(0) + c.max(0)) / 2.0)
    seat = mid + n * float(((c - np.array(mid)) @ np.array(n)).min())  # the cover's back face, on the plate
    cov = rig_lib.make_stow('Starter_Port_Cover', mid, [cover], parent=back, released_by='fastener set starter_port_cover (one hex head)',
                            source='seat = the cover 28A-B69 in its kit pose; taken for the inertia starter port, not checked against a drawing')
    rig_lib.gameplay(cov, interaction='undo the centre bolt, take the cover off the starter port', damage='cover dented or its bolt seized by a hit low on the rear',
                     repair='free the bolt with the wrench and hammer')
    w = split_lib.world_verts(rod)
    stow = Vector((w.min(0) + w.max(0)) / 2.0)
    crank = rig_lib.make_stow('Starter_Crank', stow, [rod], parent=back, released_by='its two clips 28A-Z16C (left closed in the kit; named approximation)',
                              source='stowed pose = the kit pose in its clips; taken for the starting crank shaft, not checked')
    crank['length_m'] = float(w[:, 0].max() - w[:, 0].min())
    rig_lib.gameplay(crank, interaction='lift the shaft out of its clips, carry it to the open port, seat its square end',
                     damage='bent or lost with the rear plate fittings', repair='straighten with the hammer, or do without: tow start or the electric starter')
    sock = rig_lib.make_socket('Starter_Crank_Turn', seat, -n, 'Starter_Crank', parent=back,
                               source='axis = normal of the cover 28A-B69 into the hull, through the cover\'s middle at the plate face')
    sock['requires'] = 'Starter_Port_Cover off'
    rig_lib.gameplay(sock, interaction='turn the shaft to spin up the inertia starter, then trip it to turn the engine over',
                     damage='starter dog or flywheel gear broken', repair='none in the field')
    return {'cover': cov.name, 'crank': crank.name, 'socket': sock.name, 'port': [round(x, 4) for x in seat], 'normal': [round(x, 4) for x in n],
            'shaft_length_m': round(crank['length_m'], 3)}


def build():
    out = {'handle': split_handle(), 'jack': rig_jack(), 'latch': latch_limits(), 'crank': crank_stowed_range(),
           'shaft': split_shaft(), 'starter': rig_starter()}
    rod = part('28A-Z16_ROLLAR')
    w = split_lib.world_verts(rod)
    tip = w[w[:, 0] > w[:, 0].max() - 0.002]
    e = bpy.data.objects['CTRL.Starter_Crank']
    e['drive_square_m'] = float(max(tip[:, 1].max() - tip[:, 1].min(), tip[:, 2].max() - tip[:, 2].min()))
    e['drive_end'] = 'the +X end in the stowed pose, the end nearest the port'
    return out


# Which heads on the rear plate parts become fastener objects, and as what. Read on 2026-11-08
# from the full CAD census (fastener_census.py); sizes in mm. Everything not named here stays
# moulded into its part: hubs, posts, pins and knobs are not fasteners. Round heads called rivets
# are a reading of the part (riveted jack body, sheet plate), not a checked fact.
def _is(shape, lo, hi, material=None):
    return lambda r: r['shape'] == shape and lo <= r['size'] * 1000.0 <= hi and (material is None or r.get('material') == material)


FASTENER_POLICY = [
    # part key, test, kind, removable, set
    ('28A-B69_COVER', _is('hex', 14, 18), 'bolt', True, 'starter_port_cover'),
    ('28A_HOLDER', _is('hex', 12, 16), 'bolt', True, 'jack_holder'),
    ('28A-O39_TURN', _is('round', 9, 12, 'mat_2'), 'screw', True, 'jack_gear_case_cover'),
    ('28A-O38_BLACKET', _is('round', 8, 12.5), 'rivet', False, 'jack_body'),
    ('28A-J104_PLATE', _is('round', 11, 13.5), 'rivet', False, 'rear_centre_plate'),
    ('28A-J104_PLATE', _is('hex', 22, 27), 'bolt', True, 'rear_centre_plate'),
    ('30Z02_COVER', _is('hex', 33, 37), 'bolt', True, 'rear_plate_top_edge'),
    ('30Z02_COVER', lambda r: r['shape'] == 'round' and 33 <= r['size'] * 1000.0 <= 36 and abs(r['height'] - 0.0105) < 0.001, 'screw', True, 'rear_plate_disc'),
]


def rear_fasteners(census_path):
    """Stand fastener objects on the rear plate parts from a census file (see fastener_census.py)."""
    import json

    import fasteners
    data = json.load(open(census_path))
    by_object = {v['object']: v['rows'] for v in data.values()}
    reports, cut_meshes = [], set()
    for key in sorted({p[0] for p in FASTENER_POLICY}):
        rules = [p for p in FASTENER_POLICY if p[0] == key]
        objs = [o for o in bpy.data.objects if o.type == 'MESH' and (PRE + key) in o.name]
        for o in objs:
            rows = by_object.get(o.name)
            if rows is None:
                rows = next((v for k, v in by_object.items() if (PRE + key) in k), None)
            if rows is None:
                reports.append({'object': o.name, 'skipped': 'not in the census'})
                continue
            if o.get('fasteners_cut') and o.data.name not in cut_meshes:
                reports.append({'object': o.name, 'skipped': 'already done'})
                continue

            def kind_of(r, rules=rules):
                for _, test, kind, removable, _set in rules:
                    if test(r):
                        return kind, removable
                return None

            # one set name per part; a part with two rules takes the set of the rule that matched
            rep = fasteners.apply_rows(o, rows, kind_of, set_name='', cut=o.data.name not in cut_meshes)
            cut_meshes.add(o.data.name)
            for f in [c for c in o.children if c.name.startswith('FASTENER.') and 'fastener_set' not in c]:
                for _, test, kind, removable, set_name in rules:
                    if f['fastener_kind'] == kind:
                        f['fastener_set'] = set_name if len(objs) == 1 else '%s.%s' % (set_name, o.name.rsplit('.', 1)[-1] if o.name[-3:].isdigit() else '000')
                        break
            reports.append(rep)
    return reports
