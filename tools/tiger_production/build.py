"""Build the Tiger production master from the hand-edited game master (run inside Blender).

    TIGER_WORK=<dir with tags.json> blender --background <game master .blend> --python tools/tiger_production/build.py -- <out .blend>

Every mesh object keeps its geometry, parent, pivot and materials. It gains:
  - a readable name SYSTEM.Part_kitnumber (game-authored parts keep their own name after SYSTEM.);
  - custom properties: game_node (its name in the game's GLBs, which the engine and the bake read),
    kit_number, kit_name, kit_assembly, kit_match, phys_material, fea_group, fea_id, armour_nominal_mm;
  - membership in one system collection under TIGER (collections only; parenting is untouched);
  - an object display colour per physical material (Solid view, Object colour), materials untouched.
"""
import bpy, json, os, sys, re, colorsys
W = os.environ['TIGER_WORK']
tags = json.load(open(os.path.join(W, 'tags.json')))
out = sys.argv[sys.argv.index('--')+1]
PHYS_COLOUR = {
    'armour_rolled_homogeneous': (0.45, 0.47, 0.50), 'steel_structural_plate': (0.62, 0.64, 0.66), 'steel_structural': (0.72, 0.72, 0.70),
    'steel_cast': (0.40, 0.36, 0.33), 'steel_track_cast': (0.28, 0.24, 0.22), 'steel_roadwheel_rubber_tyred': (0.15, 0.15, 0.16),
    'steel_gun': (0.30, 0.33, 0.40), 'iron_cast_machinery': (0.55, 0.30, 0.20), 'radiator_copper_brass': (0.80, 0.50, 0.25),
    'steel_sheet_tank': (0.70, 0.62, 0.25), 'steel_exhaust': (0.45, 0.25, 0.15), 'ammunition_88mm': (0.85, 0.70, 0.30),
    'glass_laminated': (0.55, 0.80, 0.90), 'copper_cable': (0.75, 0.35, 0.20), 'seat_steel_leather': (0.35, 0.20, 0.12),
    'tool_steel_wood': (0.55, 0.40, 0.22), 'hydraulic_steel_oil': (0.20, 0.45, 0.30), 'lamp_steel_glass': (0.95, 0.90, 0.60),
    'steel_spring': (0.50, 0.50, 0.55), 'instrument': (0.85, 0.20, 0.20)}
def system(name, e, o):
    fg, pm = e['fea_group'], e['phys_material']
    side = 'L' if o.matrix_world.translation.x > 0 else 'R'
    turret = any(p.name == 'Turret_Pivot' for p in parents(o))
    if fg == 'track': return f'TRACK_{side}'
    if fg == 'running_gear': return f'RUNGEAR_{side}'
    if fg.startswith('armour:'): return 'TURRET_ARMOUR' if ('turret' in fg or 'mantlet' in fg) else 'HULL_ARMOUR'
    c = fg.split(':', 1)[1] if fg.startswith('component:') else ''
    if c.startswith(('engine', 'radiator', 'cooling', 'fuel')): return 'ENGINE_BAY'
    if c.startswith(('gearbox', 'steering', 'final_drive')): return 'DRIVETRAIN'
    if c.startswith('ammo'): return 'AMMUNITION'
    if c.startswith(('gun_', 'coax')) or fg == 'gun': return 'MAIN_GUN'
    if c: return 'CREW_STATIONS'
    if pm == 'tool_steel_wood': return 'STOWAGE'
    if pm == 'glass_laminated': return 'OPTICS'
    return 'TURRET_FITTINGS' if turret else 'HULL_FITTINGS'
def parents(o):
    p = o.parent
    while p is not None:
        yield p; p = p.parent
def word(e):
    w = re.sub(r'[^A-Za-z0-9]+', ' ', e.get('kit_name', '')).strip().title().replace(' ', '')
    return w or 'Part'
root = bpy.data.collections.get('TIGER') or bpy.data.collections.new('TIGER')
if root.name not in bpy.context.scene.collection.children: bpy.context.scene.collection.children.link(root)
cols = {}
def col(name):
    if name not in cols:
        c = bpy.data.collections.get(name) or bpy.data.collections.new(name)
        if c.name not in root.children: root.children.link(c)
        cols[name] = c
    return cols[name]
renamed = 0
for o in list(bpy.data.objects):
    if o.type != 'MESH' or o.name not in tags: continue
    e = tags[o.name]; game = o.name
    sysn = system(game, e, o)
    o['game_node'] = game
    for k in ('kit_number', 'kit_name', 'kit_assembly', 'phys_material', 'fea_group', 'armour_nominal_mm'):
        if e.get(k) is not None: o[k] = e[k]
    o['kit_match'] = e['match']
    o['fea_id'] = f"{sysn}/{e['fea_group']}/{game}"
    o.color = (*PHYS_COLOUR.get(e['phys_material'], (1, 0, 1)), 1.0)
    for c in list(o.users_collection): c.objects.unlink(o)
    col(sysn).objects.link(o)
    o.name = f"{sysn}.{word(e)}_{e['kit_number']}" if e.get('kit_number') else f"{sysn}.{game}"
    renamed += 1
# Pivots, empties, lights and cameras: into a RIG collection, names and parents untouched.
for o in bpy.data.objects:
    if o.type != 'MESH':
        for c in list(o.users_collection): c.objects.unlink(o)
        col('RIG').objects.link(o)
for scr in bpy.data.screens:
    for a in scr.areas:
        if a.type == 'VIEW_3D':
            for sp in a.spaces:
                if sp.type == 'VIEW_3D': sp.shading.color_type = 'OBJECT'
bpy.ops.wm.save_as_mainfile(filepath=out, copy=False)
print('BUILT', renamed, 'parts into', len(cols), 'collections ->', out)
