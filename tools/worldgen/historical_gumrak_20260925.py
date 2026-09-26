#!/usr/bin/env python3
"""Gumrak supply-yard aftermath: sourced condition, explicitly invented local layout."""
import math
import numpy as np
from historical_scene_common_20260925 import Scene

def build():
    s=Scene('a1_gumrak_burning_stores','gumrak',half=64,spacing=.5)
    rng=np.random.default_rng(19421123)
    # A broad road and service apron, with modest surviving frozen ruts. This terrain
    # does not purport to trace a surveyed Gumrak street or airfield perimeter.
    def terrain(x,z):
        undulation=(.12*np.sin(x/13)+.10*np.cos(z/17))*np.clip((np.maximum(abs(x),abs(z))-27)/20,0,1)
        ruts=-.075*(np.exp(-((x-9.6)/.28)**2)+np.exp(-((x-12.4)/.28)**2))
        ruts*=np.clip((48-abs(z))/8,0,1)
        return undulation+ruts
    s.set_terrain(terrain)
    char_plank=s.bank.variant('wooden_plank_1','gumrak_charred_plank',[.24,.19,.15])
    char_beam=s.bank.variant('beam_hc_horizonatal_1','gumrak_charred_joist',[.20,.16,.13])
    char_crate=s.bank.variant('wooden_crate_hx_4_2','gumrak_scorched_crate',[.34,.26,.19])
    char_luggage=s.bank.variant('luggage_mp_1','gumrak_scorched_luggage',[.35,.29,.24])
    ash='assets/authoring/battlefield/scorched_earth.glb'

    # Open store, x=-18..-2, z=-8..8. Broad entrance leaves a real 4m service opening.
    for x in [-16,-12,-8,-4]:
        for z in [-6,-2,2,6]:
            s.put('floor_ceiling_hr_1',x,z,y=-.32,role='Inferred raised store floor, 8cm above service apron')
    for z in [-6,-2,2,6]:
        s.put('wall_hr_1',-18,z,y=.08,yaw=90,role='Inferred store side wall')
        if z not in [-2,2]:
            s.put('wall_hr_1',-2,z,y=.08,yaw=90,role='Store front flanks the unblocked loading opening')
    for x in [-16,-12,-8,-4]:
        s.put('window_hr_1',x,-8,y=.08,role='Rear wall with actual source window openings')
        s.put('wall_hr_1',x,8,y=.08,scale=[1,.65 if x==-4 else 1,1],role='Uneven surviving end wall; inferred fire damage')
    for x in [-18,-2]:
        for z in [-8,8]:
            s.put('pillar_hr_1_broken',x,z,y=.08,scale=[1,1.4,1],role='Exposed broken structure')
    # Rear roof survives over one bay. A lost central roof admits sky into the stores.
    for x in [-14,-6]:
        s.put('roof_mh_4_a_1',x,-6,y=3.10,role='Surviving roof strip; inferred weather protection')
    for z in [-6,-2,2,6]:
        s.put(char_beam,-10,z,y=3.05,scale=[2,.8,1],role='Burned roof joist, original source geometry')
    s.put(char_beam,-12,5,y=.12,yaw=22,scale=[1,.9,1],role='Fallen joist kept clear of central walking aisle')
    for x,z,sc in [(-13,-3,[3.5,.6,2.8]),(-9,4,[2.6,.6,2.4]),(-3,10,[1.8,.7,2.1])]:
        s.put(ash,x,z,y=.081,scale=sc,role='Cold char footprint; timing and boundary inferred')

    # Surviving and scorched crate groups share a storage purpose, not a random rubble scatter.
    for row,z in enumerate([-5.4,-3.6,4.0,5.8]):
        for col,x in enumerate([-16,-14,-12]):
            burnt=row>=2 or col==1
            key=char_crate if burnt else 'wooden_crate_hx_4'
            sc=.7 if row<2 else .58
            s.put(key,x,z,y=.08+.01*sc,yaw=(row*3-col*2),scale=sc,
                  role='Inferred containers in the Gumrak food/clothing storage setting')
            if row<2 and col!=1:
                s.put('wooden_crate_hx_4_1',x,z,y=.08+1.844*sc,yaw=5,scale=sc,
                      role='Upper surviving crate; source bounds determine stack height')
    # Generic jars/tins communicate contents without fabricated wartime brands or text.
    for i in range(22):
        x=-15.8+(i%6)*.38; z=4.8+(i//6)*.42
        s.put('tin_can_mp_1_rusty' if i%3 else 'pickles_jar_mp_1_small',x,z,y=.10,
              yaw=float(rng.uniform(-180,180)),role='Unlabelled preserved-food container; asset/type/count inferred')
    for i in range(11):
        s.put(char_luggage,-7.5+(i%3)*.65,4.1+(i//3)*.55,y=.09,yaw=-18+i*8,scale=.85,
              role='Scorched baggage evokes lost stores; no asserted individual owner or exact contents')
    for i in range(28):
        x=float(rng.uniform(-16.8,-5)); z=float(rng.choice([-1,1])*rng.uniform(3.1,7.4))
        s.put(char_plank,x,z,y=.10+(i%3)*.025,yaw=float(rng.uniform(-160,160)),
              scale=float(rng.uniform(.6,1.8)),role='Broken packaging and charred roof timber at burn footprint')

    # Domestic building surviving beside the road: only the material relationship is attested.
    # Its precise position, orientation and the dwelling-to-store relationship are invented.
    for x in [23,27]:
        for z in [-14,-10,-6]:s.put('floor_ceiling_hr_1',x,z,y=-.32,role='Inferred neighboring house floor')
    for z in [-14,-10,-6]:
        s.put('window_hr_1',29,z,y=.08,yaw=90,role='Ruined domestic side wall')
        if z!=-10:s.put('wall_hr_1',21,z,y=.08,yaw=90,scale=[1,.6 if z==-6 else 1,1],role='Partial road-facing domestic wall')
    for x in [23,27]:
        s.put('wall_hr_1',x,-16,y=.08,role='Domestic rear wall')
        s.put('window_hr_1',x,-4,y=.08,role='Domestic front wall with daylight openings')
        s.put('wall_hr_1',x,-16,y=3.08,scale=[1,.45,1],role='Fragment of upper floor silhouette')
    s.put(ash,25,-10,y=.085,scale=[3.5,.7,4.0],role='Cold fire scar inside dwelling')
    for x,z in [(23,-14),(27,-6)]:s.put('floor_ceiling_hr_1',x,z,y=2.9,role='Isolated ceiling remnant; open center admits winter sky')
    for i in range(9):
        s.put(char_beam,24.8+(i%2)*.5,-14+i*1.05,y=.10+(i%2)*.18,yaw=35+(i%3)*24,
              scale=[.45,.65,.7],role='Collapsed domestic roof timber; not a claimed photographed position')
    s.put('table_large_3',27,-13,y=.10,scale=.85,role='Restrained inferred domestic remnant')
    s.put('chair_mp_1',27.1,-11.2,y=.10,yaw=73,role='Unattributed surviving chair')

    # Channeled traffic and half-abandoned unloading: gaps are wide enough for a tank.
    for x,z in [(3,10),(3,11.5),(4.5,10),(4.5,11.5),(-2,17),(-3.6,17)]:
        s.put('wooden_crate_hx_4',x,z,yaw=-4,scale=.65,role='Surviving unloading group away from the through-road')
    for x,z in [(17,3),(18,4.2),(17.4,5.4)]:
        s.put('wooden_barrel_1',x,z,yaw=18,scale=.95,role='Generic surviving stores; contents unspecified')
    # Masonry falls concentrate beside broken edges; the approach/aisle are kept clear.
    for i in range(96):
        side=i%4
        if side==0:x,z=float(rng.uniform(-20,-17.4)),float(rng.uniform(-11,11))
        elif side==1:x,z=float(rng.uniform(-17.8,-2)),float(rng.uniform(8.2,11.8))
        elif side==2:x,z=float(rng.uniform(19.0,21.0)),float(rng.uniform(-17,-3))
        else:x,z=float(rng.uniform(21,31)),float(rng.uniform(-19.0,-16.2))
        s.put('brick_mx_1',x,z,yaw=float(rng.uniform(-180,180)),scale=float(rng.uniform(.7,1.4)),
              role='Masonry from adjacent surviving wall; local scatter inferred')
    for x,z,yaw in [(-19,-10,15),(-17,10,-20),(22,-17,40),(29,-4,10)]:
        s.put('debris_bricks_mx_1',x,z,yaw=yaw,scale=.9,role='Major collapse toe, placed outside circulation')

    s.socket('first_person_entry','player_eye',[11,1.7,29],180)
    s.socket('first_person_look_target','look_target',[-8,1.2,0])
    for i,(x,z) in enumerate([(11,25),(7,16),(1,7),(-5,0),(-12,0)]):
        s.socket(f'foot_route_{i+1:02d}','foot_route',[x,s.ground(x,z)+.08,z])
    s.connectors=[{'id':'road_north','kind':'road','position_m':[11,0,42],'heading_deg':180},
                  {'id':'road_south','kind':'road','position_m':[11,0,-42],'heading_deg':0}]
    s.view('approach',[11,2.1,29],[-9,1.5,0])
    s.view('store_threshold',[1,1.78,1],[-13,1.1,1])
    s.view('store_residue',[-7.5,1.78,1],[-13.5,.7,4.8])
    s.notes += [
        'Event: late November 1942, Gumrak. Source: anonymous 79th Infantry Division diary, Museum-Reserve KP25221, https://stalingrad1942-1943.ru/node/2723 .',
        'This is imagined cold aftermath of the observed Gumrak house/food/clothing fires. Elapsed time, snow, street plan, architecture, containers and counts are reconstruction choices.',
        'The separate Karpovka-road deliberate burning and ready weapons are OMITTED. These are not asserted to be one witnessed yard; no cause of the Gumrak fires is assigned.',
        'No exact tank type, airfield, aircraft, unit insignia, claimed ammunition depot, casualty count or named person is introduced.',
        'Textured generic jars, baggage and containers are existing game assets, not authenticated replicas of documented personal possessions.',
        'Source-model building openings and clear walking/tank lanes retained; runtime collision and visual approval require the cooked workshop review.'
    ]
    s.features += [{'kind':'inferred_cold_fire_aftermath','location':'supply-store and separate domestic shell'},
                   {'kind':'inferred_traffic_route','center_x_m':11,'width_m':7}]
    return s

if __name__=='__main__':build().save()
