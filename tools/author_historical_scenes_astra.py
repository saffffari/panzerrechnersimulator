#!/usr/bin/env python3
"""Author bounded, source-linked local scene drafts from the canonical 36-event catalog.

This creates editor data, not historical placement approval or a rendered reconstruction. Exact
sites without a verified campaign anchor remain non-loadable plans outside the editor palette.
No stock character substitutes stand in for missing horses, dogs, witnesses or specific landmarks.
"""
import hashlib
import json
import math
from pathlib import Path
import struct
import sys

ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / 'assets/worldgen/scene_blocks/historical_scene_catalog.v1.json'
BLOCKS = ROOT / 'assets/worldgen/scene_blocks/astra_20260905'
PLANS = ROOT / 'assets/worldgen/scene_plans/astra_20260905'
RECEIPT = ROOT / 'artifacts/astra-audit-20260905/historical-scene-authoring.json'

# Each row chooses a specific spatial grammar and records what that grammar cannot yet show.
# Missing hero assets are production blockers, not permission to substitute another event.
DESIGNS = {
 'a1_burned_cellar_household': ('cellar', 'Burned threshold, escaped opening, courtyard trenches, household residue', ['Burn/char material states and source-specific opening proportions need visual authoring.']),
 'a1_oil_water_rail_siding': ('water_route', 'Shelter buckets, exposed outward path and separate return containers', ['Rail cistern, railway track and contaminated-water material are missing; barrels are not substituted for cisterns.']),
 'a1_grain_elevator_extraction_convoy': ('loading', 'Service approach, dispersed loading supplies and emptied departure lane', ['Grain-elevator hero architecture, grain sacks, cart and draught animals are not present.']),
 'a1_horse_road_hospital': ('road_ward', 'Rutted road beside improvised bedding and abandoned carrying implements', ['Horse/cart/transport failure is the principal evidence and needs suitable owned hero assets.']),
 'a1_apartment_firewood': ('stripped_room', 'Domestic furniture reduced to sorted boards beside a shelter entrance', ['Furniture damage stages and heat source need authored geometry/materials; no present fire is inferred.']),
 'a1_gumrak_burning_stores': ('stores', 'Converging rutted routes, separated supply stacks and exposed hardstanding', ['Gumrak context, active store fire and period guns/vehicles remain unplaced.']),
 'a1_pitomnik_failed_evacuation': ('airfield', 'Clear exposed strip, dispersed low shelter cuts and empty aid bedding', ['Actual runway surface, aircraft and wounded evacuation figures require sourced hero assets.']),
 'a1_factory_cellar_ward': ('ward', 'Cellar bedding, separate care corner and a route between shelter spaces', ['Industrial factory shell, doctor cellar linkage and documented floor fire need further authored geometry.']),
 'a1_sanatorium_surrender_ward': ('ward', 'Ward circulation and separate surrender collection space', ['Exact sanatorium location and architectural reference alignment are pending.']),
 'a1_red_army_house_frozen_courtyard': ('ruin', 'Burned complex and open courtyard as the endpoint', ['Exact Red Army House anchor, facade and frozen courtyard evidence need source-matched production.']),
 'a1_white_sheet_surrender_road': ('surrender', 'Emptied route with a clearly separate surrender deposit zone', ['White sheet and surrendered weapons need correct assets; crates do not stand in for weapons.']),
 'a1_red_october_factory_cemetery': ('graves', 'Factory defensive ground interrupted by temporary burial traces', ['Exact Red October anchor, factory context and source-specific grave treatment remain pending.']),
 'a2_floodwater_hidden_crossing': ('crossing', 'Raised banks, low timber path and small engineer staging groups on both sides', ['Water surface, flood line and functioning crossing collision must be validated; dry earth is not called floodwater.']),
 'a2_dead_searchlight_battery': ('road', 'Raised approach, side ditches and a traffic bottleneck', ['Searchlight battery and actual traffic/period transport are missing.']),
 'a2_civilian_checkpoint_mined_bridge': ('checkpoint', 'Narrow passage between supply positions and a turned-back waiting space', ['Bridge, civilians and period demolitions equipment require dedicated assets; no invented explosive wiring.']),
 'a2_refugees_stragglers_fortress': ('road_ward', 'Converging transport traces and temporary rest positions', ['Human column, period transport and fortress context remain needed.']),
 'a2_kustrin_pompeii_block': ('ruin', 'Continuous street between isolated facades and exposed cellar mouths', ['District-scale street layout and ruined masonry silhouettes need visual review and additional modules.']),
 'a2_kustrin_surviving_chapel': ('ruin', 'Surviving chapel as one precise exception among destroyed neighbors', ['Exact chapel location, rose window and surviving architecture must not be replaced by a generic church.']),
 'a2_kustrin_replacement_bridge_train': ('crossing', 'Approach alignment toward broken spans and an interim crossing', ['Exact railway alignment, bridge spans and train remain missing.']),
 'a2_reitweiner_sporn_command_complex': ('forest_posts', 'Linked shelters and observation approaches', ['Surveyed earthwork positions and exact site anchor must precede a historical stamp.']),
 'a2_stein_stellung_trench_site': ('forest_posts', 'Paired defensive lines with rear shelter circulation', ['Surveyed trench geometry and exact site anchor must precede a historical stamp.']),
 'a2_kurmark_forest_replenishment': ('forest_posts', 'Forest supply lane connecting separated pits and stacks', ['Forest cover, period replenishment vehicles and unit-specific material evidence are pending.']),
 'a2_dog_drawn_casualty_evacuation': ('road_ward', 'A narrow evacuation route toward a low care point', ['The attested independently harnessed dogs, wooden cart and casualty are the hero evidence; no substitute tableau is spawned.']),
 'a2_only_surviving_tanker': ('assembly', 'An assembly space with one isolated position and absent crew places', ['A historically supported bandaged tanker and assembled figures need correct rig/wardrobe/poses. Empty chairs are inference, not the witness scene.']),
 'a3_halbe_refugee_relay': ('relay', 'Separate arrival and departure lanes around temporary bedding', ['Communal building identity and sourced human movement remain to be authored.']),
 'a3_buried_household_escape_variants': ('burial', 'Domestic containers below ground and a route out', ['Two witnesses remain separate variants; no combined family history or exact contents are asserted.']),
 'a3_town_hall_unused_weapons': ('stores', 'Stable storage and unfinished defensive preparation', ['Exact town hall, lower stable and unused weapons are pending.']),
 'a3_dahme_bridge_bottleneck': ('checkpoint', 'Converging approaches compressed into one passage', ['Exact Dahme crossing, water/forest constraints and mixed traffic require the named location.']),
 'a3_forsthaus_hammer_command_post': ('forest_posts', 'Forest approach to an abandoned command space', ['Exact Forsthaus Hammer anchor and building/cable evidence are pending.']),
 'a3_forsthaus_halbe_aftermath': ('ruin', 'Confined road/rail approach and photographed structural damage', ['Exact Forsthaus Halbe photo alignment and architecture are pending.']),
 'a3_twin_barricade_exits': ('checkpoint', 'Two separately named exit variants, each with its own bottleneck', ['Do not join the street exit and forest firebreak into a single invented locale. Exact variant anchors are pending.']),
 'a3_destroyed_forest_convoy': ('road', 'A road without useful bypass and overlapping blocked movement traces', ['Burned period transport is the essential missing hero layer; crates do not represent a destroyed convoy.']),
 'a3_halbe_dead_loose_horses': ('road', 'Obstructed street, broken transport route and westward survivor trace', ['Horse bodies, loose horses and broken harness are missing; do not substitute unrelated human corpses.']),
 'a3_immediate_graves': ('graves', 'Garden/corner ground and reused craters with immediate burial work', ['No grave count or individual identity is reconstructed. Soil material transitions and settlement need visual review.']),
 'a3_beelitz_evacuation_line': ('road_ward', 'Pavilion-side evacuation corridor with resting points', ['Exact pavilion layout, wounded column and defended perimeter need named-site authoring.']),
 'a3_tangermunde_bridge_crossing': ('crossing', 'Constrained one-person route toward a ruined span', ['Exact Tangermünde ruined span, photographed platform and disarmed column are pending.']),
}

def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False)+'\n')

def assets():
    want = ['wall_hr_1','wooden_crate_1','bucket_mx_1','wooden_barrel_1','wooden_board_1',
            'wooden_plank_1','beam_1','chair_mp_1','table_large_2_1','old_mattress_mx_1',
            'debris_bricks_mx_1','shovel_mx_1','lamp_mx_1_b_off']
    found={}
    for p in sorted((ROOT/'assets/pizza').rglob('*.glb')):
        if p.stem in want:
            # Prefer Mega Pack II's supplied original when names overlap between packs.
            if p.stem not in found or 'Mega Pack II' in str(p): found[p.stem]=p
    out={}
    for name in want:
        p=found[name]
        with p.open('rb') as f:
            magic,version,total=struct.unpack('<III',f.read(12))
            assert magic==0x46546c67 and version==2
            size,kind=struct.unpack('<II',f.read(8)); doc=json.loads(f.read(size))
        # The chosen assets have no transformed nodes; fail rather than silently mis-grounding.
        assert all(not any(k in node for k in ('matrix','rotation','translation','scale')) for node in doc.get('nodes',[])), name
        aa=[doc['accessors'][p['attributes']['POSITION']] for m in doc['meshes'] for p in m['primitives']]
        out[name]={'asset':str(p.relative_to(ROOT)), 'min':[min(a['min'][i] for a in aa) for i in range(3)],
                   'max':[max(a['max'][i] for a in aa) for i in range(3)], 'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
    return out

def height(kind,x,z):
    # Local authored geometry, never a claimed surveyed terrain reconstruction.
    if kind in ('cellar','ward'):
        floor=-2.2 if abs(x)<=4.0 and 0<=z<=8 else 0.0
        if abs(x)<=1.4 and -8<=z<0: floor=-2.2*(z+8)/8
        if 7<=abs(x)<=8 and -3<=z<=11: floor=-.8
        return floor
    if kind in ('road','road_ward','checkpoint','stores','loading','surrender'):
        rut=-.10*sum(math.exp(-((x-q)/.34)**2) for q in (-1.0,1.0))
        ditch=-.5*sum(math.exp(-((x-q)/.70)**2) for q in (-5.7,5.7))
        return rut+ditch
    if kind=='crossing':
        return -.8*math.exp(-(z/4.5)**4) + .14*math.exp(-((abs(z)-7)/2.3)**2)
    if kind=='forest_posts':
        pits=sum(-1.05*math.exp(-(((x-a)/2.4)**4+((z-b)/2.8)**4)) for a,b in [(-6,1),(6,5),(-6,11)])
        trench=-.6*math.exp(-((x-1)/.7)**2) if -10<z<14 else 0
        return pits+trench
    if kind=='airfield':
        return sum(-.7*math.exp(-(((x-a)/2.0)**4+((z-b)/2.4)**4)) for a,b in [(-9,0),(9,7),(-10,13)])
    if kind=='graves':
        # Three compositional work areas, not a source-derived number of victims.
        return sum(.23*math.exp(-(((x-a)/.85)**4+((z-b)/1.5)**4)) for a,b in [(-3,1),(1,4),(4,8)])-.3*math.exp(-(((x+6)/1.8)**2+((z-8)/1.8)**2))
    if kind=='burial': return -.6*math.exp(-(((x-2)/1.5)**4+((z-3)/1.3)**4))
    return 0.0

def draft(scene, bank):
    kind, sentence, gaps=DESIGNS[scene['id']]
    items=[]; evidence=[]
    def put(model,x,z,*,y=None,yaw=0,scale=(1,1,1),role='supporting object'):
        a=bank[model];idx=f'{model}_{len(items):03}'
        ground=height(kind,x,z) if y is None else y
        items.append(dict(id=idx,asset=a['asset'],dx=x,dy=round(ground-a['min'][1]*scale[1],5),dz=z,deg=yaw,s=list(scale)))
        evidence.append(dict(item_id=idx,role=role,basis='authoring_inference',source_scene=scene['id']))
    def debris(x,z,count=4):
        for i in range(count): put('debris_bricks_mx_1',x+(i%2)*1.7,z+(i//2)*2.1,yaw=31*i,role='masonry residue; exact fragment placement is inferred')
    def boards(x,z,count=6):
        for i in range(count): put('wooden_plank_1',x+(i%3)*.22,z+(i//3)*.38,yaw=12*i-20,role='removed or displaced timber')
    def ward(x,z,count=5):
        for i in range(count):
            put('old_mattress_mx_1',x+(i%2)*2.4,z+(i//2)*2.5,yaw=90,role='improvised bedding; count/composition is inferred')
        put('bucket_mx_1',x+3,z-.8,role='care-space vessel')
    def stacks(x,z,count=6):
        for i in range(count): put('wooden_crate_1',x+(i%2)*1.65,z+(i//2)*1.5,yaw=(i%3-1)*7,role='generic unmarked supply container; contents unspecified')
    def room(x,z,cellar=False):
        y=-2.2 if cellar else 0
        for dx,dz,yaw in [(-4,2,90),(-4,6,90),(4,2,90),(4,6,90),(-2,8,0),(2,8,0)]:
            put('wall_hr_1',x+dx,z+dz,y=y,yaw=yaw,role='inferred masonry enclosure; not a named facade reconstruction')
        # Broken front leaves the central approach physically open, with no fake door mesh.
        put('wall_hr_1',x-3,z,y=y,scale=(.5,.6,1),role='broken threshold side')
        put('wall_hr_1',x+3,z,y=y,scale=(.5,.45,1),role='broken threshold side')
        debris(x-5,z-2);debris(x+3,z+9)
    if kind in ('cellar','ward'):
        room(0,0,True)
        if kind=='ward': ward(-2,1,5);put('table_large_2_1',2.5,6,y=-2.2,role='separate care corner')
        else:
            put('chair_mp_1',-2,3,yaw=25);put('table_large_2_1',-2,5,scale=(.8,.8,.8));put('bucket_mx_1',1,1)
        boards(4.8,2);put('beam_1',-4.6,-1,yaw=38,role='fallen joist outside escape route')
        put('shovel_mx_1',7.4,-1,yaw=30,role='trench work residue')
    elif kind=='stripped_room':
        room(0,0);put('chair_mp_1',-2,2,yaw=20);put('table_large_2_1',-2,5,scale=(.8,.8,.8));boards(2,3,15)
        put('beam_1',4.8,3,yaw=90);put('bucket_mx_1',1.5,0)
    elif kind=='water_route':
        room(-8,0)
        for i,(x,z) in enumerate([(-7,-1),(-5,-4),(-2,-3),(1,1),(4,4),(7,7),(8,8)]):
            put('bucket_mx_1',x,z,yaw=29*i,role='bucket route; spacing is compositional, not witnessed coordinates')
        boards(-10,2);debris(-12,-1,6)
    elif kind in ('loading','stores'):
        stacks(-5,0,10);stacks(5,7,8);stacks(-8,11,4)
        for z in (-4,2,9): put('wooden_barrel_1',7.5,z,role='unmarked service vessel; contents unspecified')
        boards(-6,8);put('shovel_mx_1',5,-2,yaw=28)
    elif kind=='road_ward':
        ward(-9,1,6);stacks(6,7,4);boards(-7,-3);put('bucket_mx_1',-5,0);put('shovel_mx_1',6,-2,yaw=58)
    elif kind=='airfield':
        ward(-10,0,3);ward(8,7,2);stacks(-13,8,4)
        for x,z in [(-9,-3),(9,4),(-10,10)]: put('beam_1',x,z,role='shelter lip support')
    elif kind=='surrender':
        stacks(5,4,3);boards(-5,7);put('wooden_board_1',5,2,yaw=90,role='edge of deposit area, not a weapon or flag')
    elif kind=='crossing':
        for i in range(20): put('wooden_plank_1',0,-5.7+i*.6,y=-.28,yaw=0,scale=(1.8,1,3.15),role='provisional transverse walking board, placement inferred')
        stacks(-4,-8,4);stacks(4,8,3);boards(4,-8);put('shovel_mx_1',-4,8,yaw=20)
    elif kind=='checkpoint':
        stacks(-4,0,8);stacks(4,2,6)
        put('beam_1',-2.9,1,y=.8,yaw=20,role='partial timber barrier; center passage stays open')
        put('beam_1',3.1,1,y=.8,yaw=-15,role='partial timber barrier')
        boards(-6,-4);put('chair_mp_1',5,-1,yaw=220)
    elif kind=='ruin':
        room(-7,0);room(7,7)
        for x,z in [(-10,-4),(7,3),(10,16),(-5,12)]: debris(x,z,6)
        boards(-9,6);boards(10,8)
    elif kind=='forest_posts':
        for x,z in [(-6,1),(6,5),(-6,11)]:
            for side in (-1,1): put('beam_1',x+side*2.0,z,y=0,yaw=90,role='pit edge timber')
            put('wooden_crate_1',x,z,y=height(kind,x,z),scale=(.7,.7,.7));put('shovel_mx_1',x+2.7,z,yaw=50)
        stacks(6,-5,8);boards(-5,-5,10)
    elif kind=='assembly':
        for i in range(5):put('chair_mp_1',-4+i*1.2,5,yaw=180,role='temporary staging placeholder; not asserted as witnessed seating')
        put('chair_mp_1',2,0,yaw=165,role='isolated compositional position, no person substituted');stacks(-6,-2,3)
    elif kind=='relay':
        room(0,0);ward(-2,1,4);stacks(-7,-2,4);put('bucket_mx_1',3,5);boards(5,8)
    elif kind=='burial':
        put('wooden_crate_1',2,3,scale=(.7,.7,.7),role='unmarked buried container, exact contents unknown')
        put('shovel_mx_1',4,3,yaw=40);put('chair_mp_1',-3,6);boards(-2,5);debris(-5,1)
    elif kind=='graves':
        for x,z in [(-3,1),(1,4),(4,8)]:
            put('wooden_board_1',x+1.5,z,role='loose work material; not an invented named grave marker')
        put('shovel_mx_1',-5,7,yaw=42);put('bucket_mx_1',-4,5);boards(5,-2)
        put('wall_hr_1',-9,3,yaw=90,scale=(1,.45,1),role='domestic boundary surviving beside burial ground')
    elif kind=='road':
        stacks(-5,0,6);stacks(5,8,6);boards(4,-4,9);boards(-6,10,8)
        put('beam_1',2.8,5,yaw=37,role='obstruction beside narrow bypass');put('shovel_mx_1',-4,-2,yaw=20)
    else: raise ValueError(kind)
    spacing=1.0;res=65
    heights=[round(height(kind,x-32,z-32),5) for z in range(res) for x in range(res)]
    provenance=dict(placement_class=scene['provenance_class'],source_ids=[s['id'] for s in scene['source_records']],
        source_urls=list(dict.fromkeys(s['url'] for s in scene['source_records'])),
        placement_constraints=list(dict.fromkeys([scene['historical_region'],*scene['placement_constraints']['requires']])) )
    if scene['id']=='a1_grain_elevator_extraction_convoy':
        provenance['exact_anchor']=dict(act_id=1,landmark_id='grain_elevator',x=900,z=1100,tolerance_m=50)
    sockets=[dict(id='first_person_entry',kind='player_eye',position_m=[0,height(kind,0,-15)+1.7,-15],heading_deg=0),
             dict(id='first_person_look_target',kind='look_target',position_m=[0,1,4],heading_deg=0)]
    for i,(x,z) in enumerate([(0,-11),(0,-7),(0,-3),(0,0)]):
        sockets.append(dict(id=f'first_person_walk_{i+1:02}',kind='foot_route',position_m=[x,height(kind,x,z),z],heading_deg=0))
    # Light fixtures remain off until a source-compatible energy source/time-of-day is authored.
    # Keeping an empty runtime sound list is deliberate: a horror/music drone is not weather.
    return dict(schema='kessel.scene-block',version=1,name=f"{scene['title']} — sourced spatial draft",
        anchor=dict(origin_m=[0,0,0],forward='+z',grounding='rigid_anchor'),
        terrain=dict(datum='anchor_ground',res=res,spacing_m=spacing,apron_m=8,heights_m=heights),
        items=items,sockets=sockets,connectors=[dict(id='approach',kind='path',position_m=[0,height(kind,0,-20),-20],heading_deg=180),
            dict(id='departure',kind='path',position_m=[0,height(kind,0,20),20],heading_deg=0)],
        environment=dict(sound_emitters=[],authored_lights=[]),
        metadata=dict(status='spatial_draft_requires_hero_assets_and_visual_review',catalog_id=scene['id'],
            provenance=provenance,source_records=scene['source_records'],confidence=scene['confidence'],
            historical_integrity_boundary='Evidence supports the catalog event. This local layout, dimensions, prop choices/counts and terrain are explicit authoring inferences, not surveyed reconstruction; no campaign adjacency is asserted.',
            visual_sentence=sentence,source_visual_beats=scene['visual_beats'],item_evidence=evidence,
            missing_production_requirements=gaps,source_caveats=scene['caveats'],
            acoustic_intent=dict(status='pending_suitable_owned_diegetic_cues',listener_path='first_person_entry → first_person_walk sockets',
                separation='Foreground object sounds belong at their modeled object; exposed weather and distant activity remain separate spatial layers.',
                inference='No actual historical recording is claimed. The available inspected Echoes/ROT ambience is predominantly music/horror design; it is not substituted for witnessed wind, water, horses or human voices.'),
            lighting_intent=dict(status='pending_scene_visual_review',daylight='Physically transported sky at actual openings; no unmodeled fill light.',
                practicals='Add only source-compatible modeled fixtures; no floating amber points to manufacture drama.'),
            review=dict(camera='actual grounded player, 1.7 m foot eye',media_required='first-person video with ordinary gameplay/environment audio',rendered=False)))

def main():
    catalog=json.loads(CATALOG.read_text());bank=assets();records=[]
    assert len(catalog['scenes'])==36 and set(DESIGNS)=={s['id'] for s in catalog['scenes']}
    sys.path.insert(0,str(ROOT/'tools/worldgen'))
    from world_authoring import validate_scene
    for scene in catalog['scenes']:
        payload=draft(scene,bank)
        pending=scene['provenance_class']=='exact_landmark' and scene['id']!='a1_grain_elevator_extraction_convoy'
        if pending:
            # Retain full local composition, but never invent zero coordinates to satisfy a lock.
            path=PLANS/(scene['id']+'.scene-plan.v1.json')
            payload=dict(schema='kessel.historical-scene-plan',version=1,status='blocked_exact_site_anchor',
                catalog_id=scene['id'],pending_reason='Named site has no verified campaign representation anchor. Do not map it to a generic marker.',
                proposed_local_block=payload)
        else:
            validate_scene(payload)
            path=BLOCKS/(scene['id']+'.scene-block.v1.json')
        save(path,payload)
        records.append(dict(id=scene['id'],placement_class=scene['provenance_class'],path=str(path.relative_to(ROOT)),
            status='blocked_exact_site_anchor' if pending else 'valid_editor_spatial_draft',
            items=len(payload.get('proposed_local_block',payload)['items']),missing=DESIGNS[scene['id']][2],
            media_status='not_rendered',historical_placement_approved=False))
    save(RECEIPT,dict(schema='kessel.historical-scene-authoring-receipt.v1',catalog=str(CATALOG.relative_to(ROOT)),
        canonical_scene_count=36,editor_drafts=sum(r['status']=='valid_editor_spatial_draft' for r in records),
        exact_site_plans_pending=sum(r['status']=='blocked_exact_site_anchor' for r in records),
        assets=bank,scenes=records,validation='All editor drafts pass current world_authoring.validate_scene; raw supplied GLB bounds and node transforms checked for chosen assets. No GPU render or historical layout approval claimed.'))
    print(f"{len(records)} source-linked compositions; {sum(r['status']=='valid_editor_spatial_draft' for r in records)} editor drafts; 14 exact-site plans pending")

if __name__=='__main__': main()
