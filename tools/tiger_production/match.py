"""Match every part of the hand-edited Tiger master to its kit part (RM-5010 CAD) and tag it:
kit identity, physical material, FEA group. Frames: CAD world metres at kit scale; master = s*CAD + t,
identity rotation (fit.json). Master Blender frame: +x vehicle left, -y forward, +z up."""
import json, re, collections, numpy as np
import os, sys
D = os.environ.get('TIGER_WORK', '/tmp/claude-1000/-home-alex-panzer/a73a07a8-47a6-4503-a770-d77dc3a832d5/scratchpad/tigerblend/')
cad = json.load(open(D+'cad_parts.json')); mas = json.load(open(D+'master_parts.json')); fit = json.load(open(D+'fit.json'))
s, t = fit['s'], np.array(fit['t'])
import os
SURF = json.load(open(D+'surface.json')) if os.path.exists(D+'surface.json') else {}
def clean(n): return re.sub(r'(_îAÔO.*?)?(\.\d+)?$', '', n).strip()
for p in cad:
    p['mcen'] = s*np.array(p['cen'])+t; p['mvol'] = p['vol']*s**3
    p['mmin'] = s*np.array(p['bmin'])+t; p['mmax'] = s*np.array(p['bmax'])+t
    full = clean(p['name']); p['kit_name'] = full
    mnum = re.match(r'TG_RM5010-([\w-]+?)_(.+)$', full)
    p['kit_number'], p['kit_word'] = (mnum.group(1), mnum.group(2)) if mnum else ('', full)
    p['kit_assembly'] = clean(p['chain'][0]).replace('TG_RM5010-', '') if p['chain'] else ''
Cc = np.array([p['mcen'] for p in cad]); Vc = np.array([p['mvol'] for p in cad])
used = set(); out = {}
for m in mas:
    cen = np.array(m['cen']); vol = m['vol']; lo, hi = np.array(m['bmin'])-0.01, np.array(m['bmax'])+0.01
    d = np.linalg.norm(Cc-cen, axis=1); i = int(d.argmin())
    rec = dict(match='none', kit=[])
    if d[i] < 0.01 and abs(np.log((vol+1e-12)/(Vc[i]+1e-12))) < 0.05:
        rec = dict(match='exact', kit=[i])
    else:
        inside = [j for j in np.where(np.all((Cc >= lo) & (Cc <= hi), axis=1))[0] if Vc[j] <= vol*1.05+1e-6]
        if inside and abs(np.log((vol+1e-12)/(Vc[inside].sum()+1e-12))) < 0.15:
            rec = dict(match='merged', kit=sorted(inside, key=lambda j: -Vc[j]))
        else:
            cont = [j for j in range(len(cad)) if np.all(cen >= cad[j]['mmin']-0.01) and np.all(cen <= cad[j]['mmax']+0.01) and Vc[j] >= vol*0.95]
            if cont:
                j = min(cont, key=lambda j: Vc[j]); rec = dict(match='piece_of', kit=[j])
            elif inside:
                rec = dict(match='approx', kit=sorted(inside, key=lambda j: -Vc[j]))
    if rec['match'] != 'exact' and m['name'] in SURF:
        sv = SURF[m['name']]
        rec = dict(match='piece_of', kit=[sv['kit']], surface=sv['frac']) if sv['frac'] >= 0.6 and sv['kit'] is not None \
            else dict(match='game_authored', kit=[], surface=sv['frac'])
    for j in rec['kit']: used.add(j)
    out[m['name']] = (m, rec)

ARMOUR = {  # kit number -> (region, nominal mm, note); Tiger I Ausf. E (Jentz & Doyle, Germany's Tiger Tanks VK45.01 to Tiger E)
    '16LH01': ('hull_lower', '100 nose / 60 lower side / 25 belly', 'tub: nose plate, lower sides, belly'),
    '29Z29': ('hull_upper_side_left', '80', ''), '29Z30': ('hull_upper_side_right', '80', ''),
    '33B01': ('hull_roof', '25', 'superstructure roof and engine deck'),
    '36F03': ('hull_driver_front', '100', 'driver\'s front plate'), '28B-B04': ('hull_front', '100', 'front plate pieces'),
    '28B-K46': ('hull_front', '100', 'front plate pieces'), '30Z02': ('hull_rear', '80', ''),
    '10S03': ('turret_walls', '80', 'turret sides and rear (one bent plate)'), '04S01': ('turret_front', '100', ''),
    '12Z01': ('turret_roof', '25', 'early production roof'), '01A03': ('mantlet', '100-110', 'gun mantlet'),
}
COMP = {'28U05': 'engine_block', '28U20': 'engine_air_filters', '27K76': 'radiator_left', '27K77': 'radiator_right',
        '21K49': 'gearbox', '21K54': 'steering_unit', '21K51': 'steering_unit', '21K59': 'steering_unit', '21K57': 'radio',
        '28B-K81': 'hull_mg', '28B-K07': 'hull_mg', '20K67': 'driver_seat', '20K65': 'driver_seat', '02U03': 'gun_breech',
        '02J07': 'gun_cradle', '02J51': 'gun_cradle', '02J50': 'gun_cradle', '03J19': 'coax_mg', '01J18': 'gunner_sight',
        '05U13': 'turret_traverse_motor', '06J20B': 'gunner_seat'}
SIDED = {'27H13': 'cooling_fans', '27H25': 'cooling_fans', '27H12': 'fuel_tank', '28H21': 'fuel_tank', '28H22': 'fuel_tank',
         '29D56': 'final_drive', '18-22': 'final_drive', '18-23': 'final_drive'}
def side(cen): return 'left' if cen[0] > 0 else 'right'

def classify(p, cen):
    """phys_material, fea_group for one kit part (p) placed at master centroid cen."""
    num, word, asm = p['kit_number'], p['kit_word'].upper(), p['kit_assembly'].upper()
    base = num.split('-')[0] if num.count('-') == 0 else num
    for k, (region, nom, note) in ARMOUR.items():
        if num == k or num.startswith(k+'_') or num == k:
            return 'armour_rolled_homogeneous', f'armour:{region}', dict(armour_nominal_mm=nom, armour_note=note)
    if num in COMP: return None, f'component:{COMP[num]}', {}
    if num in SIDED: return None, f'component:{SIDED[num]}_{side(cen)}', {}
    if num.startswith('23M03') or ('AMMO' in word):
        loc = 'floor' if cen[2] < 0.75 else ('sponson_' + side(cen) + ('_front' if cen[1] < 0 else '_rear'))
        return 'ammunition_88mm', f'component:ammo_{loc}', {}
    if num.startswith('25M06') or num.startswith('24H0'):
        loc = 'sponson_' + side(cen) + ('_front' if cen[1] < 0 else '_rear')
        return None, f'component:ammo_{loc}', {}
    return None, None, {}

def material(p, cen):
    w, asm = p['kit_word'].upper(), p['kit_assembly'].upper()
    if 'TRACK UNIT' in asm or w in ('TRACK',): return 'steel_track_cast'
    if 'ROLLOR' in asm or 'ROLLOR' in w or 'ROLLAR' in w or 'WHELL' in w:
        return 'steel_roadwheel_rubber_tyred' if 'ROLLOR' in asm else 'steel_cast'
    if 'DRIVE ROLLAR' in asm or 'GUIDE ROLLER' in asm: return 'steel_cast'
    if 'GLASS' in w or 'VIEWER' in w or 'VIEW' == w or 'SCAN' in w: return 'glass_laminated'
    if 'AMMO' in w: return 'ammunition_88mm'
    if 'BARREL' in asm or w in ('BARREL', 'GUN', 'SHIELD') or 'MG34' in w or 'GUN UNIT' in asm: return 'steel_gun'
    if w in ('MAIN', 'CARBURETOR', 'CONVERTER', 'GEAR', 'MOTOR', 'CYLINDER', 'COUPLING', 'SHAFT'): return 'iron_cast_machinery'
    if w in ('COOLBOX',): return 'radiator_copper_brass'
    if w in ('FUEL', 'FILTER TANK', 'OIL BOTTLE', 'JAR'): return 'steel_sheet_tank'
    if w in ('EXHAUST', 'PIPE'): return 'steel_exhaust'
    if 'WIRE' in w or w in ('ANTENNA',): return 'copper_cable'
    if w in ('SEAT', 'CHAIR'): return 'seat_steel_leather'
    if w in ('SHOVEL', 'AXE', 'HAMMER', 'TOOL', 'PLIERS'): return 'tool_steel_wood'
    if w in ('HYDRAULIC',): return 'hydraulic_steel_oil'
    if w in ('EXTINGUISHER',): return 'steel_sheet_tank'
    if w in ('LIGHT',): return 'lamp_steel_glass'
    if w in ('SPRING',): return 'steel_spring'
    if 'ARMOR UNIT' in asm or 'COVER UNIT' in asm or 'FRONT ARMOR' in asm or 'DOOR UNIT' in asm: return 'steel_structural_plate'
    return 'steel_structural'

GAME = {'periscope glass': 'glass_laminated', 'Glass': 'glass_laminated', 'brass': 'ammunition_88mm', 'bullet': 'ammunition_88mm',
        'seats_black': 'seat_steel_leather', 'dial red': 'instrument', 'Illuminated Dial': 'instrument'}
cat = {}
counts = collections.Counter(); mats = collections.Counter(); fea = collections.Counter()
for name, (m, rec) in out.items():
    cen = np.array(m['cen'])
    entry = dict(match=rec['match'])
    if rec['kit']:
        p = cad[rec['kit'][0]]
        entry.update(kit_number=p['kit_number'], kit_name=p['kit_word'], kit_assembly=p['kit_assembly'],
                     kit_parts=[cad[j]['kit_number']+'_'+cad[j]['kit_word'] for j in rec['kit']][:12])
        pm = material(p, cen); pm2, fg, extra = classify(p, cen)
        entry['phys_material'] = pm2 or pm
        if fg is None:
            fg = ('running_gear' if entry['phys_material'] in ('steel_roadwheel_rubber_tyred', 'steel_cast') and 'ROLL' in p['kit_assembly'].upper()+p['kit_word'].upper()
                  else 'track' if entry['phys_material'] == 'steel_track_cast'
                  else 'gun' if entry['phys_material'] == 'steel_gun'
                  else 'fitting')
        entry['fea_group'] = fg; entry.update(extra)
    else:
        gm = next((GAME[x] for x in m['mats'] if x in GAME), None)
        entry.update(phys_material=gm or 'steel_structural', fea_group='fitting', note='no kit counterpart: authored or reshaped in the game master')
    cat[name] = entry
    counts[rec['match']] += 1; mats[entry['phys_material']] += 1; fea[entry['fea_group'].split(':')[0] if ':' not in entry['fea_group'] else entry['fea_group']] += 1
json.dump(cat, open(D+'tags.json', 'w'), indent=0)
print('match kinds', dict(counts)); print('kit parts used', len(used), 'of', len(cad))
print('materials', mats.most_common()); print('fea', sorted(fea.items(), key=lambda x: x[0]))
missing = [cad[j]['kit_name'] for j in range(len(cad)) if j not in used]
print('kit parts with no master counterpart', len(missing), collections.Counter(re.sub(r'^TG_RM5010-[\w-]+?_', '', x) for x in missing).most_common(12))
