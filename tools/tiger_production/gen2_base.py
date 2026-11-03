"""Gen2 Tiger base: the RM-5010 kit parts in the game frame, hung on the game master's rig.
Run inside Blender on the kit conversion; writes a new file, never the inputs:

    TIGER_WORK=<dir> blender --background cad_tiger_i_rm5010_complete.blend \\
        --python tools/tiger_production/gen2_base.py -- <game master .blend> <out .blend>

TIGER_WORK holds fit.json (game = s*kit + t), tags.json, kit_map.json and master_objects.json from
match.py and export_pivots.py. Every kit mesh is placed at s*kit + t with its mesh data scaled once
(shared data such as the track links stays shared); the kit's assembly empties are dropped (each part
keeps kit_assembly); the master's pivots, lights, cameras and its 21 game-authored parts are appended;
each kit part is parented as its master counterpart is (kit-only parts as their nearest matched
neighbour), named SYSTEM.Part_kitnumber and tagged like tools/tiger_production/build.py."""
import bpy, json, os, re, sys
from mathutils import Matrix, Vector
W = os.environ['TIGER_WORK']
master_path, out = sys.argv[sys.argv.index('--')+1:][:2]
fit = json.load(open(os.path.join(W, 'fit.json'))); s = fit['s']; t = Vector(fit['t'])
tags = json.load(open(os.path.join(W, 'tags.json')))
km = json.load(open(os.path.join(W, 'kit_map.json'))); kit_to_master = km['kit_to_master']; authored = set(km['game_authored'])
mobj = {r['name']: r for r in json.load(open(os.path.join(W, 'master_objects.json')))}
# Original geometry only (owner, 2 Nov 2026): the gen1 master's authored and reshaped parts stay out;
# only its rig (pivots, lights, cameras) comes across. GEN2_AUTHORED=1 brings them in for comparison.
if os.environ.get('GEN2_AUTHORED') != '1':
    authored_all = set(authored); authored = set()
else:
    authored_all = set(authored)
def rig_parent(name):
    """A master parent, skipping authored parts that gen2 leaves out."""
    while name and name in authored_all and name not in authored:
        name = mobj[name]['parent']
    return name
master_to_kit = {m: k for k, ms in kit_to_master.items() for m in ms}
def clean(n): return re.sub(r'(_îAÔO.*?)?(\.\d+)?$', '', n)

# 1. Kit parts into the game frame, flat (no assembly empties).
G = Matrix.Translation(t) @ Matrix.Scale(s, 4)
kit = [o for o in bpy.data.objects if o.type == 'MESH']
world = {o.name: o.matrix_world.copy() for o in kit}
assembly = {o.name: clean(o.parent.name).replace('TG_RM5010-', '') if o.parent else '' for o in kit}
for o in kit:
    o.parent = None
for me in {o.data for o in kit}:
    me.transform(Matrix.Scale(s, 4))
for o in kit:
    o.matrix_world = G @ world[o.name] @ Matrix.Scale(1.0/s, 4)
    o['kit_assembly'] = assembly[o.name]
for o in [o for o in bpy.data.objects if o.type != 'MESH']:
    bpy.data.objects.remove(o, do_unlink=True)

# 2. The master's rig and authored parts.
want = [n for n, r in mobj.items() if r['type'] in ('EMPTY', 'LIGHT', 'CAMERA')] + sorted(authored)
with bpy.data.libraries.load(master_path, link=False) as (src, dst):
    dst.objects = [n for n in src.objects if n in want]
appended = {}
for o in dst.objects:
    if o is None: continue
    bpy.context.scene.collection.objects.link(o)
    appended[o.name] = o
# Appending an object brings its parent along; a non-authored master mesh pulled in that way is
# replaced by its kit counterpart below and removed.
pulled = [o for o in bpy.data.objects if o.type == 'MESH' and o.name in mobj and o.name not in authored and o.name not in want]
# Authored parts pulled in only as a pivot's parent are not wanted either.
pulled += [o for o in bpy.data.objects if o.type == 'MESH' and o.name in authored_all and o.name not in authored and o not in pulled]

def target(master_name):
    """The gen2 object standing for a master object."""
    master_name = rig_parent(master_name)
    if not master_name: return None
    if master_name in appended: return appended[master_name]
    if master_name in authored: return bpy.data.objects.get(master_name)
    k = master_to_kit.get(master_name)
    return bpy.data.objects.get(k) if k else None

def hang(o, parent):
    mw = o.matrix_world.copy()
    o.parent = parent
    o.matrix_parent_inverse = parent.matrix_world.inverted() if parent else Matrix.Identity(4)
    o.matrix_world = mw

for o in pulled:
    for c in list(o.children):
        hang(c, target(o.name))
for o in appended.values():
    p = rig_parent(mobj[o.name]['parent'])
    if p != mobj[o.name]['parent'] or (p and p not in appended and p not in authored):
        hang(o, target(p) if p else None)
for o in pulled:
    bpy.data.objects.remove(o, do_unlink=True)

# 3. Kit parts onto the rig.
matched, unmatched = [], []
for o in kit:
    ms = kit_to_master.get(o.name)
    if not ms:
        unmatched.append(o); continue
    p = rig_parent(mobj[ms[0]]['parent'])
    par = target(p) if p else None
    if par is o: par = None
    hang(o, par)
    matched.append(o)
for o in unmatched:
    c = o.matrix_world.translation
    near = min(matched, key=lambda m: (m.matrix_world.translation - c).length)
    hang(o, near.parent)

# 4. Names, tags, systems.
def system(e, o):
    fg, pm = e['fea_group'], e['phys_material']
    side = 'L' if o.matrix_world.translation.x > 0 else 'R'
    chain, p = [], o.parent
    while p: chain.append(p.name); p = p.parent
    turret = 'Turret_Pivot' in chain
    if fg == 'track': return f'TRACK_{side}'
    if fg == 'running_gear': return f'RUNGEAR_{side}'
    if fg.startswith('armour:'): return 'TURRET_ARMOUR' if ('turret' in fg or 'mantlet' in fg) else 'HULL_ARMOUR'
    comp = fg.split(':', 1)[1] if fg.startswith('component:') else ''
    if comp.startswith(('engine', 'radiator', 'cooling', 'fuel')): return 'ENGINE_BAY'
    if comp.startswith(('gearbox', 'steering', 'final_drive')): return 'DRIVETRAIN'
    if comp.startswith('ammo'): return 'AMMUNITION'
    if comp.startswith(('gun_', 'coax')) or fg == 'gun': return 'MAIN_GUN'
    if comp: return 'CREW_STATIONS'
    if pm == 'tool_steel_wood': return 'STOWAGE'
    if pm == 'glass_laminated': return 'OPTICS'
    return 'TURRET_FITTINGS' if turret else 'HULL_FITTINGS'
root = bpy.data.collections.new('TIGER_GEN2'); bpy.context.scene.collection.children.link(root)
cols = {}
def col(n):
    if n not in cols:
        cols[n] = bpy.data.collections.new(n); root.children.link(cols[n])
    return cols[n]
def put(o, c):
    for u in list(o.users_collection): u.objects.unlink(o)
    c.objects.link(o)
for o in kit:
    full = clean(o.name).replace('TG_RM5010-', '')
    num, _, word = full.partition('_')
    ms = kit_to_master.get(o.name, [])
    e = tags[ms[0]] if ms else dict(fea_group='fitting', phys_material='steel_structural', match='kit_only')
    o['kit_number'], o['kit_name'] = num, word or full
    o['kit_match'] = e.get('match', 'kit_only') if ms else 'kit_only'
    o['game_node'] = ms[0] if ms else ''
    if len(ms) > 1: o['game_nodes'] = ','.join(ms)
    for k in ('phys_material', 'fea_group', 'armour_nominal_mm'):
        if e.get(k) is not None: o[k] = e[k]
    sysn = system(e, o)
    o['fea_id'] = f"{sysn}/{e['fea_group']}/{num}"
    put(o, col(sysn))
    o.name = f"{sysn}.{re.sub(r'[^A-Za-z0-9]+', ' ', word or full).strip().title().replace(' ', '')}_{num}"
for n in authored:
    o = bpy.data.objects.get(n)
    if o:
        e = tags[n]; o['game_node'] = n; o['kit_match'] = 'game_authored'
        for k in ('phys_material', 'fea_group'): o[k] = e[k]
        put(o, col('GAME_AUTHORED'))
for o in appended.values():
    if o.type != 'MESH': put(o, col('RIG'))
bpy.ops.wm.save_as_mainfile(filepath=out, copy=False)
print('GEN2 original geometry' if not authored else 'GEN2 with authored parts', len(kit), 'kit parts,', len(matched), 'on master rig,', len(unmatched), 'kit-only,', len(appended), 'appended ->', out)
