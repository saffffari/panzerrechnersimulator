#!/usr/bin/env python3
"""Three local historical-condition studies; immutable editor API outputs, never act replacement.

No renderer, game process, soundtrack, campaign-state or source-draft mutation occurs here.
The geometry/layout/weather choices are explicitly inferred. Source SHA and actual node bounds
are recorded; the ordinary-control review remains pending until the game records and is inspected.
"""
from __future__ import annotations
import copy
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import sys
import numpy as np

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tools/worldgen'))
import world_authoring as author

VERSION=4
PACKAGE=ROOT/f'artifacts/astra-historical-scene-studies-20260905-v{VERSION}'
BLOCKS=ROOT/f'assets/worldgen/scene_blocks/astra_historical_studies_20260905_v{VERSION}'
CATALOG=ROOT/'assets/worldgen/scene_blocks/historical_scene_catalog.v1.json'
SELECTED=['a1_apartment_firewood','a2_kurmark_forest_replenishment','a3_immediate_graves']
SHORT=['winter-firewood','clearing-forest','blue-day-garden']
N=193;SPACING=0.75;HALF=(N-1)*SPACING/2
AXIS=np.linspace(-HALF,HALF,N);X=AXIS[None,:];Z=AXIS[:,None]

def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def save(p,v):
    p.parent.mkdir(parents=True,exist_ok=True)
    if p.exists():raise FileExistsError(f'preserve existing artifact: {p}')
    p.write_text(json.dumps(v,indent=2,ensure_ascii=False,allow_nan=False)+'\n')
def relative(p):return p.relative_to(ROOT).as_posix()
def trs(n):
    if 'matrix' in n:return np.array(n['matrix'],float).reshape(4,4).T
    x,y,z,w=n.get('rotation',[0,0,0,1]);s=np.array(n.get('scale',[1,1,1]))
    r=np.array([[1-2*y*y-2*z*z,2*x*y-2*z*w,2*x*z+2*y*w],
      [2*x*y+2*z*w,1-2*x*x-2*z*z,2*y*z-2*x*w],
      [2*x*z-2*y*w,2*y*z+2*x*w,1-2*x*x-2*y*y]])
    m=np.eye(4);m[:3,:3]=r@np.diag(s);m[:3,3]=n.get('translation',[0,0,0]);return m

class Assets:
    def __init__(self):
        self.catalog={a['source']:a for a in json.loads((ROOT/'catalog/runtime_assets.v1.json').read_text())['assets']}
        self.gw=author.generator_module();self.paths=dict(self.gw.KIT);self.cache={}
        base=self.paths['wall'].rsplit('Modular Structures/',1)[0]
        for key,path in {
            'doorway':'Modular Structures/doorway_hr_2_wide.glb',
            'board':'Props/wooden_board_2.glb','plank':'Props/wooden_plank_1.glb',
            'beam':'Props/beam_1.glb','wood_barrel':'Props/wooden_barrel_1.glb',
            'bucket':'Props/bucket_mx_1.glb','mattress':'Props/old_mattress_mx_1.glb',
        }.items():self.paths[key]=base+path
        self.paths['stripped_frame']=self.empty_frame(self.paths['doorway'])
        for k in ['tree_b','tree_c']:
            self.paths[k+'_bare']=author.bare_tree(self.paths[k])
    def empty_frame(self,source):
        # The source already has no leaf. A new semantic identity prevents Frame::from_asset
        # installing fictional doors in a scene whose evidence is their removal for fuel.
        p=ROOT/source;target=ROOT/f'assets/worldgen/derived/{sha(p)[:16]}/stripped_apartment_open_frame.glb'
        target.parent.mkdir(parents=True,exist_ok=True)
        if not target.exists():
            target.write_bytes(p.read_bytes())
            save(target.with_suffix('.source.json'),{'source':source,'source_sha256':sha(p),
              'operation':'byte-identical source wide frame; distinct semantic name omits automatic live door leaves',
              'geometry_material_bytes_changed':False,'reason':'documented stripped doors; no replacement leaf inferred'})
        assert target.read_bytes()==p.read_bytes()
        return relative(target)
    def geometry(self,path):
        if path in self.cache:return self.cache[path]
        data=(ROOT/path).read_bytes();assert data[:4]==b'glTF'
        ln,kind=struct.unpack_from('<II',data,12);assert kind==0x4e4f534a
        d=json.loads(data[20:20+ln]);start=28+ln
        assert struct.unpack_from('<I',data,24+ln)[0]==0x004e4942
        def acc(i):
            a=d['accessors'][i];v=d['bufferViews'][a['bufferView']]
            dtype={5121:'u1',5123:'<u2',5125:'<u4',5126:'<f4'}[a['componentType']]
            width={'SCALAR':1,'VEC2':2,'VEC3':3,'VEC4':4}[a['type']];size=np.dtype(dtype).itemsize
            return np.ndarray((a['count'],width),dtype=dtype,buffer=data,
              offset=start+v.get('byteOffset',0)+a.get('byteOffset',0),strides=(v.get('byteStride',size*width),size)).copy()
        triangles=[];nodes=[]
        def visit(i,parent):
            n=d['nodes'][i];m=parent@trs(n)
            if 'mesh' in n:
                for p in d['meshes'][n['mesh']]['primitives']:
                    assert p.get('mode',4)==4
                    v=acc(p['attributes']['POSITION'])@m[:3,:3].T+m[:3,3]
                    ix=acc(p['indices']).ravel() if 'indices' in p else np.arange(len(v))
                    triangles.append(v[ix.reshape(-1,3)]);nodes.append(n.get('name',str(i)))
            for c in n.get('children',[]):visit(c,m)
        for i in d['scenes'][d.get('scene',0)]['nodes']:visit(i,np.eye(4))
        t=np.concatenate(triangles);lo=t.min((0,1));hi=t.max((0,1));entry=self.catalog.get(path)
        derived=(ROOT/path).with_suffix('.source.json')
        record={'asset':path,'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data),
          'catalog_id':entry['id'] if entry else None,'owned_source':'downloaded Pizza bundle',
          'bounds_m':{'min':lo.tolist(),'max':hi.tolist()},'bounds_method':'active GLB scene hierarchy and transformed POSITION/index buffers',
          'triangles':len(t),'source_nodes':list(dict.fromkeys(nodes)),
          'period_authenticity':'generic material/shape chosen for plausibility; asset is not period photographic evidence'}
        if derived.exists():record['derivation']=json.loads(derived.read_text())
        self.cache[path]=(t,record);return t,record
    def source(self,key):
        path=self.paths[key];self.geometry(path);return path

def smoothstep(a):a=np.clip(a,0,1);return a*a*(3-2*a)
def rect_distance(x,z,cx,cz,hx,hz):return np.hypot(np.maximum(abs(x-cx)-hx,0),np.maximum(abs(z-cz)-hz,0))
def segment_distance(x,z,a,b):
    dx=b[0]-a[0];dz=b[1]-a[1];t=np.clip(((x-a[0])*dx+(z-a[1])*dz)/max(.00001,dx*dx+dz*dz),0,1)
    return np.hypot(x-a[0]-t*dx,z-a[1]-t*dz)

class Study:
    def __init__(self,assets,source,act):
        self.a=assets;self.source=source;self.act=act;self.items=[];self.evidence=[];self.buildings=[];self.features=[];self.grounded={}
        self.rng=np.random.default_rng(20260905+act*2309)
        self.h=(.42*np.sin(X/25)+.27*np.cos(Z/34)+.18*np.sin((X+Z)/19))*smoothstep((np.maximum(abs(X),abs(Z))-15)/20)
        self.roads=[((0,66),(0,12),2.8),((-56,30),(55,30),3.5)]
        self.soil_masks=[]
    def ground(self,x,z):return author.sample_height(self.h,SPACING,x,z)
    def pad(self,x,z,hx,hz,height=0):
        w=1-smoothstep(rect_distance(X,Z,x,z,hx,hz)/5);self.h=self.h*(1-w)+height*w
    def depression(self,x,z,hx,hz,depth,angle=0,berm=.0):
        a=math.radians(angle);u=(X-x)*math.cos(a)+(Z-z)*math.sin(a);v=-(X-x)*math.sin(a)+(Z-z)*math.cos(a)
        r=((u/hx)**4+(v/hz)**4)**.25
        self.h-=depth*(1-smoothstep((r-.70)/.42))
        if berm:self.h+=berm*np.exp(-((r-1.12)/.19)**2)
        self.soil_masks.append((x,z,hx*1.35,hz*1.35,angle))
        self.features.append({'kind':'inferred_local_earthwork','center_m':[x,z],'half_extents_m':[hx,hz],'depth_m':depth,'yaw_deg':angle})
    def put(self,key,x,z,y=None,yaw=0,scale=1,role='inferred environmental detail'):
        path=self.a.source(key);s=[scale]*3 if isinstance(scale,(int,float)) else list(scale)
        attached=y is None
        if attached:y=self.ground(x,z)-self.a.geometry(path)[1]['bounds_m']['min'][1]*s[1]
        ident=f'{SHORT[self.act-1].replace("-","_")}_{len(self.items):04}'
        self.items.append(dict(id=ident,asset=path,dx=round(x,5),dy=round(y,5),dz=round(z,5),deg=round(yaw,5),s=s))
        if attached:self.grounded[ident]=self.ground(x,z)
        self.evidence.append({'item_id':ident,'role':role,'evidence_class':'authored_inference','source_shape':Path(path).stem})
    def room(self,cx,cz,nx,nz,*,main=False,damage=False,doorless=False,upper=False):
        hx=nx*2;hz=nz*2;base=self.ground(cx,cz)
        xs=[-hx+2+4*i for i in range(nx)];zs=[-hz+2+4*i for i in range(nz)]
        assert nx%2==1,'one centered source doorway'
        for x in xs:
            for z in zs:self.put('floor',cx+x,cz+z,base-.3,role='continuous source floor; 10 cm above grade avoids coincident terrain and remains a standing step')
        for x in xs:
            self.put('stripped_frame' if doorless else 'doorway',cx+x,cz+hz,base,role='empty frame, no invented replacement door' if doorless else 'live source-hinged door opening') if x==0 else self.put('wall',cx+x,cz+hz,base,role='structural front wall')
            self.put('wall_win' if x!=0 else 'wall',cx+x,cz-hz,base,role='source rear wall and window assembly')
        for z in zs:
            for sign in [-1,1]:
                self.put('wall_win' if (int((z+hz)/4)%2==0) else 'wall',cx+sign*hx,cz+z,base,yaw=90,role='source side wall; protected aisle remains open')
        for x in [-hx,hx]:
            for z in [-hz,hz]:self.put('pillar',cx+x,cz+z,base,role='source corner structure')
        # Source slabs preserve a closed ceiling over the traversed rooms. The damaged study
        # leaves rear ceiling cells absent; no invisible blocker seals those real openings.
        for x in xs:
            for z in zs:
                if damage and z < -hz+4 and x>0:continue
                self.put('floor',cx+x,cz+z,base+3,role='source ceiling slab; deliberate rear collapse opening' if damage else 'source ceiling slab')
        if upper:
            for x in xs:
                self.put('wall',cx+x,cz+hz,base+3.4,role='exposed second-storey front shell')
            for z in zs:
                self.put('wall',cx-hx,cz+z,base+3.4,yaw=90,role='surviving second-storey side shell')
        elif not damage:
            # One source roof module over each 4 m bay; this is generic modular architecture,
            # recorded as an inferred stores/house shell, not a surveyed roof reconstruction.
            for x in xs:
                for z in zs:self.put('roof_pitch',cx+x,cz+z,base+3.35,role='source weather roof module, inferred assembly')
        self.buildings.append({'center_m':[cx,base,cz],'half_size_m':[hx,hz],'entrance_m':[cx,base+.1,cz+hz],
          'floor_top_m':base+.1,'floor_above_grade_m':.1,'clear_aisle_half_width_m':1.5,'doorless':doorless,'main':main,
          'historical_status':'generic enterable modular inference; no exact building identity claimed'})
    def nature(self,count,winter=False):
        # Sample spatial clumps with open tracks; preserve a quiet readable route and varied skyline.
        centres=[(-42,-36),(39,-34),(-43,41),(43,44)]
        def disturbed(x,z,margin):
            for cx,cz,hx,hz,angle in self.soil_masks:
                a=math.radians(angle);u=(x-cx)*math.cos(a)+(z-cz)*math.sin(a);v=-(x-cx)*math.sin(a)+(z-cz)*math.cos(a)
                if (u/(hx+margin))**4+(v/(hz+margin))**4<1.1:return True
            return False
        for i in range(count):
            cx,cz=centres[i%len(centres)];x=cx+self.rng.normal(0,12);z=cz+self.rng.normal(0,13)
            if max(abs(x),abs(z))>65 or any(segment_distance(x,z,a,b)<w+3 for a,b,w in self.roads):continue
            if disturbed(x,z,1.5):continue
            if any(rect_distance(x,z,b['center_m'][0],b['center_m'][2],b['half_size_m'][0]+3,b['half_size_m'][1]+3)<.1 for b in self.buildings):continue
            key=(['pine','tree_b_bare','tree_c_bare'] if winter else ['pine','tree_b','tree_c','tree_d'])[i%(3 if winter else 4)]
            self.put(key,float(x),float(z),yaw=float(self.rng.uniform(0,360)),scale=float(self.rng.uniform(.7,1.15)),role='winter evergreen or leafless deciduous source tree' if winter else 'spring forest silhouette; species mix inferred')
        for i in range(count*2):
            x=float(self.rng.uniform(-63,63));z=float(self.rng.uniform(-61,61))
            if abs(x)<10 and -12<z<36:continue
            if disturbed(x,z,.3):continue
            if any(segment_distance(x,z,a,b)<w+1.5 for a,b,w in self.roads):continue
            if any(rect_distance(x,z,b['center_m'][0],b['center_m'][2],b['half_size_m'][0]+1,b['half_size_m'][1]+1)<.1 for b in self.buildings):continue
            key=['stone4','stone5','stone','log','stump'][i%5]
            self.put(key,x,z,yaw=float(self.rng.uniform(0,360)),scale=float(self.rng.uniform(.3,.75)),role='ground-scale source stone, branch/log or stump; never particle impostors')
    def make(self):
        self.pad(0,0,9,11);self.pad(36,39,6,7)
        if self.act==1:
            # Descend from a residential yard into a genuine below-grade shelter and back to
            # open ground through a walkable graded communication route.
            self.pad(-24,-6,7,7,-1.7)
            dist=segment_distance(X,Z,(-24,1),(-24,20));depth=np.clip((20-Z)/19,0,1)*1.7
            weight=1-smoothstep((dist-1.6)/1.2)
            self.h=self.h*(1-weight)-depth*weight
            self.room(0,0,3,4,main=True,damage=True,doorless=True,upper=True)
            self.room(-24,-6,3,3,damage=True,doorless=True)
            for x,z in [(-18,-30),(20,-24),(32,10)]:
                self.pad(x,z,6,6);self.room(x,z,3,2,damage=True,doorless=True,upper=(x<0))
            for j in range(42):
                x=-4.7+(j%6)*.28;z=-4.3+(j//6)*.55
                self.put('plank' if j%3 else 'board',x,z,y=.13+(j%3)*.035,yaw=10+(j%4)*8,role='fuel-sized dismantled timber, quantity/layout inferred from stripped-furniture condition')
            for j in range(14):
                self.put('plank',-20.8+(j%3)*.28,4.5+j*.58,yaw=75+j%5*9,role='timber trail toward the lower shelter mouth; inferred circulation')
            self.put('mattress',4,-4,y=.1,scale=.8,role='sparse worn bedding residue; exact household contents unasserted')
            self.put('bucket',-4,4,y=.1,role='unmarked domestic container; no inferred contents')
            for j in range(65):
                x=float(self.rng.uniform(-34,39));z=float(self.rng.uniform(-31,18))
                if abs(x)<8 or (-30<x<-17 and -13<z<23):continue
                self.put('rubble_field' if j%5==0 else 'brick',x,z,yaw=float(self.rng.uniform(0,360)),role='source masonry debris outside walking aisles')
            self.nature(42,True)
            self.features.append({'kind':'graded_shelter_route','from_xz':[-24,20],'to_xz':[-24,1],'drop_m':1.7,'inference':'generic deep communication route, not a surveyed trench'})
        elif self.act==2:
            for args in [(-27,2,4,6,1.2,-15,.36),(26,-4,4.5,7,1.05,25,.4),(-31,-25,3,3,.8,7,.25),(33,22,5,4,.9,-30,.3),(-28,40,3,6,1.0,10,.32)]:self.depression(*args)
            self.room(0,0,3,4,main=True)
            for j in range(12):
                x=(-4.5 if j%2 else 4.5);z=-5+(j//2)*1.3
                self.put('crate' if j%3 else 'wood_barrel',x,z,y=.1,yaw=(j*19)%90,role='generic unmarked stores along side bays; no pit function or owner asserted')
            self.put('furn_table',-4,4,y=.1,role='small shelter work surface, inferred')
            self.put('furn_chair',-4,2.4,y=.1,yaw=180,role='shelter chair, inferred')
            for j in range(25):
                x=-20+(j%5)*.72;z=22+(j//5)*.7
                self.put('crate',x,z,yaw=(j%4)*4,role='modest uncovered generic store cluster; no ammunition/fuel contents inferred')
            self.roads.extend([((-51,-17),(-17,22),3),((17,29),(48,-33),3)])
            self.nature(120)
        else:
            self.room(0,0,3,4,main=True,damage=True)
            self.pad(-27,-17,7,7);self.room(-27,-17,3,3,damage=True,doorless=True)
            # No markers, named identities, tools, lime, standardized spacing or total. Mounds
            # are local authored soil shapes in a recurring condition; count is not historical.
            for x,z,hx,hz,a in [(17,2,1.2,2.1,12),(23,8,1.0,1.8,-18),(18,16,1.1,2.2,26),(31,3,1.7,2.8,4)]:
                aa=math.radians(a);u=(X-x)*math.cos(aa)+(Z-z)*math.sin(aa);v=-(X-x)*math.sin(aa)+(Z-z)*math.cos(aa)
                self.h+=.23*np.exp(-((u/hx)**4+(v/hz)**4));self.soil_masks.append((x,z,hx*1.45,hz*1.3,a))
            self.depression(34,14,3.1,3.5,.55,5,.18)
            for x in [-38,-34,-30,-26,-22,-18,14,18,22,26,30,34]:
                self.put('fence_wood',x,25,scale=.7,role='interrupted domestic garden boundary, inferred source timber')
            self.put('furn_table',-4,-4,y=.1,role='remaining domestic furnishing; no named household')
            self.put('furn_chair',-3.5,-2.2,y=.1,yaw=155,role='remaining domestic furnishing')
            self.put('mattress',4,-4,y=.1,scale=.85,role='household residue away from circulation')
            self.put('wood_barrel',-4.5,3.8,y=.1,role='unmarked domestic storage; no burial supplies asserted')
            for j in range(30):
                x=-12-float(self.rng.uniform(0,9));z=float(self.rng.uniform(-6,17))
                self.put('rubble_field' if j%4==0 else 'brick',x,z,yaw=float(self.rng.uniform(0,360)),role='damaged-house masonry beside the garden approach')
            self.nature(78)
            self.features.append({'kind':'unmarked_soil_disturbance_and_reused_crater','historical_count_claim':False,'tools_markers_bodies':False})
        # Terrain roads are actual drivable grading. Details remain outside the ordinary route.
        for a,b,w in self.roads:
            d=segment_distance(X,Z,a,b);weight=1-smoothstep((d-w)/2)
            self.h*=1-.75*weight
        # The primary room and straight entry retain one exact shared datum after grading.
        self.pad(0,4,9,22)
        # Keep rigid buildings on their recorded foundation after road grading. Re-ground only
        # ground-attached props; furniture and upper modules keep the original rigid floor datum.
        for b in self.buildings:
            cx,base,cz=b['center_m'];hx,hz=b['half_size_m']
            self.pad(cx,cz,hx+.35,hz+.35,base)
        for item in self.items:
            if item['id'] in self.grounded:
                item['dy']=round(item['dy']+self.ground(item['dx'],item['dz'])-self.grounded[item['id']],5)
        return self
    def block(self):
        c=self.source
        p={'placement_class':c['provenance_class'],'source_ids':[x['id'] for x in c['source_records']],
          'source_urls':list(dict.fromkeys(x['url'] for x in c['source_records'])),
          'placement_constraints':list(dict.fromkeys([c['historical_region'],*c['placement_constraints']['requires']]))}
        return {'schema':'kessel.scene-block','version':1,'name':c['title']+f' — authored local study v{VERSION}',
          'anchor':{'origin_m':[0,0,0],'forward':'+z','grounding':'rigid_anchor'},
          'terrain':{'datum':'anchor_ground','res':N,'spacing_m':SPACING,'apron_m':12,'heights_m':np.round(self.h,6).ravel().tolist()},
          'items':self.items,'sockets':[{'id':'first_person_entry','kind':'player_eye','position_m':[0,1.7,24],'heading_deg':180},
            {'id':'first_person_look_target','kind':'look_target','position_m':[0,1.7,0],'heading_deg':180},
            {'id':'door_interaction','kind':'foot_route','position_m':[0,0,9.7],'heading_deg':180},
            {'id':'interior_review','kind':'foot_route','position_m':[0,.1,-3.225],'heading_deg':180}],
          'connectors':[{'id':'road_north','kind':'road','position_m':[0,self.ground(0,62),62],'heading_deg':0},
            {'id':'road_east','kind':'road','position_m':[62,self.ground(62,30),30],'heading_deg':90}],
          'environment':{'authored_lights':[],'sound_emitters':[]},
          'metadata':{'status':'authored_geometry_cpu_validated_pending_game_review','catalog_id':c['id'],'provenance':p,
            'source_records':c['source_records'],'source_visual_beats':c['visual_beats'],'source_caveats':c['caveats'],
            'original_draft_preserved':True,'buildings':self.buildings,'terrain_features':self.features,'roads':self.roads,
            'item_evidence':self.evidence,'weather_review':{'act_id':self.act,'phase':'day','endpoint':['blizzard','spring_clearing','bright_blue_day'][self.act-1],
              'historical_daily_weather_claim':False,'runtime_path':'Game::new -> Renderer::apply_campaign_atmosphere; independent Playtest freezes campaign progress'},
            'historical_integrity_boundary':'Only the catalog condition/region is sourced. Layout, terrain, architecture, individual props, quantities, tree mix and weather are disclosed authoring inferences, not a surveyed place or photographed instant. No canonical act or Berlin finale is replaced.',
            'remaining_requirements':['Actual whole-route first-person collision, door and visual/audio review in the game.','Source texture readability and all inferred architecture/prop suitability require human visual review.','No corresponding human activities or causal NPC scene scripts are staged.','Continuous campaign weather progression needs a separate ordinary campaign playthrough.']}}

def route_audit(study):
    # Body-width ray bundle through the static source geometry. Explicitly not a capsule sweep,
    # dynamic-door simulation, runtime navigation proof, or GPU appearance approval.
    collisions=[];rays=0
    for item in study.items:
        t,_=study.a.geometry(item['asset']);a=math.radians(item['deg']);r=np.array([[math.cos(a),0,math.sin(a)],[0,1,0],[-math.sin(a),0,math.cos(a)]])
        tt=(t*np.array(item['s']))@r.T+np.array([item['dx'],item['dy'],item['dz']])
        lo=tt.min((0,1));hi=tt.max((0,1))
        if hi[0]<-.3 or lo[0]>.3 or hi[2]<-3.5 or lo[2]>24:continue
        e1=tt[:,1]-tt[:,0];e2=tt[:,2]-tt[:,0];direction=np.array([0.,0.,-1.]);q=np.cross(direction,e2);det=(e1*q).sum(1)
        inv=np.divide(1,det,out=np.zeros_like(det),where=abs(det)>1e-9)
        for x in [-.28,0,.28]:
            for y in [.20,.6,1.1,1.8]:
                rays+=1;o=np.array([x,y,24]);s=o-tt[:,0];u=(s*q).sum(1)*inv;c=np.cross(s,e1);v=c@direction*inv;distance=(e2*c).sum(1)*inv
                mask=(abs(det)>1e-9)&(u>=0)&(v>=0)&(u+v<=1)&(distance>.0001)&(distance<27.3)
                if mask.any():collisions.append({'item_id':item['id'],'asset':item['asset'],'ray_x_y':[x,y],'nearest_m':float(distance[mask].min())})
    return {'method':'12 straight static GLB two-sided source rays at three lateral body samples/four heights; active hierarchy, all scale/yaw transforms',
      'route_xz':[[0,24],[0,-3.225]],'clear_static_route':not collisions,'collisions':collisions,
      'dynamic_doors':'not part of static GLB; forest/garden require ordinary E and completed leaf swing before entering',
      'terrain_route_max_abs_height_m':max(abs(study.ground(0,z)) for z in np.linspace(-3.225,24,100)),
      'runtime_capsule_or_visibility_claim':False,'candidate_asset_ray_tests':rays}

def ground_map(study):
    # Scene-scale real source material map. Never stretch a kilometre-scale campaign photograph
    # over a room-sized study. Native terrain detail remains the renderer's existing authority.
    spec=importlib.util.spec_from_file_location('scene_ground_tiles',ROOT/'tools/worldgen/make_ground_tex.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
    base='source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted/PSX Textures v3.1/PSX Textures/256/Color Maps/'
    names=['snow_1.png','dirt_5.png','dirt_2.png'] if study.act==1 else ['grass_7.png','dirt_5.png','dirt_2.png']
    paths=[base+n for n in names]
    for p in paths:assert (ROOT/p).is_file(),p
    tiles=[m.read_png(str(ROOT/p)) for p in paths]
    half=HALF+12+20;n=1024;axis=np.linspace(-half,half,n);x=axis[None,:];z=axis[:,None]
    def linear(v):v=v.astype(float)/255;return np.where(v<=.04045,v/12.92,((v+.055)/1.055)**2.4)
    fields=[]
    for t in tiles:
        ix=np.floor((axis+half)/12*t.shape[1]).astype(int)%t.shape[1];iz=np.floor((axis+half)/12*t.shape[0]).astype(int)%t.shape[0]
        fields.append(linear(t[iz[:,None],ix[None,:],:3]))
    cover,soil,litter=fields
    road=np.zeros((n,n))
    for a,b,w in study.roads:road=np.maximum(road,1-smoothstep((segment_distance(x,z,a,b)-w)/1.5))
    if study.act==1:road*=.24
    grove=smoothstep((np.maximum(abs(x),abs(z))-20)/28)*(.0 if study.act==1 else .62)
    rgb=cover*(1-grove[...,None])+litter*grove[...,None]
    disturbed=np.zeros((n,n))
    for cx,cz,hx,hz,angle in study.soil_masks:
        a=math.radians(angle);u=(x-cx)*math.cos(a)+(z-cz)*math.sin(a);v=-(x-cx)*math.sin(a)+(z-cz)*math.cos(a)
        radius=((u/hx)**4+(v/hz)**4)**.25;disturbed=np.maximum(disturbed,1-smoothstep((radius-.8)/.25))
    soil_weight=np.maximum(road,disturbed);rgb=rgb*(1-soil_weight[...,None])+soil*soil_weight[...,None]
    rgb=np.where(rgb<=.0031308,rgb*12.92,1.055*rgb**(1/2.4)-.055);rgb=np.round(np.clip(rgb,0,1)*255).astype(np.uint8)
    digest=hashlib.sha256(rgb.tobytes()).hexdigest();p=ROOT/f'assets/worldgen/derived/{digest[:16]}/{SHORT[study.act-1]}_source_ground.png';p.parent.mkdir(parents=True,exist_ok=True)
    if not p.exists():
        author.write_png(p,rgb);save(p.with_suffix('.source.json'),{'sources':[{'path':q,'sha256':sha(ROOT/q)} for q in paths],
          'mapping':'12 metre source tile repeat; local graded roads and disturbed-soil footprints; linear reflectance blend',
          'extent_half_m':half,'color':'retained source RGB, no exposure remap or fake lighting'})
    return relative(p)

def review(study,manifest):
    timeline=[{'from_s':1,'until_s':3,'input':{'look_dx':.991664348}},
      {'from_s':4.5,'until_s':8.5,'input':{'look_dx':-.991664348}},
      {'from_s':10,'until_s':12,'input':{'look_dx':.991664348}},
      {'from_s':12.5,'until_s':15.1,'input':{'drive':1}}]
    if study.act!=1:timeline.append({'at_s':15.5,'action':'door'})
    timeline.extend([{'from_s':17,'until_s':19.35,'input':{'drive':1}},
      {'from_s':21,'until_s':23,'input':{'look_dx':1.983328695}},
      {'from_s':23.5,'until_s':27.5,'input':{'look_dx':-1.983328695}},
      {'from_s':28,'until_s':30,'input':{'look_dx':1.983328695}}])
    expect=[{'metric':'on_foot','equals':True},{'metric':'foot_eye_above_feet_m','min_value':1.699,'max_value':1.701},
      {'metric':'foot_x_m','min_value':-.3,'max_value':.3,'at_s':20},
      {'metric':'foot_z_m','min_value':-4.3,'max_value':-2.2,'at_s':20},
      {'metric':'foot_y_m','min_value':.099,'max_value':.102,'at_s':20},
      {'metric':'foot_motion_clipped_m','min_value':0,'max_value':.6}]
    if study.act!=1:expect.append({'metric':'world_door_open_pairs','min_delta':1,'max_delta':1,'at_s':20})
    return {'schema':'kessel.first-person-mechanic.v1','id':'fp_historical_'+SHORT[study.act-1].replace('-','_')+f'_20260905_v{VERSION}',
      'title':study.source['title']+' — ordinary first-person environment walkthrough','act_id':study.act,'world_manifest':manifest,
      'night':False,'station':'commander','seconds':31,'setup':{'playtest':True,'enemy_policy':'banish','on_foot':True,
        'player_xz':[36,39],'foot_xz':[0,24],'look_at_xz':[0,0]},
      'status':'source_geometry_audited_pending_runtime_validation','historical_scope':study.block()['metadata']['historical_integrity_boundary'],
      'evidence_notes':['Actual Game camera and live audio. Initial position/heading and absence of combat are staged; remaining motion is ordinary mouse/analog movement and E interaction.',
        '640x360 logical render, exact integer 3x presentation to 1920x1080; no 5K sample workload. Ordinary normalized forward input uses the actual 5.5 m/s default gait.',
        'This is an authored local scene and act-profile endpoint. The three ordered clips do not claim a continuous weather transition or a historical day forecast.',
        'Static body-width ray bundle and shared terrain datum are CPU receipts only. Door sweep, runtime capsule contact, textures and final pixels still require executable review.'],
      'mechanics':['first_person_scene_traversal','source_pizza_interiors','act_day_weather_endpoint']+([] if study.act==1 else ['source_hinged_door']),
      'caption':'','timeline':timeline,'expect':expect}

def main():
    os.chdir(ROOT)
    if (PACKAGE/'queue.json').exists():raise FileExistsError('version already authored; create a new version instead of overwriting')
    PACKAGE.mkdir(parents=True,exist_ok=True);BLOCKS.mkdir(parents=True,exist_ok=True)
    originals={relative(p):sha(p) for folder in ['scene_blocks/astra_20260905','scene_plans/astra_20260905'] for p in (ROOT/'assets/worldgen'/folder).glob('*.json')}
    catalog=json.loads(CATALOG.read_text());rows={c['id']:c for c in catalog['scenes']};a=Assets()
    rpc=subprocess.Popen([sys.executable,str(ROOT/'tools/worldgen/editor_mcp.py')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
    calls=[]
    def call(name,args):
        request={'jsonrpc':'2.0','id':len(calls)+1,'method':'tools/call','params':{'name':name,'arguments':args}}
        rpc.stdin.write(json.dumps(request)+'\n');rpc.stdin.flush();result=json.loads(rpc.stdout.readline());calls.append({'request':request,'response':result})
        if result.get('result',{}).get('isError'):raise ValueError(result)
        return result['result']['structuredContent']
    entries=[]
    try:
        searches={query:call('pizza_asset_search',{'query':query,'limit':30}) for query in ['doorway_hr_2_wide','floor_ceiling_hr_1','tree','wooden_plank','debris_bricks','old_mattress']}
        save(PACKAGE/'asset-search-receipts.json',searches)
        for index,ident in enumerate(SELECTED,1):
            study=Study(a,rows[ident],index).make();block=study.block();block_path=BLOCKS/(ident+'.scene-block.v1.json')
            audit=route_audit(study)
            if not audit['clear_static_route']:raise ValueError(f'route obstructed: {ident}: {audit["collisions"]}')
            save(block_path,block);valid=call('scene_validate',{'source':relative(block_path)})
            cooked=call('scene_cook',{'source':relative(block_path),'name':'historical-'+SHORT[index-1]+f'-v{VERSION}','act_id':index})
            # Existing authoring publish API adds a local source-ground map to the cooked study,
            # making a second immutable revision; neither the scene nor its first cook is changed.
            world,heights,edits=author.load_world(cooked['manifest']);world['terrain']['texture']=ground_map(study)
            world['authoring']['historical_scene']=block['metadata'];world['authoring']['source_cook']=cooked['manifest']
            final=author.publish('historical-'+SHORT[index-1]+f'-ground-v{VERSION}',world,heights,edits,
              source={'manifest':cooked['manifest'],'hash':author.world_hash(cooked['manifest'])},
              recipe={'mode':'scene_source_ground','texture':world['terrain']['texture'],'source_scene':relative(block_path)})
            inspection=call('world_inspect',{'manifest':final['manifest'],'limit':3})
            spec=review(study,final['manifest']);spec_path=ROOT/'assets/review/first_person'/(spec['id']+'.json');save(spec_path,spec)
            save(PACKAGE/(SHORT[index-1]+'-route-source-audit.json'),audit)
            used={i['asset'] for i in study.items};assets=[a.geometry(p)[1] for p in sorted(used)]
            save(PACKAGE/(SHORT[index-1]+'-asset-provenance.json'),assets)
            entries.append({'catalog_id':ident,'scene':relative(block_path),'manifest':final['manifest'],'initial_cook':cooked['manifest'],
              'review_spec':relative(spec_path),'review_seconds':spec['seconds'],'act_weather_endpoint':index,
              'source_ids':valid['provenance']['source_ids'],'source_urls':valid['provenance']['source_urls'],
              'placements':len(study.items),'unique_assets':len(used),'static_route_clear':audit['clear_static_route'],
              'terrain_range_m':[float(heights.min()),float(heights.max())],
              'status':'validated_scene_api_and_source_route_only','runtime_video_pass':False,
              'requested_output_prefix':'captures/astra/'+spec['id'],'environment':inspection['environment']})
            print(json.dumps({k:entries[-1][k] for k in ['catalog_id','placements','unique_assets','manifest','review_spec']}),flush=True)
    finally:
        rpc.stdin.close();rpc.wait();save(PACKAGE/'editor-mcp-transcript.json',calls)
    preserved={p:sha(ROOT/p)==h for p,h in originals.items()};assert all(preserved.values())
    save(PACKAGE/'original-draft-preservation.json',{'original_sha256':originals,'unchanged':preserved,'canonical_acts_modified':False})
    save(PACKAGE/'queue.json',{'schema':'kessel.historical-scene-review-queue','version':VERSION,'entries':entries,
      'supersedes_review_queue':'artifacts/astra-historical-scene-studies-20260905-v3/queue.json',
      'corrections':'v2 repaired shelter grade, grounding and actual foot controls; v3 cleared disturbed footprints. v4 restores the normal 10 cm source floor threshold above terrain, lifts furnishings, and checks standing floor support. All earlier outputs remain preserved.',
      'canonical_acts_modified':False,'source_catalog_scenes':len(rows),'remaining_catalog_scenes_not_completed_by_this_batch':len(rows)-3,
      'weather_sequence':'three separate existing Game profile endpoints: blizzard -> clearing -> bright blue day; continuous transition not yet filmed',
      'rendering':'root-coordinated only, actual game first person and audio, 640x360 native integer3x',
      'no_gpu_or_cargo_run':True})

if __name__=='__main__':main()
