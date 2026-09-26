#!/usr/bin/env python3
"""Layer a deterministic battle aftermath into the live, editable Map 1 working copy."""
import argparse
import json
import math
from pathlib import Path
import random
import numpy as np
import ruined_courtyard_live as live

live.SESSION=live.ROOT/'artifacts/authoring/live-winter-20260925'
MAP=live.ROOT/'assets/worldgen/workshops/act1-winter-courtyard-20260925'
HF=np.load(MAP/'heights.npy')
SPACING=json.loads((MAP/'manifest.v1.json').read_text())['terrain']['spacing']
PREFIX='assets/authoring/battlefield/'


def ground(x,z):
    gx=x/SPACING+(len(HF)-1)/2;gz=z/SPACING+(len(HF)-1)/2
    ix=int(gx);iz=int(gz);u=gx-ix;v=gz-iz
    return float((1-v)*((1-u)*HF[iz,ix]+u*HF[iz,ix+1])+v*((1-u)*HF[iz+1,ix]+u*HF[iz+1,ix+1]))


def build(stage):
    rng=random.Random(20260925)
    items=[]
    def put(name,x,z,y=None,yaw=0,scale=1):
        path=name if name.startswith('assets/') else live.asset(name)
        items.append({'asset':path,'position':[x,ground(x,z) if y is None else y,z],
            'yaw':yaw,'scale':[scale]*3 if isinstance(scale,(int,float)) else scale})
    if stage=='wrecks':
        put(PREFIX+'kv1_wreck_burnt.glb',1906,202,y=ground(1906,202)+.02,yaw=32)
        put(PREFIX+'pz3_wreck_burnt.glb',1855,153,y=ground(1855,153)-.10,yaw=-28)
        for x,z,yaw in [(1901.5,200,25),(1873,188.8,-40),(1856.5,159,112)]:
            put('assets/flavor/Characters/deadman-psx/baked/deadman_prone.glb',x,z,
                y=ground(x,z)+.01,yaw=yaw)
        for x,z,s in [(1906,202,4.3),(1855,153,4.1),(1901,176,2.3),(1865,181,2.1)]:
            put(PREFIX+'scorched_earth.glb',x,z,y=ground(x,z)+.025,yaw=rng.uniform(0,360),scale=[s,1,s*.78])
        for x,z,yaw,scale in [(1864,194,70,[.8,.42,1]),(1864,189,87,[.75,.65,1]),
                               (1857,163,28,[1,.28,1]),(1913,198,8,[.85,.52,1])]:
            put('fence_b_1',x,z,yaw=yaw,scale=scale)
        for x,z,yaw in [(1912,195,66),(1859,160,12)]:put('tree_log_1',x,z,yaw=yaw,scale=.8)
        put('tree_stump_1',1900,209,scale=.9)
    elif stage=='rubble':
        # Damage fans cluster against breached walls and wrecks; the central walking route stays clear.
        centers=[(1887,171,3.0),(1888,164,2.4),(1906,202,4.0),(1855,153,3.7),(1865,181,2.1)]
        for n in range(240):
            cx,cz,r=centers[n%len(centers)];a=rng.uniform(0,math.tau);d=r*math.sqrt(rng.random())
            x=cx+math.cos(a)*d;z=cz+math.sin(a)*d
            name=['brick_mx_1','debris_bricks_mx_1','stone_3','wooden_board_2'][n%4]
            scale=rng.uniform(.32,.78) if n%4==1 else rng.uniform(.5,1.15)
            if n%4==2:scale=rng.uniform(.10,.28)
            if n%4==3:scale=[rng.uniform(.3,.75),rng.uniform(.5,.9),rng.uniform(.5,1.2)]
            put(name,x,z,y=ground(x,z)+rng.uniform(.005,.035),yaw=rng.uniform(0,360),scale=scale)
        for n in range(70):
            x=rng.uniform(1871,1889);z=rng.uniform(172,190)
            if abs(x-1880)<1.3:continue
            put('brick_mx_1',x,z,y=5.415,yaw=rng.uniform(0,360),scale=rng.uniform(.35,.7))
        # Bullet strikes on the surviving front wall, with a loose diagonal burst pattern.
        for n in range(14):
            x=1882.1+(n%7)*.42+rng.uniform(-.1,.1);y=6.1+(n//7)*.45+(n%7)*.055
            put(PREFIX+'shell_chip.glb',x,171.115,y=y,scale=rng.uniform(.035,.07))
        for n in range(24):
            x=1882.3+rng.uniform(-.6,.7);z=170.4+rng.uniform(-.5,.4)
            put('ammo_pistol_loose_mp_1_casing',x,z,y=5.43,yaw=rng.uniform(0,360),scale=1)
    elif stage=='snow':
        # Low-poly drifts share one GPU prototype; uneven margins retain the paved footpath.
        for n in range(46):
            side=n%4
            x=(1870.7 if side==0 else 1889.2) if side<2 else rng.uniform(1871,1889)
            z=rng.uniform(172,189.5) if side<2 else (171.5 if side==2 else 189.9)
            radius=rng.uniform(.8,1.6)
            put(PREFIX+'snow_drift.glb',x,z,y=5.42,yaw=rng.uniform(0,360),scale=[radius,rng.uniform(.6,1.4),radius*.65])
        for x,z,y,sx,sz in [(1876,161,8.84,1.8,1.6),(1880,161,8.84,1.6,1.4),
                (1884,161,8.84,1.6,1.5),(1876,165,8.84,1.4,1.5),
                (1894,187,8.87,1.8,1.2),(1898,187,8.87,1.9,1.2)]:
            put(PREFIX+'snow_drift.glb',x,z,y=y,yaw=rng.uniform(0,360),scale=[sx,.6,sz])
        for x,z in [(1903,205),(1908,199),(1853,154),(1857,156),(1872,188.6)]:
            put(PREFIX+'snow_drift.glb',x,z,y=ground(x,z)+.025,scale=[1.1,.6,.7])
    elif stage=='lamps':
        # An improvised timber lamppost at the gate plus a wall lamp over the entrance.
        put('pillar_hr_2',1884,190,y=5.4,scale=[.12,1.38,.12])
        put('lamp_mx_4_on',1884,190,y=9.45,yaw=180,scale=1.1)
        put('lamp_mx_3_on',1880.1,171.13,y=7.98,scale=1)
    return items


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('stage',choices=['wrecks','rubble','snow','lamps'])
    args=parser.parse_args();items=build(args.stage)
    result=live.send('20-aftermath-'+args.stage,{'op':'add','label':'Winter '+args.stage,'items':items})
    print(json.dumps({'stage':args.stage,'pieces':len(items),'result':result}))
