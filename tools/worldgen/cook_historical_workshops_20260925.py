#!/usr/bin/env python3
"""Cook independent editable historical workshops without stamping the live winter map."""
import argparse
import copy
import hashlib
import json
import math
from pathlib import Path
import numpy as np
from historical_scene_common_20260925 import ROOT, ASSET_ROOT, BLOCK_ROOT, RECEIPT_ROOT, author, resolve, save_json, sha

def snow_texture(scene,half):
    # Neutral procedural snow albedo: local centimetric variation, not a stretched city
    # photograph. The game supplies lighting, weather, tonemapping and pixel treatment.
    n=512
    rng=np.random.default_rng(194211)
    grain=rng.normal(0,1.4,(n,n,1))
    rgb=np.clip(np.array([229,232,235])[None,None,:]+grain,0,255).astype(np.uint8)
    # Below-grade cut faces are soil; actual source floor slabs cover the cellar floor.
    h=np.asarray(scene['terrain']['heights_m']).reshape(scene['terrain']['res'],scene['terrain']['res'])
    axis=np.linspace(-half,half,n); xx,zz=np.meshgrid(axis,axis)
    shalf=(len(h)-1)*scene['terrain']['spacing_m']/2
    ix=np.clip(((xx+shalf)/scene['terrain']['spacing_m']).round().astype(int),0,len(h)-1)
    iz=np.clip(((zz+shalf)/scene['terrain']['spacing_m']).round().astype(int),0,len(h)-1)
    soil=np.clip((-h[iz,ix]-.18)/.65,0,.88)
    rgb=(rgb*(1-soil[...,None])+np.array([81,75,63])*soil[...,None]).astype(np.uint8)
    digest=hashlib.sha256(rgb.tobytes()).hexdigest()
    relative=f'assets/authoring/historical_20260925/snow_ground_{digest[:12]}.png'
    path=ASSET_ROOT/relative
    path.parent.mkdir(parents=True,exist_ok=True)
    if not path.exists():author.write_png(path,rgb)
    save_json(path.with_suffix('.source.json'),{'operation':'Deterministic neutral snow and exposed cut-face soil albedo, no baked illumination',
        'resolution':[n,n],'seed':194211,'world_half_extent_m':half,'historical_status':'authored winter atmosphere, not recorded event weather'})
    return relative

def cook(slug):
    path=BLOCK_ROOT/f'{slug}.scene-block.v1.json'
    source=path.relative_to(ROOT).as_posix()
    scene=author.validate_scene(json.loads(path.read_text()))
    terrain=scene['terrain']; half=(terrain['res']-1)*terrain['spacing_m']/2+terrain['apron_m']+20
    spacing=terrain['spacing_m']; res=2*math.ceil(half/spacing)+1
    heights=np.zeros((res,res),np.float32)
    world={'schema':'kessel.act','version':1,'act_id':1,'act':scene['name'],
        'seed':int(author.digest(scene)[:8],16),
        'terrain':{'res':res,'spacing':spacing,'heights':'heights.npy','texture':snow_texture(scene,half)},
        'assets':[],'placements':[],'placement_ids':[],'markers':[],
        'authoring':{'mode':'scene','campaign_canonical':False,'source':source,
            'compatibility_tags':scene['metadata']['provenance']['placement_constraints'],
            'historical_claim':'Independent material reconstruction; inferred layout, not a surveyed place or a linked Tiger itinerary',
            'historical_scene':scene['metadata']}}
    baseline=json.loads((ROOT/'assets/worldgen/workshops/act1-winter-courtyard-20260925/manifest_edits.json').read_text())
    edits={'ops':[],'display_tuning':copy.deepcopy(baseline['display_tuning']),
           'environment_lighting':copy.deepcopy(baseline['environment_lighting'])}
    edits['environment_lighting'].update(camera_exposure=1.25,fog_density=.0018,overcast=.96,storm=.48)
    author.add_scene(world,heights,edits,scene,source,[0,0,0],0,slug,diagnostic=True)
    sockets={x['id']:x for x in scene['sockets']}
    approach=scene['metadata']['review_views'][0]
    entry=sockets.get('first_person_entry',{'position_m':approach['eye_m']})['position_m']
    target=sockets.get('first_person_look_target',{'position_m':approach['target_m']})['position_m']
    world['authoring']['first_person']={'eye_m':entry,'target_m':target,'source':'author-authored standing camera socket; runtime check required'}
    # The game's default Tiger is a real rendered/collidable object. Park it outside
    # the pedestrian scene instead of enclosing the first-person review camera in it.
    parked=[half-12,-half+12]
    world['authoring']['parked_tank_xz']=parked
    world['markers']=[{'name':'Workshop vehicle staging','kind':'Spawn','x':parked[0],'y':author.sample_height(heights,spacing,*parked),'z':parked[1]},
        {'name':'Workshop exit','kind':'Goal','x':half-8,'y':0,'z':half-8},
        {'name':scene['name'],'kind':'Monument','x':0,'y':0,'z':0}]
    result=author.publish(f'historical-{slug}-20260925',world,heights,edits,
        source={'scene':source,'sha256':sha(path)},recipe={'mode':'independent_historical_workshop','campaign_modified':False})
    result.update(slug=slug,scene=source,views=scene['metadata']['review_views'])
    save_json(RECEIPT_ROOT/f'{slug}.workshop.json',result)
    return result

def main():
    parser=argparse.ArgumentParser(); parser.add_argument('scenes',nargs='*',default=['household','ward','gumrak'])
    args=parser.parse_args(); entries=[]
    for slug in args.scenes:
        entry=cook(slug);entries.append(entry)
        print(json.dumps({'slug':slug,'manifest':entry['manifest'],'placements':entry['placements']}))
    index=RECEIPT_ROOT/'workshops.json'
    previous=json.loads(index.read_text()) if index.exists() else {'schema':'panzer.historical-workshop-index.v1','scenes':[]}
    updated={x['slug']:x for x in previous['scenes']};updated.update({x['slug']:x for x in entries})
    previous['scenes']=list(updated.values());save_json(index,previous)

if __name__=='__main__':main()
