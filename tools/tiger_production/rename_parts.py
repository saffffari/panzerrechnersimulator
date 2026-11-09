"""Readable object names for the gen2 rig file, reversible (2026-11-09, the owner: "rename the
blender objects names to be more informative").

The kit's own names (TG_RM5010-28A-B69_COVER_îAÔO.001) say little: 424 parts are called PLATE.
New names read  <Zone>.<What>[.L|.R].<kit number>[.NN] :

    Turret.Hatch_Loader.02J50          the main part a rig control carries takes the control's name
    Turret.Hatch_Loader.Lock.02J51     other parts on that control add the kit's word for them
    Hull.Plate.L.27H25.02              a part no control carries: zone from its kit assembly, the
                                       kit's word (spelling mended), side (+X is the tank's left),
                                       numbered front to rear where several share a name
    Assembly.Suspension_Station.0B17.03   the kit's grouping empties

The kit number stays in every name, and the kit name is kept on the object as `kit_name`, so
nothing is lost: kit_names() puts the old names back, apply() the new ones. Names held in string
properties (fastener_host, split_from, the repair metadata on actions) are rewritten both ways.
The older build scripts (rear_plate_rig, running_gear_rig, tools_rig, turret_hull_rig, fasten_all,
materials) look parts up by kit name: call kit_names() before running them, apply() after.
rig_build.obj() and assembly() accept kit names in either state.

    import rename_parts; rename_parts.plan()     # what would change, nothing touched
    rename_parts.apply()                         # rename
    rename_parts.kit_names()                     # back to the kit's names
"""
import collections
import json
import re

import bpy

PRE = 'TG_RM5010-'
KIT = re.compile(r'^TG_RM5010-([0-9A-Za-z-]+?)_(.+?)(?:_îAÔO)?(?:\.\d+)?$')
WORDS = {'BLACKET': 'Bracket', 'BLACET': 'Bracket', 'BLOCKET': 'Bracket', 'ROLLOR': 'Wheel', 'ROLLAR': 'Roller', 'BROCK': 'Block',
         'WHELL': 'Wheel', 'LCOK': 'Lock', 'PEDLAL': 'Pedal', 'HAND': 'Handle', 'CHAIR': 'Seat', 'TRACK': 'Link', 'AMMO': 'Round',
         'COOLBOX': 'Radiator', 'VIEWER': 'Vision_Block', 'VIEW': 'Vision_Block', 'ARMOR': 'Armour', 'CARBURETOR': 'Carburettor',
         'îAÔO': 'Part', 'COLLAR(R)': 'Collar', 'COLLAR(L)': 'Collar', 'DRIVER': 'Final_Drive', 'SCAN': 'Sight', 'W': 'Part'}
ASSEMBLIES = {'Tiger I': ('Tiger', 'Tiger'), 'Base Unit': ('Hull', 'Hull'), 'Rollor System': ('RunningGear', 'Suspension_Station'),
              'Drive Rollar': ('RunningGear', 'Sprocket'), 'Guide Roller': ('RunningGear', 'Idler'),
              'Back Cover Unit': ('RearPlate', 'Rear_Plate'), 'Front Armor': ('FrontPlate', 'Front_Plate'),
              'Cover Unit': ('Deck', 'Deck'), 'Track Unit': ('Track', 'Track_Link'), 'EngineSystem': ('Engine', 'Engine'),
              'JACK Unit': ('Jack', 'Jack'), '88 Barrel Unit': ('Gun', 'Barrel'), '88 Barrel Base Unit': ('Gun', 'Barrel_Base'),
              'Gun Unit': ('Gun', 'Gun'), 'Room Unit': ('Turret', 'Turret_Shell'), 'Armor Unit': ('Turret', 'Mantlet'),
              'Door Unit': ('Turret', 'Hatch'), 'Top Cover Unit': ('Turret', 'Roof'), 'BLACET': ('Hull', 'Bracket_Set')}
GUN = {'Gun_Elevation', 'Gun_Recoil', 'Breech_Block', 'Breech_Lever', 'Recoil_Brake', 'Recuperator', 'Muzzle_Cover', 'Coax_MG',
       'Gunner_Sight'}
CONTAINERS = {'Turret_Traverse', 'Gun_Elevation', 'Gun_Recoil', 'Engine'}
RUNNING = ('Wheel', 'Arm', 'Sprocket', 'Idler', 'Final_Drive', 'Track_')
ENGINE = ('Engine', 'Carburettor', 'Air_Filter', 'Radiator', 'Fan_', 'Fuel_Tank')
SIDED = re.compile(r'(^|_)[LR](\d|_|$)')


def kit_name(o):
    return o.get('kit_name', o.name)


def _word(w):
    if w in WORDS:
        return WORDS[w]
    return '_'.join(t.capitalize() for t in re.split(r'[\s_]+', w.strip()) if t)


def _chain(o):
    out, p = [], o.parent
    while p is not None:
        out.append(p)
        p = p.parent
    return out


def _zone_of_assembly(chain):
    for p in chain:
        m = KIT.match(kit_name(p))
        if m and p.type == 'EMPTY':
            z = ASSEMBLIES.get(m.group(2))
            if z:
                # the kit files both the engine deck and a turret part under "Cover Unit"
                return 'Turret' if (m.group(2) == 'Cover Unit' and m.group(1).startswith('0C')) else z[0]
    return 'Hull'


def _base(o):
    """(base name without numbering, sort key) for one kit object, or None to leave it alone."""
    kn = kit_name(o)
    chain = _chain(o)
    controls = [p.name[5:] if p.name.startswith('CTRL.') else p.name[6:] for p in chain if p.name.startswith(('CTRL.', 'FRAME.'))]
    c = o.matrix_world.translation
    if o.type == 'MESH':
        xs = [(o.matrix_world @ v.co) for v in (o.data.vertices[0], o.data.vertices[len(o.data.vertices) // 2], o.data.vertices[-1])] if o.data.vertices else [c]
        c = sum(xs, xs[0] * 0.0) / len(xs)
    key = (round(c.y, 3), round(-c.x, 3), round(c.z, 3))
    if kn.startswith('GUN_MG34'):
        own = next((x for x in controls if x.endswith('_MG')), 'MG34')
        return 'MG34.%s.Part' % own, key
    m = KIT.match(kn)
    if not m:
        return None
    kit, word = m.group(1), m.group(2)
    if o.type == 'EMPTY':
        z = ASSEMBLIES.get(word)
        return 'Assembly.%s.%s' % (z[1] if z else _word(word), kit), key
    if any(x in GUN for x in controls):
        zone = 'Gun'
    elif 'Turret_Traverse' in controls:
        zone = 'Turret'
    elif any(x.startswith(ENGINE) for x in controls):
        zone = 'Engine'
    elif any(x.startswith(RUNNING) for x in controls):
        zone = 'RunningGear'
    else:
        zone = _zone_of_assembly(chain)
    own = next((x for x in controls if x not in CONTAINERS), None)
    what = _word(word)
    if own is not None:
        holder = bpy.data.objects.get('CTRL.' + own)
        mates = [k for k in (holder.children_recursive if holder else []) if k.type == 'MESH' and not k.name.startswith('FASTENER.')]
        biggest = max(mates, key=lambda k: len(k.data.polygons)) if mates else o
        what = own if (o == biggest or len(mates) == 1) else '%s.%s' % (own, what)
    side = ''
    if zone != 'Gun' and not SIDED.search(what):
        side = '.L' if c.x > 0.2 else ('.R' if c.x < -0.2 else '')
    return '%s.%s%s.%s' % (zone, what, side, kit), key


def plan():
    """{object name now: new name} for every kit object. Touches nothing."""
    groups = collections.defaultdict(list)
    for o in bpy.data.objects:
        if not kit_name(o).startswith((PRE, 'GUN_MG34')):
            continue
        b = _base(o)
        if b is not None:
            groups[b[0]].append((b[1], o.name))
    out = {}
    for base, rows in groups.items():
        rows.sort()
        for n, (_, name) in enumerate(rows, 1):
            out[name] = base if len(rows) == 1 else '%s.%0*d' % (base, 2 if len(rows) < 100 else 3, n)
    return out


def _deep(x, sub):
    if isinstance(x, str):
        return sub(x)
    if isinstance(x, list):
        return [_deep(k, sub) for k in x]
    if isinstance(x, dict):
        return {sub(k): _deep(val, sub) for k, val in x.items()}
    return x


def _rewrite(mapping):
    """Replace old names by new ones inside string properties on objects, actions and the scene."""
    if not mapping:
        return 0
    pat = re.compile('|'.join(re.escape(k) for k in sorted(mapping, key=len, reverse=True)) .join(('(?:', r')(?![\w.])')))
    count = 0
    blocks = list(bpy.data.objects) + list(bpy.data.actions) + [bpy.context.scene]
    for b in blocks:
        for k in list(b.keys()):
            v = b[k]
            if k in ('kit_name', 'source_file') or not isinstance(v, str):
                continue
            sub = lambda text: pat.sub(lambda m: mapping[m.group(0)], text)
            new = sub(v)
            if v[:1] in '{[':
                # JSON written with escaped non-ASCII letters hides the kit's names from a plain search
                try:
                    new = json.dumps(_deep(json.loads(v), sub))
                    if json.loads(new) == json.loads(v):
                        new = v
                except ValueError:
                    pass
            if new != v:
                b[k] = new
                count += 1
    return count


def apply():
    mapping = plan()
    taken = {o.name for o in bpy.data.objects} - set(mapping)
    clash = [n for n in mapping.values() if n in taken]
    if clash or len(set(mapping.values())) != len(mapping):
        raise RuntimeError('names would collide: %s' % clash[:5])
    objs = {old: bpy.data.objects[old] for old in mapping}
    for old, o in objs.items():
        if 'kit_name' not in o:
            o['kit_name'] = old
        o.name = '__tmp__' + mapping[old][:50] + str(id(o) % 9973)
    for old, o in objs.items():
        o.name = mapping[old]
    bad = [(mapping[old], o.name) for old, o in objs.items() if o.name != mapping[old]]
    return {'renamed': len(mapping), 'not_as_planned': bad[:5], 'properties_rewritten': _rewrite({old: o.name for old, o in objs.items()})}


def refresh_properties():
    """Rewrite kit names still held in properties to the names the objects carry now."""
    return _rewrite({o['kit_name']: o.name for o in bpy.data.objects if 'kit_name' in o and o.name != o['kit_name']})


def kit_names():
    back = {o.name: o['kit_name'] for o in bpy.data.objects if 'kit_name' in o and o.name != o['kit_name']}
    objs = {now: bpy.data.objects[now] for now in back}
    for now, o in objs.items():
        o.name = '__tmp__' + str(id(o))
    for now, o in objs.items():
        o.name = back[now]
    return {'restored': len(back), 'properties_rewritten': _rewrite(back)}
