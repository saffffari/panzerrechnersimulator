#!/usr/bin/env python3
"""Shared source-bound authoring for the September 25 historical environment batch.

Scene layouts are interpretations, not surveys. This helper writes new scene blocks and
receipts, and never modifies a campaign or an existing source model.
"""
from __future__ import annotations
import hashlib
import json
import math
from pathlib import Path
import struct
import numpy as np
import world_authoring as author

ROOT = Path(__file__).resolve().parents[2]
ASSET_ROOT = Path('/home/alex/panzer/tank_simulator')
BLOCK_ROOT = ROOT / 'assets/worldgen/scene_blocks/historical_20260925'
RECEIPT_ROOT = ROOT / 'artifacts/authoring/historical-20260925'
CATALOG = ROOT / 'assets/worldgen/scene_blocks/historical_scene_catalog.v1.json'

def sha(path):
    with Path(path).open('rb') as f: return hashlib.file_digest(f, 'sha256').hexdigest()

def save_json(path, value):
    path = Path(path); path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False)+'\n')

def resolve(raw, *, exists=True):
    p = Path(raw)
    if p.is_absolute() or '\\' in str(raw) or any(x in ('.','..') for x in str(raw).split('/')):
        raise ValueError(f'Expected safe repository relative path: {raw}')
    target = ROOT / p
    if not exists: return target
    if target.is_file(): return target
    if str(raw).startswith('assets/') and (ASSET_ROOT / p).is_file(): return ASSET_ROOT / p
    raise FileNotFoundError(raw)

# The existing authoring validator remains authoritative. Assets are read from the
# game's canonical library; generated scene/world data stays in this shader worktree.
author.repo_path = resolve

def node_matrix(node):
    if 'matrix' in node: return np.asarray(node['matrix'], dtype=float).reshape(4,4).T
    x,y,z,w = node.get('rotation',[0,0,0,1])
    r = np.array([[1-2*y*y-2*z*z,2*x*y-2*z*w,2*x*z+2*y*w],
                  [2*x*y+2*z*w,1-2*x*x-2*z*z,2*y*z-2*x*w],
                  [2*x*z-2*y*w,2*y*z+2*x*w,1-2*x*x-2*y*y]])
    m = np.eye(4); m[:3,:3] = r @ np.diag(node.get('scale',[1,1,1]))
    m[:3,3] = node.get('translation',[0,0,0]); return m

class AssetBank:
    def __init__(self):
        self.paths = sorted((ASSET_ROOT/'assets/pizza/_extracted').rglob('*.glb'))
        self.records = {}

    def asset(self, key):
        if str(key).startswith('assets/'): raw = str(key)
        else:
            found = [p for p in self.paths if p.stem == key]
            found.sort(key=lambda p: ('PSX Mega Pack II' not in str(p), str(p)))
            if not found: raise ValueError(f'Missing owned asset {key}')
            raw = found[0].relative_to(ASSET_ROOT).as_posix()
        self.inspect(raw)
        return raw

    def inspect(self, raw):
        if raw in self.records: return self.records[raw]
        path = resolve(raw); data = path.read_bytes()
        assert data[:4] == b'glTF'
        size, kind = struct.unpack_from('<II',data,12); assert kind == 0x4e4f534a
        doc = json.loads(data[20:20+size]); start = 28+size
        points = []; triangles = 0
        def positions(index):
            a = doc['accessors'][index]; view = doc['bufferViews'][a['bufferView']]
            assert a['componentType'] == 5126 and a['type'] == 'VEC3'
            return np.ndarray((a['count'],3),dtype='<f4',buffer=data,
                offset=start+view.get('byteOffset',0)+a.get('byteOffset',0),
                strides=(view.get('byteStride',12),4))
        def visit(index, parent):
            nonlocal triangles
            node=doc['nodes'][index]; model=parent @ node_matrix(node)
            if 'mesh' in node:
                for part in doc['meshes'][node['mesh']]['primitives']:
                    if part.get('mode',4) != 4: continue
                    v=positions(part['attributes']['POSITION'])
                    world=v @ model[:3,:3].T + model[:3,3]
                    points.extend([world.min(axis=0),world.max(axis=0)])
                    triangles += (doc['accessors'][part['indices']]['count'] if 'indices' in part else len(v))//3
            for child in node.get('children',[]): visit(child,model)
        for index in doc['scenes'][doc.get('scene',0)]['nodes']: visit(index,np.eye(4))
        assert points, raw
        lo=np.min(points,axis=0); hi=np.max(points,axis=0)
        result={'asset':raw,'sha256':sha(path),'bytes':len(data),'triangles':triangles,
            'bounds_m':{'min':lo.tolist(),'max':hi.tolist()},
            'bounds_method':'Actual POSITION buffers transformed through the active GLB scene hierarchy',
            'historical_status':'Existing game asset; shape/material is a reconstruction choice, not historical evidence'}
        self.records[raw]=result; return result

    def bounds(self,key):
        b=self.inspect(self.asset(key))['bounds_m']
        return np.asarray(b['min']),np.asarray(b['max'])

    def variant(self,key,label,color_factor):
        """Immutable material-only soot variant, with unchanged geometry and image chunks."""
        raw=self.asset(key); source=resolve(raw); data=source.read_bytes()
        size,kind=struct.unpack_from('<II',data,12); doc=json.loads(data[20:20+size])
        for mat in doc.get('materials',[]):
            pbr=mat.setdefault('pbrMetallicRoughness',{})
            old=pbr.get('baseColorFactor',[1,1,1,1])
            pbr['baseColorFactor']=[old[i]*color_factor[i] for i in range(3)]+[old[3]]
            pbr['roughnessFactor']=max(.86,pbr.get('roughnessFactor',1.0))
        encoded=json.dumps(doc,separators=(',',':')).encode(); encoded+=b' '*((-len(encoded))%4)
        tail=data[20+size:]
        output=struct.pack('<III',0x46546c67,2,20+len(encoded)+len(tail))+struct.pack('<II',len(encoded),kind)+encoded+tail
        digest=hashlib.sha256(output).hexdigest()
        target=ASSET_ROOT/f'assets/authoring/historical_20260925/{label}_{digest[:12]}.glb'
        target.parent.mkdir(parents=True,exist_ok=True)
        if not target.exists(): target.write_bytes(output)
        assert target.read_bytes()==output
        save_json(target.with_suffix('.source.json'),{'source':raw,'source_sha256':sha(source),
            'operation':'material color-factor and roughness change only','color_factor':color_factor,
            'binary_chunks_unchanged':True,'source_tail_sha256':hashlib.sha256(tail).hexdigest()})
        return self.asset(target.relative_to(ASSET_ROOT).as_posix())

class Scene:
    def __init__(self,catalog_id,slug,*,half=48.0,spacing=.5,bank=None):
        self.bank=bank or AssetBank(); self.slug=slug
        self.source=next(s for s in json.loads(CATALOG.read_text())['scenes'] if s['id']==catalog_id)
        self.res=2*round(half/spacing)+1; self.spacing=spacing; self.half=(self.res-1)*spacing/2
        axis=np.linspace(-self.half,self.half,self.res)
        self.x,self.z=np.meshgrid(axis,axis); self.heights=np.zeros((self.res,self.res),dtype=np.float32)
        self.items=[]; self.evidence=[]; self.views=[]; self.sockets=[]; self.features=[]
        self.notes=[]; self.lights=[]; self.connectors=[]

    def set_terrain(self,fn):
        values=fn(self.x,self.z) if callable(fn) else fn
        self.heights=np.broadcast_to(values,self.heights.shape).astype(np.float32).copy()

    def ground(self,x,z): return float(author.sample_height(self.heights,self.spacing,x,z))

    def put(self,key,x,z,y=None,*,yaw=0,scale=1,role='Inferred physical detail',evidence='reconstruction_choice'):
        raw=self.bank.asset(key); lo,hi=self.bank.bounds(raw)
        s=[float(scale)]*3 if np.isscalar(scale) else [float(v) for v in scale]
        if y is None: y=self.ground(x,z)-lo[1]*s[1]
        ident=f'{self.slug}_{len(self.items):04d}'
        self.items.append({'id':ident,'asset':raw,'dx':float(x),'dy':float(y),'dz':float(z),
                           'deg':float(yaw),'s':s})
        self.evidence.append({'item_id':ident,'role':role,'evidence_class':evidence})
        return ident

    def socket(self,ident,kind,position,heading=0):
        self.sockets.append({'id':ident,'kind':kind,'position_m':list(position),'heading_deg':heading})

    def view(self,ident,eye,target):
        delta=np.asarray(target)-eye
        self.views.append({'id':ident,'eye_m':list(eye),'target_m':list(target),
            'yaw_deg':math.degrees(math.atan2(delta[0],-delta[2])),
            'pitch_deg':math.degrees(math.atan2(delta[1],math.hypot(delta[0],delta[2])))})

    def block(self):
        src=self.source
        return {'schema':'kessel.scene-block','version':1,'name':src['title']+' — material reconstruction',
            'anchor':{'origin_m':[0,0,0],'forward':'+z','grounding':'rigid_anchor'},
            'terrain':{'datum':'anchor_ground','res':self.res,'spacing_m':self.spacing,'apron_m':8,
                       'heights_m':self.heights.astype(float).ravel().tolist()},
            'items':self.items,'sockets':self.sockets,'connectors':self.connectors,
            'environment':{'sound_emitters':[],'authored_lights':self.lights},
            'metadata':{'status':'assembled_pending_visual_review','catalog_id':src['id'],
                'provenance':{'placement_class':src['provenance_class'],
                    'source_ids':[s['id'] for s in src['source_records']],
                    'source_urls':list(dict.fromkeys(s['url'] for s in src['source_records'])),
                    'placement_constraints':list(dict.fromkeys([src['historical_region'],*src['placement_constraints']['requires']]))},
                'source_records':src['source_records'],'source_caveats':src['caveats'],
                'historical_integrity_boundary':'Sources support the event/material conditions. Local layout, dimensions, prop selection, positions and counts are design inferences. This is not a surveyed site, a continuous Tiger route, or proof of adjacency to another scene.',
                'item_evidence':self.evidence,'source_visual_beats':src['visual_beats'],
                'review_views':self.views,'features':self.features,'production_notes':self.notes}}

    def save(self):
        block=self.block(); author.validate_scene(block)
        path=BLOCK_ROOT/f'{self.slug}.scene-block.v1.json'
        save_json(path,block)
        receipt={'scene':path.relative_to(ROOT).as_posix(),'sha256':sha(path),
            'items':len(self.items),'unique_assets':len({i['asset'] for i in self.items}),
            'asset_records':list(self.bank.records.values()),'validation':'Source asset closure, actual GLB bounds, scene schema; not GPU/traversal acceptance',
            'review_views':self.views}
        save_json(RECEIPT_ROOT/f'{self.slug}.receipt.json',receipt)
        print(json.dumps({k:receipt[k] for k in ['scene','items','unique_assets']}))
        return path
