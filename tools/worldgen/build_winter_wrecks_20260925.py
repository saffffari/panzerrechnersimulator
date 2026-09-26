#!/usr/bin/env python3
"""Private source-preserving wreck variants and a bounded additive winter workshop edit.

Run from shader-graphics: python tools/worldgen/build_winter_wrecks_20260925.py [--near-courtyard]
Existing hull transforms, authoring operations, geometry bytes and original assets are preserved.
"""
from __future__ import annotations
import argparse, copy, hashlib, json, math, struct
from pathlib import Path
import numpy as np
from build_winter_approach_revision import glb_geometry

ROOT = Path(__file__).resolve().parents[2]
SOURCE = Path('/home/alex/panzer/tank_simulator')
MAP = ROOT/'assets/worldgen/workshops/act1-winter-courtyard-20260925'
PREFIX = 'assets/authoring/battlefield/tiger_wreck_20260925_'
DEST = SOURCE/'assets/authoring/battlefield'
TAG = 'winter_wreck_aftermath_20260925'

def sha(data): return hashlib.sha256(data).hexdigest()
def write_new(path, data):
    if path.exists():
        if path.read_bytes() != data: raise RuntimeError(f'Preserving existing different output: {path}')
    else: path.write_bytes(data)
def encode(value): return (json.dumps(value, indent=2)+'\n').encode()
def rotation(axis, degrees):
    a=math.radians(degrees);c,s=math.cos(a),math.sin(a);m=np.eye(4)
    if axis=='x':m[1:3,1:3]=[[c,-s],[s,c]]
    elif axis=='y':m[np.ix_([0,2],[0,2])]=[[c,s],[-s,c]]
    else:m[:2,:2]=[[c,-s],[s,c]]
    return m
def translation(p):
    m=np.eye(4);m[:3,3]=p;return m

def derive(source, name, matrix=None, crew=False):
    raw=source.read_bytes();length,kind=struct.unpack_from('<II',raw,12)
    assert raw[:4]==b'glTF' and kind==0x4e4f534a
    data=json.loads(raw[20:20+length]);binary=raw[20+length:]
    triangles=sum(data['accessors'][p['indices']]['count']//3
        for m in data['meshes'] for p in m['primitives'])
    for material in data.get('materials',[]):
        pbr=material.setdefault('pbrMetallicRoughness',{})
        if crew:
            color=pbr.get('baseColorFactor',[1,1,1,1]);color=[v*.38 for v in color[:3]]+[color[3]]
        else:
            label=material.get('name','')
            color=([.046,.032,.024,1] if label=='Material.001' else
                [.018,.017,.016,1] if label=='Interior Paint' else [.010,.009,.008,1])
        pbr.update(baseColorFactor=color, metallicFactor=.18 if not crew else 0, roughnessFactor=.96)
        material['emissiveFactor']=[0,0,0]
        material.get('extensions',{}).pop('KHR_materials_clearcoat',None)
        material.get('extensions',{}).pop('KHR_materials_emissive_strength',None)
    if matrix is not None:
        scene=data['scenes'][data.get('scene',0)]
        node={'name':'Authored wreck resting pose','matrix':matrix.T.flatten().tolist(),'children':scene['nodes']}
        scene['nodes']=[len(data['nodes'])];data['nodes'].append(node)
    text=json.dumps(data,separators=(',',':')).encode();text+=b' '*(-len(text)%4)
    result=struct.pack('<4sII',b'glTF',2,20+len(text)+len(binary))+struct.pack('<II',len(text),kind)+text+binary
    output=SOURCE/(PREFIX+name+'.glb');write_new(output,result)
    assert result[20+len(text):]==binary
    return {'asset':PREFIX+name+'.glb','source':str(source.relative_to(SOURCE)),
        'source_sha256':sha(raw),'sha256':sha(result),'bytes':len(result),'triangles':triangles,
        'binary_chunks_preserved_sha256':sha(binary),'root_pose_matrix':None if matrix is None else matrix.tolist(),
        'operation':'Soot/rust material factors and optional root resting transform only; original indices, vertices, normals, UVs and images unchanged.',
        'provenance':'AdrielCZ Free Tiger And Crew Kit; CC-BY-4.0; see crew attribution below.' if crew else 'Existing owner-private Tiger rig; source rights unchanged.'}

def main(near_courtyard=False):
    manifest_path=MAP/'manifest.v1.json';edits_path=MAP/'manifest_edits.json'
    original_manifest=manifest_path.read_bytes();original_edits=edits_path.read_bytes()
    manifest=json.loads(original_manifest);edits=json.loads(original_edits)
    if TAG in edits:
        print(json.dumps({'status':'already applied; preserving current user edits','receipt':edits[TAG]}));return
    DEST.mkdir(parents=True,exist_ok=True)
    terrain=np.load(MAP/manifest['terrain']['heights']);spacing=manifest['terrain']['spacing'];half=(len(terrain)-1)*.5
    def ground(x,z):
        gx=x/spacing+half;gz=z/spacing+half;ix=math.floor(gx);iz=math.floor(gz);u=gx-ix;v=gz-iz
        return float((1-v)*((1-u)*terrain[iz,ix]+u*terrain[iz,ix+1])+v*((1-u)*terrain[iz+1,ix]+u*terrain[iz+1,ix+1]))
    receipts=[]
    for part in ('hull','turret','gun'):
        receipts.append(derive(SOURCE/f'assets/tanks/tiger/baked/tiger_{part}.glb',part,
            rotation('x',12) if part=='gun' else None))
    fall=rotation('z',26)@rotation('x',18)
    receipts.append(derive(SOURCE/'assets/tanks/tiger/baked/tiger_turret.glb','turret_detached',fall))
    receipts.append(derive(SOURCE/'assets/tanks/tiger/baked/tiger_gun.glb','gun_detached',
        fall@translation([.004,1.845,1.040])@rotation('x',12)))
    turret_tri,_=glb_geometry(SOURCE/(PREFIX+'turret_detached.glb'))
    turret_low=float(turret_tri[:,:,1].min())
    del turret_tri
    for face,tag,tilt in [('02','crew_a',24),('03','crew_b',-29)]:
        source=SOURCE/f'assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/baked/german_panzertruppe_face{face}_seated_static.glb'
        tri,_=glb_geometry(source);pose=rotation('z',tilt)@rotation('x',18)
        points=tri.reshape(-1,3)@pose[:3,:3].T;pose[1,3]=-float(points[:,1].min())
        receipts.append(derive(source,tag,pose,crew=True))
    assets=manifest['assets'];placements=manifest['placements'];hulls=[]
    for index,path in enumerate(assets):
        for part in ('hull','turret','gun'):
            if path==f'assets/tanks/tiger/baked/half/tiger_{part}.glb':
                assets[index]=PREFIX+part+'.glb'
                if part=='hull':hulls=[i for i,p in enumerate(placements) if p[0]==index]
    def asset_index(path):
        if path not in assets:assets.append(path)
        return assets.index(path)
    old_ops=copy.deepcopy(edits['ops']);new_ops=[];identities=[]
    def add(asset,p,yaw,label):
        op={'op':'add','asset':PREFIX+asset+'.glb','x':float(p[0]),'y':float(p[1]),'z':float(p[2]),
            'deg':float(yaw),'s':[1,1,1]}
        effective=len(placements)+sum(o.get('op')=='add' for o in old_ops)+len(new_ops)
        new_ops.append(op);return {'stable_tag':label,'effective_source_index':effective,'asset':op['asset'],'position':list(map(float,p)),'yaw':float(yaw)}
    def dress_open_hull(position,yaw,label):
        rot=rotation('y',yaw)
        for n,(name,local) in enumerate([('crew_a',[-.46,.49,-.24]),('crew_b',[.49,.49,.32])]):
            p=np.asarray(position)+rot[:3,:3]@np.asarray(local)
            add(name,p,yaw+(140 if n else -25),label+f'_crew_{n}')
    for ordinal,index in enumerate(hulls):
        hull=placements[index];identities.append({'stable_tag':f'base_tiger_{index}','base_source_index':index,'hull_placement':hull.copy(),'turret_detached':ordinal in (0,2)})
        if ordinal not in (0,2):continue
        assert assets[placements[index+1][0]]==PREFIX+'turret.glb'
        assert assets[placements[index+2][0]]==PREFIX+'gun.glb'
        local=rotation('y',hull[4])[:3,:3]@np.array([-5.4,0,2.6]);x,z=hull[1]+local[0],hull[3]+local[2]
        y=ground(x,z)-turret_low+.025;yaw=hull[4]+43
        for offset,part in [(1,'turret_detached'),(2,'gun_detached')]:
            placements[index+offset]=[asset_index(PREFIX+part+'.glb'),float(x),float(y),float(z),yaw,1.0]
        dress_open_hull([hull[1],hull[2],hull[3]],hull[4],f'base_tiger_{index}')
    if near_courtyard:
        pos=[1907.,ground(1907,218),218.];yaw=32.
        identities.append(add('hull',pos,yaw,'winter_wreck_20260925_near_courtyard'))
        rot=rotation('y',yaw);p=np.asarray(pos)+rot[:3,:3]@np.array([-5.4,0,2.6]);p[1]=ground(p[0],p[2])-turret_low+.025
        add('turret_detached',p,yaw+43,'winter_wreck_20260925_near_courtyard_turret')
        add('gun_detached',p,yaw+43,'winter_wreck_20260925_near_courtyard_gun')
        dress_open_hull(pos,yaw,'winter_wreck_20260925_near_courtyard')
    else:
        h=placements[hulls[0]];pos=[h[1],h[2],h[3]];yaw=h[4]
    def view(local_eye,local_target):
        r=rotation('y',yaw)[:3,:3];eye=np.asarray(pos)+r@np.asarray(local_eye);target=np.asarray(pos)+r@np.asarray(local_target);d=target-eye
        return {'eye':eye.tolist(),'target':target.tolist(),'yaw_deg':math.degrees(math.atan2(d[0],-d[2])),
            'pitch_deg':math.degrees(math.asin(d[1]/np.linalg.norm(d)))}
    edits['ops'].extend(new_ops)
    receipt={'tag':TAG,'source_manifest_sha256':sha(original_manifest),'source_edits_sha256':sha(original_edits),
        'preserved_existing_ops':len(old_ops),'added_ops':len(new_ops),'hulls':identities,'derived_assets':receipts,
        'crew_attribution':{'creator':'AdrielCZ','title':'Free Tiger And Crew Kit','license':'CC-BY-4.0',
            'source':'https://sketchfab.com/3d-models/free-tiger-and-crew-kit-6290669ab2cb49599f1606507626af39',
            'license_url':'https://creativecommons.org/licenses/by/4.0/','changes':'Static seated source pose rigidly slumped and soot-tinted; geometry and source texture preserved.'},
        'review_views':{'exterior':view([-9,4,8],[0,1,0]),'interior':view([0,3.1,.25],[.20,.55,.1])},
        'scope':'Authored static aftermath; not a probabilistic battle casualty model. Current player and claimable spare meshes unchanged.'}
    edits[TAG]={'receipt':'tiger_wreck_20260925_receipt.json','hulls':identities}
    assert edits['ops'][:len(old_ops)]==old_ops
    assert all(manifest['placements'][i]==json.loads(original_manifest)['placements'][i] for i in hulls)
    # Preserve exact before bytes for a reviewable local handoff; never replace a previous backup.
    backup=ROOT/'artifacts/authoring/winter-wreck-20260925';backup.mkdir(parents=True,exist_ok=True)
    write_new(backup/'manifest.before.json',original_manifest);write_new(backup/'manifest_edits.before.json',original_edits)
    manifest_path.write_bytes(encode(manifest));edits_path.write_bytes(encode(edits))
    receipt['manifest_sha256']=sha(manifest_path.read_bytes());receipt['edits_sha256']=sha(edits_path.read_bytes())
    write_new(DEST/'tiger_wreck_20260925_receipt.json',encode(receipt))
    (backup/'receipt.json').write_bytes(encode(receipt))
    print(json.dumps({'assets':len(receipts),'hulls':identities,'review_views':receipt['review_views']}))

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--near-courtyard',action='store_true');args=parser.parse_args();main(args.near_courtyard)
