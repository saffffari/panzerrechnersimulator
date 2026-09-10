#!/usr/bin/env python3
"""CPU source/provenance and ordinary-input receipts for historical scene studies.

Run with the bundled Python (Pillow only makes a labelled geometric plan, never a game image).
The frozen binary's mechanic-validate command returns before renderer/audio allocation.
"""
import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys
import numpy as np
from PIL import Image,ImageDraw,ImageFont

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tools/worldgen'))
import world_authoring as author
PACKAGE=ROOT/'artifacts/astra-historical-scene-studies-20260905-v4'

def save(name,data):(PACKAGE/name).write_text(json.dumps(data,indent=2,allow_nan=False)+'\n')
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()

def input_trace(spec):
    yaw=0.;x,z=spec['setup']['foot_xz'];frames=[]
    target=spec['setup']['look_at_xz'];yaw=math.atan2(target[0]-x,-(target[1]-z))
    for tick in range(round(spec['seconds']*120)):
        t=tick/120;inp={}
        for event in spec['timeline']:
            if event.get('from_s',1e9)<=t<event.get('until_s',-1):inp.update(event['input'])
        yaw+=inp.get('look_dx',0)*.0022
        if inp.get('drive',0):
            # Existing Foot::update normalizes the requested vector. This is an unobstructed
            # kinematic expectation, not a replacement collision simulation or fabricated state.
            speed=1.3 if inp.get('crouch') else 5.5
            sign=math.copysign(1,inp['drive']);x+=math.sin(yaw)*speed/120*sign;z-=math.cos(yaw)*speed/120*sign
        if tick%120==0:frames.append({'time_s':int(t),'expected_x_m':x,'expected_z_m':z,'expected_yaw_deg':math.degrees(yaw)})
    return {'method':'120 Hz ordinary Foot::update look/normalized movement constants; no collision, AI, rendered-frame or outcome claim',
      'source':'src/foot.rs SENS=.0022, RUN=5.5; grounded slope and actual collision remain authoritative in game','seconds':frames}

def main():
    queue=json.loads((PACKAGE/'queue.json').read_text());exe=ROOT/'artifacts/astra-builds/20260905_v16/kessel'
    gate=[];foundation=[];closure=[];traces={};plan=Image.new('RGB',(1536,626),(22,27,31));draw=ImageDraw.Draw(plan)
    font_path='/usr/share/fonts/TTF/DejaVuSans.ttf'
    if not Path(font_path).exists():font_path='/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf'
    font=ImageFont.truetype(font_path,17) if Path(font_path).exists() else ImageFont.load_default()
    small=ImageFont.truetype(font_path,13) if Path(font_path).exists() else ImageFont.load_default()
    draw.text((20,12),'CPU SOURCE SCENE PLANS — NOT GAME CAPTURES',fill=(239,238,222),font=font)
    for column,e in enumerate(queue['entries']):
        spec=json.loads((ROOT/e['review_spec']).read_text());block=json.loads((ROOT/e['scene']).read_text());traces[e['catalog_id']]=input_trace(spec)
        res=subprocess.run([str(exe),'mechanic-validate',e['review_spec']],cwd=ROOT,text=True,capture_output=True)
        gate.append({'spec':e['review_spec'],'exit_code':res.returncode,'stdout':res.stdout,'stderr':res.stderr})
        assert res.returncode==0,res.stderr
        h=np.array(block['terrain']['heights_m']).reshape(block['terrain']['res'],-1);spacing=block['terrain']['spacing_m'];half=(len(h)-1)*spacing/2
        for b in block['metadata']['buildings']:
            x,y,z=b['entrance_m'];sample=author.sample_height(h,spacing,x,z)
            foundation.append({'catalog_id':e['catalog_id'],'entrance_xz':[x,z],'floor_y_m':y,'terrain_y_m':sample,'difference_m':sample-y})
            assert abs((y-sample)-.1)<.002
        provenance=json.loads((PACKAGE/(str(['winter-firewood','clearing-forest','blue-day-garden'][column])+'-asset-provenance.json')).read_text())
        for a in provenance:
            actual=sha(ROOT/a['asset']);assert actual==a['sha256']
            closure.append({'catalog_id':e['catalog_id'],'asset':a['asset'],'sha256':actual,'catalog_id_asset':a['catalog_id']})
        # A plan of actual source-bounds footprints and terrain is useful authoring evidence,
        # but is deliberately not shaded or labelled as a first-person renderer result.
        x0=column*512+18;y0=83;size=474
        field=np.clip((h+2)/4,0,1);rgb=np.stack([55+field*70,65+field*68,66+field*62],axis=-1).astype('uint8')
        if column==0:rgb=np.stack([125+field*85,137+field*76,140+field*79],axis=-1).clip(0,255).astype('uint8')
        im=Image.fromarray(rgb[::-1]).resize((size,size),Image.Resampling.NEAREST);plan.paste(im,(x0,y0));draw=ImageDraw.Draw(plan)
        def xy(x,z):return(x0+(x+half)/(2*half)*size,y0+(half-z)/(2*half)*size)
        records={a['asset']:a for a in provenance}
        for item in block['items']:
            name=Path(item['asset']).stem;b=records[item['asset']]['bounds_m'];a=math.radians(item['deg']);points=[]
            for xx,zz in [(b['min'][0],b['min'][2]),(b['min'][0],b['max'][2]),(b['max'][0],b['max'][2]),(b['max'][0],b['min'][2])]:
                xx*=item['s'][0];zz*=item['s'][2];points.append(xy(item['dx']+xx*math.cos(a)+zz*math.sin(a),item['dz']-xx*math.sin(a)+zz*math.cos(a)))
            nature=any(n in name for n in ['tree','pine','stump','stone','log']);color=(33,73,53) if nature else (125,80,49)
            if any(n in name for n in ['floor','wall','roof','doorway','frame']):color=(40,43,47)
            draw.polygon(points,outline=color)
        for road in block['metadata']['roads']:
            a,b,w=road;draw.line([xy(*a),xy(*b)],fill=(172,151,117),width=max(1,round(w*2)))
        draw.line([xy(0,24),xy(0,-3.225)],fill=(61,239,235),width=4)
        draw.ellipse([xy(-1,25),xy(1,23)],fill=(61,239,235))
        draw.text((x0,y0+size+8),'Cyan: ordinary camera route; outlines: source bounds',fill=(203,214,216),font=small)
        draw.text((x0,y0+size+27),f'{e["placements"]} placements | {e["unique_assets"]} source assets | 144 m core',fill=(203,214,216),font=small)
        title=['WINTER / STRIPPED RESIDENCE','CLEARING / FOREST STAGING','BLUE DAY / DAMAGED GARDEN'][column]
        draw.text((x0,52),title,fill=(241,231,205),font=font)
    plan.save(PACKAGE/'source-layout-plans.png')
    save('frozen-v16-cpu-preflight.json',{'binary':str(exe),'binary_sha256':sha(exe),'gpu_created':False,'results':gate})
    save('ordinary-input-expectations.json',traces)
    save('foundation-source-audit.json',{'entrances':foundation,'passes':len(foundation),'runtime_walk_claim':False})
    save('asset-closure-recheck.json',{'assets_by_scene':closure,'unique_source_files':len({x['asset'] for x in closure}),'all_sha256_match':True})
    winter=json.loads((ROOT/queue['entries'][0]['scene']).read_text());h=np.array(winter['terrain']['heights_m']).reshape(193,193)
    route=[[-24,float(z),author.sample_height(h,.75,-24,float(z))] for z in np.linspace(22,-3,101)]
    slope=max(abs(b[2]-a[2])/abs(b[1]-a[1]) for a,b in zip(route,route[1:]))
    save('lower-shelter-grade-audit.json',{'path_x_z_ground_y_m':route,'maximum_sampled_slope_m_per_m':slope,
      'lower_room_floor_m':-1.6,'floor_above_grade_m':.1,'grade_has_no_abrupt_foundation_drop':slope<.25,'runtime_walk_claim':False})
    assert slope<.25
    preserved=json.loads((PACKAGE/'original-draft-preservation.json').read_text())['original_sha256'];assert all(sha(ROOT/p)==s for p,s in preserved.items())
    print(json.dumps({'mechanic_cpu_preflights':len(gate),'source_foundations':len(foundation),'original_drafts_preserved':len(preserved),
      'unique_assets':len({x['asset'] for x in closure}),'max_shelter_grade':slope,'gpu_or_cargo_run':False}))

if __name__=='__main__':main()
