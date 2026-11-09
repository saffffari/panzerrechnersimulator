"""Turret, gun, hull fittings, crew compartment and engine bay of the gen2 Tiger rig (destructibles
rows A8, B2 to B6, C1 to C9, D1 to D9, E1 to E8, E11). Read from part names, boxes and a few
fitted axes on 2026-11-08; most parts here were NOT looked at one by one in the viewport, so what
each is, is a reading of its kit name and place. `source` on each control says how it was read.

    sys.path.append(<this dir>); import turret_hull_rig as th
    th.build_turret(); th.build_gun(); th.build_stowed(); th.build_rounds()

The kit has no turret assembly: the turret's parts hang in ten sibling assemblies under the tank.
`Turret_Traverse` gathers them; `Gun_Elevation` under it carries the barrel, breech, cradle and
coaxial gun assemblies; `Gun_Recoil` under that carries what recoils.

Named approximations:
- The trunnion axis is put through the middle of part 01A33 (the wide bar behind the mantlet).
- The mantlet rides the elevation, not the recoil (row D2's open bug in today's game model).
- Recoil travel is swept against the cradle guard; where the sweep finds nothing it is 0.58 m,
  the gun's nominal recoil, from memory and not verified.
- A part listed as stowed comes out whole along no particular path; clamps, straps and bolts that
  hold it are named in `released_by` only where the kit shows them.
"""
import bpy
import numpy as np
from mathutils import Vector

import rig_build
import rig_lib
import split_lib

PRE = 'TG_RM5010-'
ROOT = '0A_Tiger I'
RING = (0.0002, 1.0124, 1.8352)
TRUNNION = (0.008, -0.17, 2.203)
TURRET_ASSEMBLIES = ('0C01', '0C02', '0C03', '0C05', '0C09', '0C10', '0C11', '0C12', '0C13', '0C14')
ELEVATING_ASSEMBLIES = ('0C01', '0C02', '0C03')


def _meshes(key):
    return sorted([o for o in bpy.data.objects if o.type == 'MESH' and (PRE + key) in o.name and not o.name.startswith('FASTENER')], key=lambda o: o.name)


def _asm(prefix):
    return sorted([o for o in bpy.data.objects if o.type == 'EMPTY' and o.name.startswith(PRE + prefix)], key=lambda o: o.name)


def _box(objs):
    w = np.concatenate([split_lib.world_verts(o) for o in objs])
    return w.min(0), w.max(0)


def _centre(objs):
    lo, hi = _box(objs)
    return Vector((lo + hi) / 2.0)


def _joint(name, kind, pivot, axis, parts, parent, source, **kw):
    """rig_lib joints with objects (not short names) as parts."""
    par = bpy.data.objects[parent] if parent.startswith('CTRL.') else rig_build.assembly(parent)
    if kind == 'stow':
        e = rig_lib.make_stow(name, pivot, parts, parent=par, source=source, released_by=kw.pop('released_by', ''))
    else:
        e = rig_lib.make_joint(name, kind, pivot, axis, parts, parent=par, source=source, size=kw.pop('size', 0.12))
    for k in ('continuous', 'locked_by', 'requires'):
        if k in kw:
            e[k] = kw.pop(k)
    rig_lib.gameplay(e, kw.pop('interaction', ''), kw.pop('damage', ''), kw.pop('repair', ''))
    for k, v in kw.pop('props', {}).items():
        e[k] = v
    return e


def _sweep(e, lo, hi, step, radius=0.6, ignore=(), fallback=None):
    got = rig_lib.sweep(e, lo, hi, step=step, radius=radius, ignore=list(ignore))
    why = []
    for d in ('min', 'max'):
        hit = got.get(d + '_hit', (None, []))
        why.append('%s %g (%s)' % (d, got[d], 'stopped by ' + ', '.join(hit[1]) if hit[0] is not None else 'nothing in the way'))
    a, b = got['min'], got['max']
    note = 'zero = the kit pose; %s; %g step sweep' % ('; '.join(why), step)
    if fallback is not None and got.get('max_hit', (None,))[0] is None:
        b = fallback[0]
        note += '; upper limit set to %g: %s' % fallback
    rig_lib.limits(e, a, b, note)
    return {'name': e.name, 'min': a, 'max': b, 'hits': [got.get('min_hit'), got.get('max_hit')]}


def build_turret():
    reports = []
    asms = [a for p in TURRET_ASSEMBLIES for a in _asm(p)]
    e = _joint('Turret_Traverse', 'hinge', RING, (0, 0, 1), asms, ROOT, continuous=True,
               source='axis = circle through the ring gear 10A12\'s teeth (radius 1.039 m), upright; carries the ten turret assemblies',
               interaction='the turret traverses', damage='a fragment wedged in the ring gap, or the ring bolts and bearing wrecked',
               repair='knock the fragment out from outside; pack the bearing and bolt down with spare ring bolts (concession)')
    reports.append({'name': e.name, 'riders': len(asms)})
    elev = [a for p in ELEVATING_ASSEMBLIES for a in _asm(p)]
    g = _joint('Gun_Elevation', 'hinge', TRUNNION, (-1, 0, 0), elev, 'CTRL.Turret_Traverse',
               source='axis across the turret through the middle of bar 01A33 behind the mantlet (named approximation); positive lifts the muzzle',
               interaction='the gun elevates', damage='trunnion or cradle hit: elevation stiff or lost', repair='none beyond freeing the handwheel gear')
    reports.append(_sweep(g, -15.0, 25.0, 1.0, radius=0.8))
    return reports


def build_gun():
    reports = []
    D = bpy.data.objects
    recoiling = [o for k in ('01A04_BARREL', '01A14_BARREL', '01A29_BARREL', '01A30_BARREL', '01A31_BARREL', '01A36_COVER', '01A37_COUPLING', '02U03_ROOM', '02J66_ARM') for o in _meshes(k)]
    r = _joint('Gun_Recoil', 'slide', (TRUNNION[0], 0.72, TRUNNION[2]), (0, 1, 0), recoiling, 'CTRL.Gun_Elevation',
               source='along the bore (rearward), taken at the breech ring 02U03; carries the barrel parts, breech ring and lever, not the mantlet or cradle',
               interaction='the gun recoils and runs out', damage='mount bolts sheared: the gun stays back', repair='jack it back into battery, fit spare mount bolts (concession)')
    reports.append(_sweep(r, 0.0, 0.8, 0.01, radius=0.5, fallback=(0.58, 'the gun\'s nominal recoil, from memory, not verified')))
    block = _meshes('02J54_BLOCK')
    b = _joint('Breech_Block', 'slide', tuple(_centre(block)), (0, 0, 1), block, 'CTRL.Gun_Recoil',
               source='block 02J54 under the breech ring, sliding upright', interaction='the block rises to close and drops to open',
               damage='block jammed', repair='clear it; work the lever')
    reports.append(_sweep(b, -0.1, 0.4, 0.01, radius=0.3))
    arm = _meshes('02J66_ARM')
    lo, hi = _box(arm)
    lever = _joint('Breech_Lever', 'hinge', (float((lo[0] + hi[0]) / 2), float(lo[1]), float((lo[2] + hi[2]) / 2)), (1, 0, 0), arm, 'CTRL.Gun_Recoil',
                   source='arm 02J66 beside the breech, taken for the breech lever; pivot put at its forward end (a reading, not verified)',
                   interaction='work the lever to open the breech')
    reports.append(_sweep(lever, -70.0, 70.0, 2.0, radius=0.3))
    wheels = _meshes('05J55_ROLLOR')
    for o in wheels:
        w = split_lib.world_verts(o)
        c = Vector((w.min(0) + w.max(0)) / 2.0)
        _, _, vt = np.linalg.svd(w - w.mean(0))
        high = c.z > 2.0
        name = 'Traverse_Handwheel_Commander' if high else 'Elevation_Handwheel'
        e = _joint(name, 'hinge', tuple(c), tuple(float(x) for x in vt[2]), [o], 'CTRL.Turret_Traverse', continuous=True,
                   source='handwheel 05J55 (%s); axis = the disc\'s thin direction' % ('high on the left rear: the commander\'s' if high else 'low beside the elevation gearbox 06J56'),
                   interaction='turn by hand', damage='gear stripped or jammed', repair='free with the hammer; fit a shear pin (concession)',
                   props={'drives': 'Turret_Traverse' if high else 'Gun_Elevation'})
        reports.append({'name': e.name})
    turn = _meshes('10J49_TURN')
    e = _joint('Traverse_Gear_Gunner', 'stow', tuple(_centre(turn)), None, turn, 'CTRL.Turret_Traverse',
               source='part 10J49 at the gunner\'s place, taken for the gunner\'s traverse gear and handwheel in one piece (wheel not separate)',
               interaction='the gunner\'s hand traverse', props={'drives': 'Turret_Traverse', 'wheel_not_separate': True})
    reports.append({'name': e.name})
    pedal = _meshes('05J26_PEDLAL')
    lo, hi = _box(pedal)
    p = _joint('Traverse_Pedal', 'hinge', (float((lo[0] + hi[0]) / 2), float((lo[1] + hi[1]) / 2), float(lo[2])), (1, 0, 0), pedal, 'CTRL.Turret_Traverse',
               source='pedal 05J26 on the basket floor, rocking about a line across its foot (a reading)', interaction='rock the pedal for power traverse',
               damage='linkage bent', repair='straighten it')
    reports.append(_sweep(p, -20.0, 20.0, 1.0, radius=0.3))
    for side, key in (('R', '15A20_COVER'), ('L', '15A21_COVER')):
        lid = _meshes(key)
        lo, hi = _box(lid)
        e = _joint('Bin_Lid_' + side, 'hinge', (float((lo[0] + hi[0]) / 2), float(lo[1]), float(hi[2])), (-1, 0, 0), lid, 'CTRL.Turret_Traverse',
                   source='lid %s on the turret bin, hinged along its forward edge (a reading: the kit shows no hinge barrel)' % key.split('_')[0],
                   interaction='lift the bin lid', damage='bin holed, lid torn off', repair='none')
        reports.append(_sweep(e, 0.0, 120.0, 2.0, radius=0.4))
    return reports


# name, kit keys (all objects whose name holds a key), parent control or assembly, what it is / notes.
# Sides are by place: _L is the tank's left, +X in the file.
STOWED = [
    ('Muzzle_Cover', ['01A36_COVER'], 'CTRL.Gun_Recoil', 'cap on the muzzle (01A36)', 'pull the cap off the muzzle'),
    ('Gunner_Sight', ['01J18_VIEWER'], 'CTRL.Gun_Elevation', 'gunner\'s telescope 01J18, left of the gun', 'pull the telescope inward out of its mount; seat the spare'),
    ('Coax_MG', ['03J19_GUN'], 'CTRL.Gun_Elevation', 'coaxial MG 34 (03J19) on its base 03J17', 'lift the gun out of its cradle'),
    ('AA_MG', ['12O05_GUN', '12O40_GUN'], 'CTRL.Turret_Traverse', 'cupola MG (12O05, 12O40) on its mount', 'lift the gun off the cupola mount'),
    ('Ventilator_Cowl', ['13Z59_FILTER'], 'CTRL.Turret_Traverse', 'roof ventilator cowl 13Z59 at the turret\'s middle', 'lift the cowl off'),
    ('Recoil_Brake', ['02J50_OIL'], 'CTRL.Gun_Elevation', 'cylinder 02J50 beside the breech (which is brake and which recuperator is not identified)', 'top up; a workshop job to remove'),
    ('Recuperator', ['02J51_OIL'], 'CTRL.Gun_Elevation', 'cylinder 02J51 beside the breech', 'top up; a workshop job to remove'),
    ('Side_Viewer_R', ['04S95_VIEWER_îAÔO'], 'CTRL.Turret_Traverse', 'turret side vision fitting 04S95', 'pull the block inward'),
    ('Hull_MG', ['28B-K07_GUN', '28B-K08_GUN', '28B-K81_GUN'], '0B25', 'bow MG (28B-K07, K08, K81) in its ball', 'draw the gun inward out of the ball'),
    ('Headlamp', ['37T04_LIGHT'], '0B25', 'headlamp 37T04', 'lift the lamp off its foot'),
    ('Driver_Visor_Glass', ['28B-T03_GLASS'], '0B25', 'driver\'s visor block 28B-T03', 'pull the block inward, seat a spare'),
    ('Mudguard_Rear_R', ['30F25_COVER'], '0B24', 'rear mudguard 30F25', 'lever it clear or tear it off with the crowbar'),
    ('Mudguard_Rear_L', ['30F32_COVER'], '0B24', 'rear mudguard 30F32', 'lever it clear or tear it off with the crowbar'),
    ('Mudguard_Front_R', ['37F33_COVER'], '0B16', 'front mudguard 37F33', 'lever it clear or tear it off with the crowbar'),
    ('Mudguard_Front_L', ['37F34_COVER'], '0B16', 'front mudguard 37F34', 'lever it clear or tear it off with the crowbar'),
    ('Carburettor_1', ['28U37_CARBURETOR'], 'CTRL.Engine', 'carburettor 28U37', 'clear or lift off the carburettor'),
    ('Carburettor_2', ['28U43_CARBURETOR'], 'CTRL.Engine', 'carburettor 28U43', 'clear or lift off the carburettor'),
    ('Air_Filter_A', ['28U20_FILTER TANK_îAÔO'], 'CTRL.Engine', 'air filter bowl 28U20', 'drop the bowl, clean it'),
    ('Fuel_Tank_R', ['27H12_BOX_îAÔO'], '0B16', 'fuel tank 27H12', 'bung the hole; a workshop job to remove'),
    ('Radiator_L', ['27K76_COOLBOX'], '0B16', 'radiator 27K76', 'plug the core; lift out upward through the deck'),
    ('Radiator_R', ['27K77_COOLBOX'], '0B16', 'radiator 27K77', 'plug the core; lift out upward through the deck'),
    ('Gearbox', ['21K49_BOX'], '0B16', 'gearbox 21K49', 'a workshop job'),
    ('Steering_Unit', ['21K51_BOX', '21K54_BOX'], '0B16', 'steering unit 21K51 and 21K54', 'top up oil; a workshop job to remove'),
    ('Radio', ['21K57_BOX'], '0B16', 'radio set 21K57', 're-seat plugs and fuses'),
    ('Aerial', ['34D36_ANTENNA'], '0B26', 'aerial rod 34D36 in its base 34D41', 'unscrew the rod, screw in a spare'),
]


def build_stowed():
    reports = []
    D = bpy.data.objects
    eng = _meshes('28U05_MAIN')
    if 'CTRL.Engine' not in D:
        _joint('Engine', 'stow', tuple(_centre(eng)), None, [next(a for a in _asm('0B29'))], '0B16',
               source='engine assembly 0B29 (block 28U05 and what is fixed to it) in its kit pose', released_by='Deck_Hatch open; its mounts and lines (not modelled)',
               interaction='a workshop job: the engine lifts out through the deck', damage='block holed, fuel line cut, ignition dead, seized', repair='patch the line, change plugs, clear the carburettor')
        reports.append({'name': 'CTRL.Engine'})
    twins = {'Side_Viewer_R': ('Side_Viewer_L', '04S95_VIEWER_îAÔO.001'), 'Air_Filter_A': ('Air_Filter_B', '28U20_FILTER TANK_îAÔO.001'), 'Fuel_Tank_R': ('Fuel_Tank_L', '27H12_BOX_îAÔO.001')}
    todo = list(STOWED)
    for name, keys, parent, what, action in STOWED:
        if name in twins:
            todo.append((twins[name][0], [twins[name][1]], parent, what, action))
    for name, keys, parent, what, action in todo:
        objs = []
        for k in keys:
            found = [o for o in D if o.type == 'MESH' and (PRE + k) in o.name]
            if k.endswith('îAÔO'):
                found = [o for o in found if o.name == PRE + k]
            if k.endswith(('.001', '.002')):
                found = [o for o in D if o.name == PRE + k]
            objs += found
        if not objs:
            reports.append({'name': name, 'skipped': 'no part ' + ', '.join(keys)})
            continue
        if name in ('Coax_MG', 'Hull_MG'):   # the MG 34's small parts ride with the gun
            asm = objs[0].parent
            while asm is not None and asm.type != 'EMPTY':
                asm = asm.parent
            box_lo, box_hi = _box(objs)
            objs += [o for o in D if o.type == 'MESH' and 'GUN_MG34' in o.name and o not in objs and
                     all(box_lo[i] - 0.25 <= o.matrix_world.translation[i] <= box_hi[i] + 0.25 for i in range(3))]
        par = parent if parent.startswith('CTRL.') else next(a.name.replace(PRE, '').split('_îAÔO')[0] for a in _asm(parent))
        e = _joint(name, 'stow', tuple(_centre(objs)), None, objs, par, source='seat = %s in the kit pose; read from its name and place, not viewed' % what,
                   released_by='not read from the kit', interaction=action)
        reports.append({'name': e.name, 'parts': len(objs)})
    return reports


def build_blocks_and_seats():
    """Vision blocks and seats: each a stowed part."""
    reports = []
    D = bpy.data.objects
    blocks = sorted(_meshes('12T07_GLASS'), key=lambda o: np.arctan2(*(np.array(_centre([o]))[:2] - np.array((0.87, 1.31)))))
    for i, o in enumerate(blocks):
        e = _joint('Vision_Block_Cupola_%d' % (i + 1), 'stow', tuple(_centre([o])), None, [o], 'CTRL.Turret_Traverse' if o.parent and o.parent.name.startswith(PRE) else o.parent.name,
                   source='cupola vision block 12T07, numbered round the cupola', released_by='its clamp (not in the kit)',
                   interaction='pull the block inward, seat a spare', damage='cracked or shattered: that view is dead', repair='fit a spare block')
        reports.append({'name': e.name})
    for o in _meshes('13T02_GLASS'):
        c = _centre([o])
        tag = 'Driver_Hatch' if (c.y < 0 and c.x > 0) else 'Radio_Hatch' if c.y < 0 else 'Loader'
        par = o.parent.name if o.parent.name.startswith('CTRL.') else ('CTRL.Turret_Traverse' if c.y > 0 else o.parent.name.replace(PRE, '').split('_îAÔO')[0])
        e = _joint('Vision_Block_' + tag, 'stow', tuple(c), None, [o], par, source='vision block 13T02 (%s)' % tag.replace('_', ' ').lower(),
                   released_by='its clamp (not in the kit)', interaction='pull the block inward, seat a spare', damage='cracked or shattered', repair='fit a spare block')
        reports.append({'name': e.name})
    seats = [o for k in ('05J37_SEAT', '06J20A_SEAT', '06J20B_SEAT', '06J24_SEAT', '06J46_SEAT', '06J75_SEAT') for o in _meshes(k)]
    for i, o in enumerate(sorted(seats, key=lambda o: o.name)):
        e = _joint('Seat_%d' % (i + 1), 'stow', tuple(_centre([o])), None, [o], 'CTRL.Turret_Traverse', source='seat part %s in the basket; which crewman\'s is not identified' % o.name.replace(PRE, '').split('_îAÔO')[0],
                   released_by='its post (not separate)', interaction='a seat can be torn off its post', damage='torn off: a crewman has no post', repair='none')
        reports.append({'name': e.name})
    return reports


def build_rounds():
    """Every 8.8 cm round in the kit as a stowed part, numbered front to rear, right to left."""
    D = bpy.data.objects
    rounds = sorted([o for o in D if o.type == 'MESH' and '_AMMO' in o.name], key=lambda o: (round(_centre([o]).y, 1), _centre([o]).x))
    made = 0
    for i, o in enumerate(rounds):
        par = o.parent
        while par is not None and not (par.type == 'EMPTY' and par.name.startswith(PRE)) and not par.name.startswith('CTRL.'):
            par = par.parent
        pname = par.name if par.name.startswith('CTRL.') else par.name.replace(PRE, '').split('_îAÔO')[0]
        e = _joint('Round_%03d' % (i + 1), 'stow', tuple(_centre([o])), None, [o], pname, source='round %s in its kit place' % o.name.replace(PRE, '').split('_îAÔO')[0],
                   released_by='its rack clamp (not in the kit)', interaction='draw the round from its rack', damage='case holed, rack bent, propellant lit',
                   repair='lever a jammed round free; throw a damaged one out')
        w = split_lib.world_verts(o)
        _, _, vt = np.linalg.svd(w - w.mean(0))
        e['long_axis'] = [float(x) for x in vt[0]]
        made += 1
    return {'rounds': made}
