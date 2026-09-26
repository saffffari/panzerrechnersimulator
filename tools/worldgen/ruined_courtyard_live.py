#!/usr/bin/env python3
"""Send incremental, editable Pizza assemblies to an already open world editor.

Every stage is additive and receives actual placement indices from the editor. Existing
stage receipts prevent accidental resubmission. Original GLBs remain untouched.
"""
import argparse
import json
import math
import os
from pathlib import Path
import random
import struct
import time

ROOT = Path(__file__).resolve().parents[2]
ASSETS = Path('/home/alex/panzer/tank_simulator')
SESSION = Path(os.environ.get('PANZER_EDITOR_LIVE_DIR', ROOT / 'artifacts/authoring/live-20260925'))
SOURCE = ASSETS / 'assets/pizza/_extracted'
PATHS = list(SOURCE.rglob('*.glb'))


def asset(name):
    matches = [p for p in PATHS if p.stem == name]
    # Duplicate ordinary crates exist in both packs. Use the matching second pack family.
    matches.sort(key=lambda p: ('PSX Mega Pack II' not in str(p), str(p)))
    if not matches:
        raise ValueError(f'Missing Pizza asset {name}')
    return matches[0].relative_to(ASSETS).as_posix()


def send(name, command):
    receipt = SESSION / 'outbox' / f'{name}.json'
    if receipt.exists():
        return json.loads(receipt.read_text())
    target = SESSION / 'inbox' / f'{name}.json'
    claimed = target.with_suffix('.processing')
    if not target.exists() and not claimed.exists():
        pending = target.with_suffix('.pending')
        pending.write_text(json.dumps(command, indent=2) + '\n')
        pending.rename(target)
    for _ in range(180):
        if receipt.exists():
            result = json.loads(receipt.read_text())
            if not result['ok']:
                raise RuntimeError(result['error'])
            return result
        time.sleep(.25)
    raise TimeoutError('Editor has not acknowledged this stage; it may be in a user transform. Request remains queued.')


def build(stage):
    items = []

    def put(name, x, y, z, yaw=0, scale=1):
        items.append({'asset': asset(name), 'position': [x,y,z], 'yaw':yaw,
                      'scale':[scale]*3 if isinstance(scale,(int,float)) else scale})

    if stage == '01-foundation':
        for x in [-4,0,4]:
            for z in [-4,0,4]:
                if (x,z) != (-4,-4):  # already the manifest's initial placement
                    put('floor_ceiling_hr_1',x,-.38,z)
        for x in [-8,-4,0,4,8]:
            for z in [8,12,16,20,24]:
                put('asphalt_hr_1',x,.006,z)
        # Porch and threshold, leaving a clear central route to the courtyard gate.
        put('floor_ceiling_hr_1',0,-.32,7,scale=[.6,.85,.5])
    elif stage == '02-house-shell':
        for x,n in [(-4,'window_hr_1'),(0,'window_hr_1'),(4,'wall_hr_1')]:
            put(n,x,.02,-6)
        for z,n in [(-4,'window_hr_1'),(0,'wall_hr_1'),(4,'window_hr_1')]:
            put(n,-6,.02,z,90)
        put('wall_hr_1',6,.02,-4,90)
        put('wall_hr_1_m1a_1',6,.02,0,90)
        put('wall_rx_1_1_3_height',6,.02,4,90)
        put('window_hr_1',-4,.02,6,180)
        put('doorway_hr_1',0,.02,6,180)
        put('wall_hr_1_hole_2',4,.02,6,180)
        # Interior partition: kitchen, stair hall, occupied map room.
        put('wall_hr_1',-2,.02,-4,90)
        put('doorway_hr_1',-2,.02,0,90)
        put('doorway_hr_1',0,.02,-2)
        put('wall_hr_1',4,.02,-2)
        # Upper storey survives along the back and left wing. The southeast is open to sky.
        for x,z in [(-4,-4),(0,-4),(4,-4),(-4,0)]:
            put('floor_ceiling_hr_1',x,3.02,z)
        for x,n in [(-4,'window_hr_1'),(0,'window_hr_1'),(4,'wall_hr_1_m2a_1')]:
            put(n,x,3.42,-6)
        put('window_hr_1',-6,3.42,-4,90)
        put('wall_hr_1_m1a_2',-6,3.42,0,90)
        put('wall_rx_1_1_3_height',6,3.42,-4,90)
        put('wall_rx_1_m1a_1_1_3_height',0,3.42,-2)
        put('stairs_5_wood',-4,.02,3,180,[1,3.4/3,1])
        # Intact west roof strip; roof loss exposes upper rooms and structural remains.
        put('roof_mh_4_a_1',-2,6.42,-4)
        put('pillar_hr_1_broken',6,.02,6)
        put('pillar_hr_1_broken',2,3.42,-2,scale=[.65,.8,.65])
        put('beam_hc_horizonatal_1',-2,3.25,2,90,scale=[1,.6,.6])
    elif stage == '03-interiors':
        # Map/command room: clear 1m passages around the work surface.
        put('table_large_3',2.2,.03,1.2,90,1.15)
        put('map_mx_1',2.2,1.079,1.2,90,.8)
        put('chair_mp_2',.9,.03,1.1,90,1.35)
        put('chair_mp_2',3.6,.03,.5,-90,1.35)
        put('open_book_mp_1',2.2,1.079,2.0,-20,.7)
        put('ashtray_1',2.2,1.084,.4,0,.85)
        put('shelf_mp_3',4.8,.03,-.9,180,.85)
        put('supply_crate_1',4.8,.03,1.1,90,.8)
        put('supply_crate_1_1_empty',4.9,.03,3.0,0,.75)
        put('first_aid_kit_hr_1',4.6,.65,1.1,0,.65)
        put('jerrycan_mx_1',5.2,.03,4.8,-12,.8)
        put('luggage_mp_1',3.8,.03,4.8,14,.8)
        # Rear room doubles as a small aid station.
        put('bed_1',2.5,.03,-4.2,0,.83)
        put('bedside_table_1',.5,.03,-5.0,0)
        put('first_aid_kit_hr_2',.5,.68,-5.0,0,.7)
        put('bucket_mx_1',1,.03,-3.0,0,.75)
        put('wardrobe_mp_1',5,.12,-5.1,180,.75)
        # Domestic west room: tableware and preserved cupboards among loose bricks.
        put('table_large_3',-4.6,.03,-4,90,.85)
        put('plate_mp_1',-4.6,.81,-4,0,.9)
        put('tin_can_mp_1_rusty',-4.6,.81,-4.55,0,.9)
        put('glass_bottle_mx_1',-4.6,.81,-3.5,0,.8)
        put('chair_mp_2',-3.2,.03,-4,-90,1.25)
        put('shelf_mp_2',-5.1,.03,-1,90,.8)
        put('wooden_crate_hx_2',-3.2,.03,-1.4,-6,.75)
        put('bucket_mx_2',-5.2,.03,1.1,0,.85)
        put('jam_jar_mp_1_small',-4.85,.65,-1.1,0,.8)
        # Upstairs bedroom. Roof loss leaves the broken second room legible in orbit.
        put('bed_1',-3.9,3.43,-4.2,90,.82)
        put('bedside_table_2',-2.55,3.43,-5.1,0,.9)
        put('book_mp_4',-2.55,4.05,-5.1,12,.8)
        put('wardrobe_mp_1',.6,3.51,-5.2,180,.78)
        put('old_mattress_mp_1',3.8,3.43,-4.1,15,.8)
        put('wooden_crate_1',1.1,3.43,-3.0,-8,.7)
        put('painting_1',-5.73,4.65,-3.7,90,.75)
    elif stage == '04-courtyard':
        # Low masonry enclosure with a 4m gate and service openings.
        for z in [10,14,18,22]:
            put('wall_hr_1',-10,.02,z,90,[1,.4,1])
            put('wall_hr_1',10,.02,z,90,[1,.4,1])
        for x in [-8,-4,4,8]:
            put('wall_hr_1',x,.02,26,0,[1,.4,1])
        for x,z in [(-10,8),(-10,26),(10,8),(10,26),(-2,26),(2,26)]:
            put('pillar_hr_2',x,.02,z,0,[1,.55,1])
        # Supply corner, covered wood stack, maintenance table.
        for x,z in [(-8.3,10),(-7,10),(-8.3,11.4),(-7,11.4)]:
            put('wooden_crate_hx_4',x,.02,z,0,.9)
        put('wooden_crate_hx_4',-8.3,.85,10,0,.75)
        put('supply_crate_1',-7.4,.02,13,90,.9)
        put('table_large_3',-8.7,.02,16,90,1.0)
        put('toolbox_mx_1',-8.7,.94,16,0,.75)
        put('jerrycan_mx_1',-7.6,.02,17.2,10,.85)
        put('metal_barrel_hr_1',8,.02,11,0,.95)
        put('metal_barrel_hr_2',8.8,.02,12.3,0,.95)
        put('wooden_barrel_1',7.4,.02,13.1,0,.9)
        put('ladder_hr_1_short',-6.7,.02,4.0,90,1.0)
        for j in range(6):
            put('wooden_plank_1',-8+(j%2)*1.3,.04+(j//2)*.07,20+(j%2)*.12,8,1.4)
        put('bench_mx_1',7.8,.02,19,90,1)
        put('bucket_mx_3',8.3,.02,20.5,0,.8)
        # Small gate sentry shelter with a furnished, open entrance.
        for x in [14,18]: put('floor_ceiling_hr_1',x,-.38,22)
        put('window_hr_1',14,.02,20)
        put('wall_hr_1',18,.02,20)
        put('window_hr_1',12,.02,22,90)
        put('wall_hr_1',20,.02,22,90)
        put('doorway_hr_1',14,.02,24,180)
        put('window_hr_1',18,.02,24,180)
        put('roof_mh_4_a_1',16,3.02,22)
        put('table_large_3',18,.03,21.5,0,1)
        put('map_mx_1',18,.945,21.5,0,.8)
        put('chair_mp_2',18,.03,22.8,180,1.3)
        put('supply_crate_1',13,.03,21,0,.8)
        put('jerrycan_mx_1',19,.03,23,0,.85)
    elif stage == '05-damage-detail':
        rng=random.Random(925)
        # Damage follows the southeast breach; keep the doorway and central lane clear.
        for i in range(58):
            x,z=rng.uniform(2.5,8.5),rng.uniform(3,9.5)
            put(['debris_bricks_mx_1','debris_bricks_mx_2'][i%2],x,.024,z,rng.uniform(0,360),rng.uniform(.25,.65))
        for i in range(28):
            x,z=rng.uniform(3,7.5),rng.uniform(3,8.8)
            put('bricks_stacked_mx_1',x,.025,z,rng.uniform(0,360),rng.uniform(.6,1.5))
        for i in range(12):
            put('wooden_plank_1',rng.uniform(1.8,7.5),.08,rng.uniform(2,8),rng.uniform(0,360),rng.uniform(.8,2))
        for x,z in [(-5.4,7.2),(-7,4.8),(5.2,-1.1),(4.8,-5.3),(-1,-4.7)]:
            put('debris_bricks_mx_1',x,.025,z,rng.uniform(0,360),.7)
        for i in range(14):
            put('brick_mx_1',rng.uniform(-1.5,5.8),3.44,rng.uniform(-5.3,-2.5),rng.uniform(0,360),rng.uniform(.7,1.3))
        for i in range(6):
            put('wooden_plank_2',rng.uniform(-1,4.8),3.46,rng.uniform(-5,-2.5),rng.uniform(0,360),.9)
        # Courtyard wear is clustered against enclosure walls, away from foot and tank paths.
        for i in range(20):
            x=(-1 if i%2 else 1)*rng.uniform(8.4,9.4)
            put('gravel_pile_hr_1',x,.008,rng.uniform(9,24),rng.uniform(0,360),rng.uniform(.25,.5))
    else:
        raise ValueError(stage)
    return {'op':'add','label':stage[3:].replace('-',' '),'items':items}


if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('stage')
    args=parser.parse_args()
    command=build(args.stage)
    result=send(args.stage,command)
    print(json.dumps({'stage':args.stage,'pieces':len(command['items']),'reply':result}))
