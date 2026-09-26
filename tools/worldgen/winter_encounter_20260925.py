#!/usr/bin/env python3
"""Stage a purposeful winter courtyard detail overlay; never modify the live workshop.

Reproduce: python tools/worldgen/winter_encounter_20260925.py
All layout is fictional environmental design. Existing sources and the 804 saved operations
are preserved byte-for-byte in the source and semantically in the staged sidecar.
"""
from __future__ import annotations
import copy
import hashlib
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys
import wave
from collections import Counter
import numpy as np
from historical_scene_common_20260925 import AssetBank, ROOT, ASSET_ROOT, resolve, save_json, sha
from build_winter_approach_revision import glb_geometry
import world_authoring as author

SOURCE = ROOT/'assets/worldgen/workshops/act1-winter-courtyard-20260925'
STAGED = ROOT/'assets/worldgen/workshops/act1-winter-encounter-20260925-staged'
OUT = ROOT/'artifacts/authoring/winter-encounter-20260925'
PREFAB = ROOT/'assets/worldgen/prefabs/winter-courtyard-encounter-detail-20260925.json'
TAG = 'winter_encounter_detail_20260925'
ANCHOR = np.array([1880.,5.4,165.])

def rot(deg):
    a=math.radians(deg); c,s=math.cos(a),math.sin(a)
    return np.array([[c,0,s],[0,1,0],[-s,0,c]])

def bounds(bank, item):
    lo,hi=bank.bounds(item['asset']); corners=np.array([[x,y,z] for x in [lo[0],hi[0]] for y in [lo[1],hi[1]] for z in [lo[2],hi[2]]])
    p=(corners*item['s'])@rot(item['deg']).T+[item[k] for k in ('x','y','z')]
    return p.min(0),p.max(0)

def cook_environment_audio():
    """Two bounded noise-only design cues, explicitly not recorded timber or weather."""
    sr=44100; rng=np.random.default_rng(0x20260925)
    def lowpass(x,hz):
        # Stable first-order filter; no tonal oscillator or melodic bed.
        alpha=1-math.exp(-2*math.pi*hz/sr); y=np.empty_like(x); value=0.
        for i,v in enumerate(x):value+=alpha*(v-value);y[i]=value
        return y
    def band(x,low,high):return lowpass(x,high)-lowpass(x,low)
    def envelope(t,start,length):
        u=np.clip((t-start)/length,0,1)
        return np.where((t>=start)&(t<=start+length),np.sin(math.pi*u)**2,0.)
    t=np.arange(8*sr)/sr; white=rng.normal(0,1,len(t)); wood=np.zeros_like(t)
    friction=band(white,220,2600);body=band(white,70,330)
    for onset,length,strength in [(1.8,.78,1.),(4.6,.49,.60),(6.85,.27,.40)]:
        # Irregular stick/slip bursts in a slowly rising friction envelope.
        knots=np.linspace(0,len(t)-1,math.ceil(len(t)/850))
        slip=np.interp(np.arange(len(t)),knots,rng.uniform(.08,1.,len(knots)))**2
        wood+=(friction*slip*.55+body*.20)*envelope(t,onset,length)*strength
        age=np.maximum(t-(onset+length*.67),0.)
        wood+=body*.11*np.exp(-age/0.035)*((t>=onset+length*.67)&(t<onset+length*.67+.20))
    wood*=.30/max(float(np.max(np.abs(wood))),1e-6)
    gt=np.arange(4*sr)/sr; noise=rng.normal(0,1,len(gt))
    gust=band(noise,95,2100)*envelope(gt,.45,2.8)
    flutter=np.interp(np.arange(len(gt)),np.linspace(0,len(gt)-1,38),rng.uniform(.48,1.,38))
    gust*=flutter;gust*=.18/max(float(np.max(np.abs(gust))),1e-6)
    outputs=[]
    for name,pcm,intent in [('timber_strain',wood,'Noise-based dry timber friction/contact design; not a recorded physical timber'),
                           ('breach_gust',gust,'One finite band-limited gust near the damaged opening; not an additional weather bed')]:
        pcm16=np.rint(np.clip(pcm,-1,1)*32767).astype('<i2');raw=pcm16.tobytes()
        fingerprint=hashlib.sha256(raw).hexdigest();logical=f'assets/sounds/winter_environment_20260925/{name}_{fingerprint[:12]}.ogg'
        target=ASSET_ROOT/logical;target.parent.mkdir(parents=True,exist_ok=True)
        wav=OUT/f'{name}.generated.wav';receipt=OUT/f'{name}.audio-receipt.json'
        with wave.open(str(wav),'wb') as f:f.setnchannels(1);f.setsampwidth(2);f.setframerate(sr);f.writeframes(raw)
        if target.exists():
            old=json.loads(receipt.read_text())
            assert old['pcm_sha256']==fingerprint and old['ogg_sha256']==sha(target),'Preserve different existing generated audio.'
        else:subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-nostdin','-i',str(wav),'-c:a','libvorbis','-q:a','5',str(target)],check=True)
        probe=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_entries','stream=codec_name,sample_rate,channels:format=duration','-of','json',str(target)]))
        decoded=np.frombuffer(subprocess.check_output(['ffmpeg','-hide_banner','-loglevel','error','-i',str(target),'-f','f32le','-acodec','pcm_f32le','-']),dtype='<f4')
        assert np.isfinite(decoded).all() and np.max(np.abs(decoded))<.40
        record={'asset':logical,'intent':intent,'license':'Original generated game sound; no external recording or music used.',
            'source':'This producer, deterministic seed0x20260925, NumPy white noise, bounded first-order filter bands and finite envelopes',
            'pcm_sha256':fingerprint,'ogg_sha256':sha(target),'bytes':target.stat().st_size,'ffprobe':probe,
            'decoded_peak':float(np.max(np.abs(decoded))),'decoded_rms':float(np.sqrt(np.mean(decoded**2))),
            'review_status':'Codec/finite/headroom/provenance checked. Perceptual in-game listening pending; no historical acoustic calibration claim.'}
        save_json(receipt,record);outputs.append(record)
    return outputs

def build():
    OUT.mkdir(parents=True,exist_ok=True); STAGED.mkdir(parents=True,exist_ok=True)
    before={name:sha(SOURCE/name) for name in ['manifest.v1.json','manifest_edits.json','heights.npy']}
    manifest=json.loads((SOURCE/'manifest.v1.json').read_text()); edits=json.loads((SOURCE/'manifest_edits.json').read_text())
    if TAG in edits: raise ValueError('Source already contains this pass; preserve it and review/rebase explicitly.')
    if len(edits['ops'])!=804: raise ValueError('Source authoring changed from reviewed 804 operations; review before rebasing.')
    bank=AssetBank(); hf=np.load(SOURCE/'heights.npy'); spacing=manifest['terrain']['spacing']
    def ground(x,z):return float(author.sample_height(hf,spacing,x,z))
    items=[]; notes=[]
    existing_assets=set(manifest['assets'])|{o['asset'] for o in edits['ops'] if o['op']=='add'}
    def put(key,x,z,*,floor=None,yaw=0,scale=1,beat,role,note):
        raw=bank.asset(key); lo,_=bank.bounds(raw); s=[float(scale)]*3 if np.isscalar(scale) else list(map(float,scale))
        y=(ground(x,z) if floor is None else floor)-lo[1]*s[1]
        ident=f'{TAG}_{len(items):03d}'
        item={'op':'add','asset':raw,'x':float(x),'y':float(y),'z':float(z),'deg':float(yaw),'s':s}
        items.append(item); bmin,bmax=bounds(bank,item)
        notes.append({'id':ident,'overlay_index':len(items)-1,'beat':beat,'role':role,'note':note,
            'evidence_class':'fictional_design_inference','source_ids':[],
            'world_bounds_m':{'min':bmin.tolist(),'max':bmax.tolist()},
            'interaction':'decorative only; no pickup or reward implied'})
        return item
    # Ground-floor circulation is kept clear. These are small personal objects in the
    # existing room fabric, not an invented named family's departure or death tableau.
    def home(key,x,z,**kw):return put(key,x,z,beat='domestic_shelter',**kw)
    home('luggage_mp_1',1874.62,165.0,floor=5.425,yaw=88,scale=.78,role='packed belongings',note='A modest bag beside the cupboard suggests portable possessions; identity and fate remain unspecified.')
    home('book_mp_4',1874.62,165.0,floor=5.425+.526846*.78,yaw=82,scale=.85,role='book on luggage',note='Support height measured from the bag source bounds; no invented legible diary text.')
    home('luggage_mp_1',1875.1,169.85,floor=5.425,yaw=-8,scale=.68,role='sheltered belongings',note='Placed under the surviving stair-side wall, outside the main door-to-room path.')
    home('open_book_mp_1',1875.05,169.62,floor=5.433,yaw=18,scale=.66,role='interrupted reading',note='Floor-level book beside possessions; no individual or specific historical event attributed.')
    for i in range(3):
        home(['book_mp_2','book_mp_4','book_mp_6'][i],1875.24+i*.018,160.56,floor=6.205+i*.038403,yaw=6-i*7,scale=.8,role='table books',note='Small supported stack on the existing domestic table; layer spacing uses measured 0.048m source book height at scale0.8.')
    home('plate_mp_1',1875.55,160.39,floor=6.205,yaw=0,scale=.65,role='domestic table setting',note='A second plate ties the kitchen together without asserting the former occupants or their number.')
    home('jam_jar_mp_1_small',1875.46,160.78,floor=6.205,yaw=17,scale=.63,role='stored food vessel',note='Existing period-neutral glass form; contents are neither legible nor collectible.')
    home('tin_can_mp_1_rusty',1874.58,163.6,floor=5.425,yaw=-12,scale=.7,role='shelf residue',note='Discarded vessel beneath the existing shelf, away from the internal doorway.')
    home('glass_bottle_mx_3_broken',1874.55,163.28,floor=5.425,yaw=53,scale=.65,role='broken vessel',note='Low fragments beneath the damaged west wall, not uniform floor scatter.')
    home('book_mp_4',1880.22,159.87,floor=6.073,yaw=22,scale=.63,role='bedside possession',note='Beside the existing aid bottle, leaving its work surface legible.')
    home('luggage_mp_1',1884.86,161.22,floor=5.425,yaw=177,scale=.62,role='rear shelter belongings',note='Against the wardrobe-side wall, not beside a fabricated casualty.')
    home('book_mp_2',1884.88,161.23,floor=5.425+.526846*.62,yaw=155,scale=.65,role='rear shelter book',note='Carried object nested on luggage; the original aid bed stays unchanged.')
    # A single historical motif is explicitly loose: domestic traces persist within
    # damaged standing walls. No borrowed cellar/trench geometry or other named site.
    soot='assets/authoring/battlefield/scorched_earth.glb'
    snow='assets/authoring/battlefield/snow_drift.glb'
    char='assets/authoring/historical_20260925/household_splinter_1ea7029d7216.glb'
    for x,z,sx,sz in [(1874.72,161.8,.58,1.5),(1885.15,165.0,.65,1.2),(1885.0,170.3,.75,.55),(1878.9,167.3,.38,.58)]:
        put(soot,x,z,floor=5.422,yaw=0,scale=[sx,.12,sz],beat='damage_weather',role='localized ash floor',note='Thin reused scorch mesh directly below damaged fabric; avoids tinting the white landscape.')
    for x,z,yaw in [(1874.6,161.7,84),(1874.75,162.0,73),(1874.8,162.25,111),
                     (1885.1,165.1,13),(1885.12,165.4,-8),(1885.3,165.7,36),
                     (1885.35,170.4,62),(1885.15,170.62,104),(1884.83,170.48,33),
                     (1878.9,167.15,17),(1879.02,167.3,-12),(1878.8,167.48,23)]:
        put(char,x,z,floor=5.435,yaw=yaw,scale=[.45,.65,.55],beat='damage_weather',role='charred splinter',note='Reuses one existing immutable soot variant; grouped below wall/beam damage and outside the standing path.')
    for x,z,sx,sz in [(1874.53,161.0,.4,.77),(1874.57,169.0,.42,.68),(1885.45,168.7,.4,.72),
                     (1885.32,170.3,.52,.48),(1879.12,171.55,.28,.48),(1880.94,171.56,.28,.51),
                     (1878.55,190.32,.38,.62),(1881.4,190.32,.38,.62)]:
        put(snow,x,z,floor=5.425,yaw=13,scale=[sx,.48,sz],beat='damage_weather',role='opening snow tongue',note='Shallow drift hugs exposed window/door jamb; the centre of the opening remains clear.')
    # Two visible but non-lootable maintenance caches establish a material route.
    # They flank existing physical salvage, never replace or multiply the twelve rounds.
    for x,z,yaw in [(1864.4,225.0,18),(1877.35,194.4,70),(1901.0,215.0,-22)]:
        put('supply_crate_1_1_empty',x,z,yaw=yaw,scale=.85,beat='search_route',role='empty carrying crate',note='Open empty crate is a visual search cue; there is no hidden inventory here.')
    put('toolbox_mx_1',1901.05,215.7,yaw=-18,scale=.70,beat='wreck_salvage',role='abandoned maintenance tool box',note='Outside the real port-track ammunition area; non-interactive set dressing.')
    put('wooden_plank_1',1900.8,216.3,yaw=12,scale=[.8,.7,.8],beat='wreck_salvage',role='packing slat',note='Short packing residue leads toward the actual exposed cartridges; no fake ammunition mesh.')
    put('wooden_plank_1',1901.0,216.45,yaw=-12,scale=[.6,.7,.8],beat='wreck_salvage',role='packing slat',note='Second slat forms an irregular small group, not a trail of collectible markers.')
    for x,z,sx,sz in [(1908.5,218.4,1.4,1.7),(1907.4,215.0,1.1,.7),(1904.9,222.6,.85,.7)]:
        put(soot,x,z,yaw=32,scale=[sx,.17,sz],beat='wreck_salvage',role='wreck soot residue',note='Localized under/adjacent to the burnt vehicle; true port-side salvage remains on pale snow.')
    for x,z in [(1908.8,220.2),(1907.8,221.8),(1905.7,224.0),(1904.1,224.7)]:
        put(snow,x,z,yaw=32,scale=[.65,.62,.42],beat='wreck_salvage',role='wreck leeward snow',note='Thin accumulation on the outer aftermath edge, clear of rounds and approach.')
    # Controlled edge fragments give the route direction without pavement or fences.
    for x,z,yaw in [(1867.5,220.4,15),(1867.3,220.6,-12),(1874.0,205.4,68),
                     (1874.2,205.8,13),(1882.2,196.6,35),(1882.5,196.7,98),
                     (1891.5,201.0,44),(1891.7,201.3,9),(1898.5,209.5,18),
                     (1898.8,209.7,47),(1881.7,234.4,121),(1882.,234.6,14)]:
        put('wooden_plank_1',x,z,yaw=yaw,scale=[.52,.75,.65],beat='search_route',role='displaced packing timber',note='Paired edge cluster; authored world-space detail does not follow the camera or fill the snow gaps.')
    for x,z in [(1864.0,223.8),(1874.,204.5),(1882.3,195.7),(1892.2,200.9),(1899.5,210.0),(1883.,234.0)]:
        put(snow,x,z,yaw=26,scale=[.85,.65,.40],beat='search_route',role='edge drift',note='Low wind-shaped group beside, not across, the route; existing white snow terrain retained.')

    # Route is a readable loop, not a promise that walking alone takes ten minutes.
    # Requested playtime includes search, situational awareness, physical carry/stow and return.
    route_xz=[('tank_to_gate',[(1853.8,248),(1860,232),(1865,218),(1874,201),(1880,194),(1880,188)]),
        ('house_entry',[(1880,188),(1880,178),(1880,172.5),(1880,170),(1879.25,168),(1879.25,165)]),
        ('domestic_room',[(1879.25,165),(1877.2,165),(1876.5,165.5),(1875.7,164.8),(1875.7,162.7),(1875.7,164.8),(1876.5,165.5),(1877.2,165)]),
        ('rear_room',[(1877.2,165),(1879.1,165),(1880,164.4),(1880,162.2),(1879.65,160.6)]),
        ('house_to_wreck',[(1879.65,160.6),(1880,162.2),(1880,164.4),(1879.25,165),(1879.25,168),(1880,170),(1880,178),(1880,194),(1889,198),(1895,207),(1902.2,212.8),(1902.8,216),(1903.2,218.0)]),
        ('wreck_to_tank',[(1903.2,218.0),(1901.2,220.7),(1897,227),(1885,235),(1868,242),(1853.8,248)])]
    routes=[{'id':name,'points_m':[[x,5.425 if 1873<=x<=1886 and 159<z<171.8 else ground(x,z),z] for x,z in points]} for name,points in route_xz]
    proposed_lights=copy.deepcopy(edits['authored_lights'])
    tune={'ruined_house_command':(8.,4.5),'ruined_house_aid':(4.,3.5),'ruined_house_kitchen':(6.,3.5),
          'ruined_house_gate':(7.,4.5),'winter_entrance':(26.,7.5),'winter_courtyard_gate':(58.,12.)}
    for light in proposed_lights:
        if light['id'] in tune:light['radiant_intensity'],light['range_m']=tune[light['id']]
    audio=cook_environment_audio()
    sounds=[]
    for asset,pos,gain,radius in [(audio[0],[1878,8.65,167],.20,8.),(audio[1],[1885.8,6.6,166],.13,9.)]:
        sounds.append({'id':TAG+'_'+Path(asset['asset']).stem,'asset':asset['asset'],'position_m':pos,
            'volume':gain,'radius_m':radius,'duration_s':0.,'playback':'once','provenance':asset,
            'intent':'Localized finite physical-source suggestion; one trigger per playthrough, no looping or artificial occupant.'})
    views=[]
    for ident,eye,target in [('gate_approach',[1876.,7.25,195.],[1880,7,172]),
        ('domestic_shelter',[1877.2,7.14,164.8],[1874.9,6.1,161.8]),
        ('rear_room',[1880.,7.14,162.0],[1884.5,6.1,160.8]),
        ('salvage_search',[1902.6,ground(1902.6,216.2)+1.7,216.2],[1905.,7.,218.3])]:
        delta=np.asarray(target)-eye
        views.append({'id':ident,'eye_m':eye,'target_m':target,'yaw_deg':math.degrees(math.atan2(delta[0],-delta[2])),
                      'pitch_deg':math.degrees(math.atan2(delta[1],math.hypot(delta[0],delta[2])))})
    motif={'classification':'fictional winter atmosphere study','motif':'Domestic material traces within damaged standing walls',
        'reference_catalog_id':'a1_burned_cellar_household',
        'boundary':'One loose household motif only. No named house, family, exact death, surveyed layout, cellar escape or real adjacency is claimed. The separate historical workshops are unchanged. Every prop/count/position here is a design inference.'}
    integration={'source_manifest':str(SOURCE/'manifest.v1.json'),'source_sha256':before,
        'preserved_existing_ops':804,'added_ops':len(items),'new_gpu_asset_paths':sorted({o['asset'] for o in items}-existing_assets),
        'routes':routes,'review_views':views,'historical_boundary':motif,
        'encounter_pacing':{'scope':'An intended ~10 minute search/carry/reload encounter, not a timed or gameplay-completed claim.',
            'beats':['Leave tank and read lamp-lit shelter (1–2 min)','Search existing domestic rooms and inspect the damaged openings (2–3 min)','Follow courtyard edge to visibly burnt Tiger and inspect physical rounds (1–2 min)','Carry finite 88mm salvage back and hand-stow/reload with ordinary controls (3–4 min)'],
            'gameplay_dependency':'Root owns trigger/encounter tuning. This overlay adds no artificial loot, new ammunition, combatant, scripted threat, timer or rewards.'},
        'audio_sources':sounds,'audio_voice_budget':2,'audio_policy':'Two once-per-playthrough localized generated cues through existing spatial scene audio; weather bed unchanged; no continuous duplicate wind or voices.',
        'light_changes':[{'id':a['id'],'before':{'intensity':a['radiant_intensity'],'range_m':a['range_m']},'staged_after':{'intensity':b['radiant_intensity'],'range_m':b['range_m']}} for a,b in zip(edits['authored_lights'],proposed_lights)],
        'license':'Existing locally purchased Pizza assets and existing owner-private derived variants; no standalone redistribution; originals unchanged.'}
    # Mirror only the documented runtime seed positions for a clearance receipt.
    # The scene creates zero ammunition bodies; root retains the real finite system.
    hull=next(o for o in edits['ops'] if o.get('asset','').endswith('tiger_wreck_20260925_hull.glb') and abs(o['x']-1907)<.01 and abs(o['z']-218)<.01)
    rounds=[];salvage_intrusions=[]
    for index in range(12):
        p=rot(hull['deg'])@np.array([-2.15-(index%3)*.13,0,-1.1+(index//3)*.34])+[hull[k] for k in ('x','y','z')]
        p[1]=ground(p[0],p[2])+.053
        rounds.append({'runtime_index_within_hull':index,'nature':'HE' if index>=9 else 'AP','position_m':p.tolist()})
        for n,o in enumerate(items):
            lo,hi=bounds(bank,o); distance=float(np.linalg.norm(np.maximum(np.maximum(lo[[0,2]]-p[[0,2]],p[[0,2]]-hi[[0,2]]),0)))
            if distance<.65:salvage_intrusions.append({'round':index,'overlay_item':n,'distance_to_added_aabb_m':distance})
    assert not salvage_intrusions, f'Added detail encroaches on finite physical salvage: {salvage_intrusions}'
    integration['salvage_clearance']={'source':'src/wreck_salvage.rs::seed_wreck_salvage','hull_pose':{k:hull[k] for k in ('asset','x','y','z','deg','s')},
        'rounds_created_by_overlay':0,'existing_runtime_rounds':rounds,'added_aabb_exclusion_radius_m':.65,'intrusions':salvage_intrusions,
        'boundary':'Check protects the current runtime seed positions. It does not create inventory, guarantee native pickup or reserve falling/rolling shell trajectories.'}
    overlay={'schema':'kessel.winter-encounter-overlay.v1','version':1,'id':TAG,'base':integration,'ops':items,'item_notes':notes,
        'authored_lights':proposed_lights,'sound_emitters':sounds}
    save_json(OUT/'overlay.v1.json',overlay)
    staged=copy.deepcopy(edits);staged['ops'].extend(items);staged['authored_lights']=proposed_lights;staged['sound_emitters'].extend(sounds);staged[TAG]=integration
    for name in ['manifest.v1.json','heights.npy','features.json','level_design.v1.json']:
        if (SOURCE/name).is_file():shutil.copyfile(SOURCE/name,STAGED/name)
    save_json(STAGED/'manifest_edits.json',staged)
    prefab={'name':'winter-courtyard-encounter-detail-20260925','grounding':'rigid_anchor',
        'items':[{'asset':o['asset'],'dx':o['x']-ANCHOR[0],'hoff':o['y']-ANCHOR[1],'dz':o['z']-ANCHOR[2],'deg':o['deg'],'s':o['s']} for o in items],
        'environment':{'authored_lights':[],'sound_emitters':[{**s,'position_m':(np.asarray(s['position_m'])-ANCHOR).tolist()} for s in sounds]},'metadata':{'anchor_m':ANCHOR.tolist(),'role':'detail overlay for the existing courtyard assembly; not a complete standalone building',
            'historical_reconstruction':False,'item_notes':notes,'historical_boundary':motif,'integration_note':'Existing practical-light adjustments live only in the staged sidecar, never duplicate lamps when inserting this prefab.'}}
    save_json(PREFAB,prefab)
    author.load_world((STAGED/'manifest.v1.json').relative_to(ROOT).as_posix())
    receipt={'schema':'kessel.winter-encounter-receipt.v1','staged_manifest':str(STAGED/'manifest.v1.json'),'overlay':str(OUT/'overlay.v1.json'),'prefab':str(PREFAB),
        'source_preserved':before,'original_ops_preserved':staged['ops'][:804]==edits['ops'],
        'added_items':len(items),'unique_added_assets':len({o['asset'] for o in items}),'new_gpu_asset_paths':integration['new_gpu_asset_paths'],
        'added_triangles':sum(bank.inspect(o['asset'])['triangles'] for o in items),'roles':dict(Counter(n['role'] for n in notes)),
        'asset_records':list(bank.records.values()),'audio_sources':audio,'audio_voice_budget':2,'validation':'Canonical source existence, actual GLB bounds, authoring world validation, generated audio codec/headroom/provenance; native traversal/GPU appearance/performance and perceptual sound review pending root review.',
        'review_views':views,'routes':routes,'historical_boundary':motif}
    assert before=={name:sha(SOURCE/name) for name in before},'Live source changed during staging; reject stale integration.'
    save_json(OUT/'receipt.v1.json',receipt)
    return bank,manifest,edits,items,routes,ground,receipt

def check_routes(bank, manifest, edits, additions, routes, ground):
    """Source-triangle body samples, not a native capsule or procedural actor proof."""
    sys.path.insert(0,str(ROOT/'tools'));from validate_tiger_hatches import vertical_hits
    world=[]
    for i,p in enumerate(manifest['placements']):
        if 1835<=p[1]<=1920 and 150<=p[3]<=265:world.append((f'base_{i}',{'asset':manifest['assets'][p[0]],'x':p[1],'y':p[2],'z':p[3],'deg':p[4],'s':p[5] if isinstance(p[5],list) else [p[5]]*3}))
    for n,items in [('saved',edits['ops']),('new',additions)]:
        for i,o in enumerate(items):
            if o['op']=='add' and 1835<=o['x']<=1920 and 150<=o['z']<=265:world.append((f'{n}_{i}',o))
    cache={}; meshes=[]
    for ident,o in world:
        lo,hi=bounds(bank,o)
        if not any(any(lo[0]-.5<=p[0]<=hi[0]+.5 and lo[2]-.5<=p[2]<=hi[2]+.5 for p in r['points_m']) for r in routes):
            # Segment candidates are admitted too, even when no waypoint touches the box.
            if not any(any(min(a[0],b[0])-.5<=hi[0] and max(a[0],b[0])+.5>=lo[0] and min(a[2],b[2])-.5<=hi[2] and max(a[2],b[2])+.5>=lo[2] for a,b in zip(r['points_m'],r['points_m'][1:])) for r in routes):continue
        if o['asset'] not in cache:cache[o['asset']]=glb_geometry(resolve(o['asset']))[0]
        t=(cache[o['asset']]*o['s'])@rot(o['deg']).T+[o[k] for k in ('x','y','z')]
        meshes.append((ident,o['asset'],t,lo,hi,t.min(1),t.max(1)))
    contacts=[];checks=[]; step=.3;radius=.28;height=1.8288;low_step=.25
    for route in routes:
        for a,b in zip(route['points_m'],route['points_m'][1:]):
            a=np.array(a);b=np.array(b)
            for w in np.linspace(0,1,math.ceil(np.linalg.norm((b-a)[[0,2]])/step)+1):
                p=a+(b-a)*w
                if not(1873<=p[0]<=1886 and 159<p[2]<171.8):p[1]=ground(p[0],p[2])
                checks.append({'route':route['id'],'feet':p.tolist()})
                for ox,oz in [(0,0),(radius,0),(-radius,0),(0,radius),(0,-radius),(.2,.2),(-.2,.2),(.2,-.2),(-.2,-.2)]:
                    x,z=p[0]+ox,p[2]+oz
                    for ident,raw,t,lo,hi,tlo,thi in meshes:
                        if not(lo[0]<=x<=hi[0] and lo[2]<=z<=hi[2] and hi[1]>p[1]+low_step and lo[1]<p[1]+height):continue
                        subset=t[(tlo[:,0]<=x)&(thi[:,0]>=x)&(tlo[:,2]<=z)&(thi[:,2]>=z)&(thi[:,1]>p[1]+low_step)&(tlo[:,1]<p[1]+height)]
                        if len(subset)==0:continue
                        hits=vertical_hits(subset.reshape(-1,3),np.arange(subset.size//3),(x,z,p[1]+low_step,p[1]+height))
                        if hits:contacts.append({'route':route['id'],'feet':p.tolist(),'item':ident,'asset':raw,'hits':hits})
    result={'schema':'kessel.winter-encounter-route-check.v1','scope':'Nine vertical body rays at <=0.30m route spacing, actual source triangle positions; excludes dynamic actors and runtime procedural foliage. Not a swept capsule, support continuity or native interaction proof.',
        'body_radius_m':radius,'stature_m':height,'low_step_allowance_m':low_step,'low_step_rationale':'Existing gate snow is <0.20m high. Source check ignores geometry below 0.25m, within native 0.30m step envelope; native support/collision still requires review.','sample_count':len(checks),'rays':len(checks)*9,
        'candidate_objects':len(meshes),'contacts':contacts,'new_detail_clear':not any(c['item'].startswith('new_') for c in contacts),'all_authored_clear':not contacts,'samples':checks}
    save_json(OUT/'route-clearance.v1.json',result)
    print(json.dumps({'new_detail_clear':result['new_detail_clear'],'all_authored_clear':result['all_authored_clear'],'contacts':len(contacts),'contact_items':dict(Counter(c['item'] for c in contacts))}))
    return result

if __name__=='__main__':
    bank,m,e,items,routes,ground,receipt=build(); check=check_routes(bank,m,e,items,routes,ground)
    print(json.dumps({k:receipt[k] for k in ['staged_manifest','added_items','unique_added_assets','new_gpu_asset_paths','added_triangles']}))
