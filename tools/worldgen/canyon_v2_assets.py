#!/usr/bin/env python3
"""Small engine-native, reproducible Canyon v2 props. Metres; palette is sRGB albedo.

Anonymous clothed combatants, field works, wrecked carts and natural rock masses.
These are inferred composite props, not portraits or reconstructions of named deaths.
No purchased geometry is copied into this generator.
"""
import io
import hashlib
import json
import math
import struct
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
PREFIX = 'assets/authoring/canyon-v2/'
SNOW = (234, 237, 241)
WOOD = (91, 76, 62)
STEEL = (53, 54, 52)


def resting_log(source):
    """Lay the purchased leaning trunk on its measured long axis; preserve its mesh/materials."""
    source = Path(source)
    raw = source.read_bytes()
    size = struct.unpack_from('<I', raw, 12)[0]
    doc = json.loads(raw[20:20+size])
    assert len(doc['nodes']) == 1 and set(doc['nodes'][0]) <= {'mesh', 'name'}
    binary = raw[28+size:]
    accessor = doc['accessors'][doc['meshes'][0]['primitives'][0]['attributes']['POSITION']]
    view = doc['bufferViews'][accessor['bufferView']]
    assert accessor['componentType'] == 5126 and view.get('byteStride', 12) == 12
    points = np.frombuffer(binary, '<f4', accessor['count']*3,
                          view.get('byteOffset', 0)+accessor.get('byteOffset', 0)).reshape(-1, 3)
    _, axes = np.linalg.eigh(np.cov(points[:, :2].T))
    axis = axes[:, -1]
    if axis[0] < 0: axis = -axis
    angle = -math.atan2(axis[1], axis[0])
    co, si = math.cos(angle), math.sin(angle)
    rotation = np.array([[co, -si, 0], [si, co, 0], [0, 0, 1]])
    rotated = points @ rotation.T
    translation = np.array([-rotated[:, 0].mean(), -rotated[:, 1].min(), 0])
    doc['nodes'][0].update(rotation=[0, 0, math.sin(angle/2), math.cos(angle/2)],
                           translation=translation.tolist())
    js = json.dumps(doc, separators=(',', ':')).encode(); js += b' '*(-len(js)%4)
    out = ROOT/PREFIX/'resting_tree_log.glb'
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(struct.pack('<III', 0x46546c67, 2, 28+len(js)+len(binary))
        + struct.pack('<II', len(js), 0x4e4f534a)+js
        + struct.pack('<II', len(binary), 0x004e4942)+binary)
    posed = rotated+translation
    return dict(path=str(out.relative_to(ROOT)), source=str(source.relative_to(ROOT)),
                source_sha256=hashlib.sha256(raw).hexdigest(), rotation_z_deg=math.degrees(angle),
                bounds=[posed.min(0).tolist(), posed.max(0).tolist()], owner_private=True)


class Mesh:
    def __init__(self):
        self.points, self.normals, self.slots, self.colors = [], [], [], []

    def tri(self, a, b, c, color):
        pts = np.array([a, b, c], dtype=float)
        n = np.cross(pts[1]-pts[0], pts[2]-pts[0])
        length = np.linalg.norm(n)
        if length < 1e-9:
            return
        if color not in self.colors:
            self.colors.append(color)
        self.points.extend(pts.tolist())
        self.normals.extend([list(n/length)]*3)
        self.slots.extend([self.colors.index(color)]*3)

    def quad(self, a, b, c, d, color):
        self.tri(a, b, c, color)
        self.tri(a, c, d, color)

    def box(self, center, size, color):
        p = np.array(center); r = np.array(size)*.5
        v = [p+r*np.array(s) for s in [(-1,-1,-1),(1,-1,-1),(1,1,-1),(-1,1,-1),
                                      (-1,-1,1),(1,-1,1),(1,1,1),(-1,1,1)]]
        for face in [(0,3,2,1),(4,5,6,7),(0,4,7,3),(1,2,6,5),(3,7,6,2),(0,1,5,4)]:
            self.quad(*(v[i] for i in face), color)

    def limb(self, a, b, r0, r1, color, sides=8):
        a, b = np.array(a), np.array(b)
        axis = b-a; axis /= np.linalg.norm(axis)
        u = np.cross(axis, [1,0,0] if abs(axis[0]) < .8 else [0,1,0]); u /= np.linalg.norm(u)
        v = np.cross(axis, u)
        rings = [[p+r*(u*math.cos(i*math.tau/sides)+v*math.sin(i*math.tau/sides))
                  for i in range(sides)] for p,r in [(a,r0),(b,r1)]]
        for i in range(sides):
            j = (i+1)%sides
            self.quad(rings[0][i],rings[0][j],rings[1][j],rings[1][i],color)
            self.tri(a,rings[0][j],rings[0][i],color)
            self.tri(b,rings[1][i],rings[1][j],color)

    def ellipsoid(self, center, size, color, seed=0, rock=False):
        rings=[]; n=11
        for j in range(7):
            phi=-math.pi/2+j*math.pi/6
            ring=[]
            for i in range(n):
                a=i*math.tau/n
                irregular=1+(.13*math.sin(i*2.71+j*3.13+seed) if rock else 0)
                q=np.array([math.cos(phi)*math.cos(a), math.sin(phi),math.cos(phi)*math.sin(a)])
                ring.append(np.array(center)+q*np.array(size)*.5*irregular)
            rings.append(ring)
        for j in range(6):
            for i in range(n):
                k=(i+1)%n
                # A thin retained snow mantle on upward rock faces; material, never emission.
                col=SNOW if rock and j>=4 else color
                self.quad(rings[j][i],rings[j+1][i],rings[j+1][k],rings[j][k],col)

    def write(self, path):
        path=Path(path); path.parent.mkdir(parents=True,exist_ok=True)
        pts=np.asarray(self.points,dtype='<f4'); normals=np.asarray(self.normals,dtype='<f4')
        uv=np.array([[(s+.5)/len(self.colors),.5] for s in self.slots],dtype='<f4')
        ids=np.arange(len(pts),dtype='<u4')
        im=Image.new('RGB',(len(self.colors),1)); im.putdata(self.colors)
        png=io.BytesIO();im.save(png,format='PNG')
        chunks=[pts.tobytes(),normals.tobytes(),uv.tobytes(),ids.tobytes(),png.getvalue()]
        data=bytearray(); views=[]
        for chunk in chunks:
            data.extend(b'\0'*(-len(data)%4));views.append(dict(buffer=0,byteOffset=len(data),byteLength=len(chunk)));data.extend(chunk)
        accessors=[dict(bufferView=i,componentType=5126 if i<3 else 5125,count=len(pts),type=t)
                   for i,t in enumerate(['VEC3','VEC3','VEC2','SCALAR'])]
        accessors[0].update(min=pts.min(0).tolist(),max=pts.max(0).tolist())
        doc=dict(asset=dict(version='2.0',generator='Panzer Canyon v2 native procedural props'),
                 scene=0,scenes=[dict(nodes=[0])],nodes=[dict(mesh=0,name=path.stem)],
                 meshes=[dict(primitives=[dict(attributes=dict(POSITION=0,NORMAL=1,TEXCOORD_0=2),indices=3,material=0)])],
                 materials=[dict(name=path.stem,pbrMetallicRoughness=dict(baseColorTexture=dict(index=0),metallicFactor=0,roughnessFactor=.95),doubleSided=True)],
                 images=[dict(bufferView=4,mimeType='image/png')],textures=[dict(source=0,sampler=0)],
                 samplers=[dict(magFilter=9728,minFilter=9728)],buffers=[dict(byteLength=len(data))],bufferViews=views,accessors=accessors)
        js=json.dumps(doc,separators=(',',':')).encode();js+=b' '*(-len(js)%4);data.extend(b'\0'*(-len(data)%4))
        raw=struct.pack('<III',0x46546c67,2,28+len(js)+len(data))+struct.pack('<II',len(js),0x4e4f534a)+js+struct.pack('<II',len(data),0x004e4942)+data
        path.write_bytes(raw)
        return dict(path=str(path.relative_to(ROOT)),triangles=len(pts)//3,bounds=[pts.min(0).tolist(),pts.max(0).tolist()])


def casualty(pose):
    m=Mesh();coat=[(76,72,64),(83,73,60),(60,65,61)][pose%3];boots=(34,31,30)
    # Head toward +Z. Heavy coat hides anatomy; the head is turned into the snow.
    side=pose%2; y=.21 if side else .15
    m.ellipsoid((0,y,.02),(.48,.30,.72),coat)
    m.ellipsoid((.04*side,.16,.53),(.25,.23,.28),(78,76,68))
    m.ellipsoid((.04*side,.22,.56),(.29,.20,.28),(63,65,61))
    hips=[(-.13,.15,-.28),(.13,.15,-.28)]
    knees=[(-.18-.06*side,.10,-.63),(.22+.12*side,.12,-.58)]
    feet=[(-.18-.12*side,.08,-.97),(.27+.21*side,.09,-.89+.12*(pose%3))]
    for a,b,c in zip(hips,knees,feet):
        m.limb(a,b,.115,.09,coat);m.limb(b,c,.085,.065,boots)
        m.ellipsoid((c[0],c[1],c[2]-.04),(.17,.16,.25),boots)
    arms=[((-.22,y,.25),(-.41,.10,.09),(-.48,.08,.36)),
          ((.22,y,.25),(.41,.10,.31 if side else .02),(.28 if side else .48,.08,.55 if side else .18))]
    for a,b,c in arms:
        m.limb(a,b,.083,.072,coat);m.limb(b,c,.07,.053,coat)
        m.ellipsoid(c,(.12,.085,.16),(63,53,47))
    # Small windward deposits leave the dark coat's silhouette legible.
    m.ellipsoid((-.06,y+.12,-.10),(.23,.04,.32),SNOW)
    return m


def emplacement():
    m=Mesh()
    # 6 m wide U-shaped breastwork; an open 3.8 m rear access, no invisible solid room.
    for layer in range(5):
        y=.12+layer*.23
        for a,b in [((-3,y,1.8),(3,y,1.8)),((-3,y,-1.8),(-3,y,1.8)),((3,y,-1.8),(3,y,1.8))]:
            m.limb(a,b,.145,.13,WOOD,7)
    for x in (-2.35,2.35):
        m.box((x,.3,.9),(.9,.6,1.15),(96,83,63))
        m.box((x,.62,.9),(.95,.05,1.2),SNOW)
    return m


def cart():
    m=Mesh()
    # Broken wooden transport cart, inferred general period construction (not a vehicle replica).
    for x in np.linspace(-.72,.72,7):m.box((x,.68,0),(.17,.12,2.6),WOOD)
    for x in (-.81,.81):
        for z in (-.9,0,.9):m.box((x,1.01,z),(.10,.66,.12),WOOD)
        m.box((x,1.15,-.3),(.10,.17,1.85),WOOD)
    m.limb((-1,.4,.5),(1,.4,.5),.06,.06,STEEL)
    for x in (-.98,.98):
        for i in range(12):
            a=i*math.tau/12;b=(i+1)*math.tau/12
            p=(x,.4+.38*math.sin(a),.5+.38*math.cos(a));q=(x,.4+.38*math.sin(b),.5+.38*math.cos(b))
            m.limb(p,q,.045,.045,WOOD,5)
            if i%2==0:m.limb((x,.4,.5),p,.025,.025,WOOD,5)
    m.limb((-.55,.7,1),(-.55,.13,3),.06,.04,WOOD)
    m.limb((.55,.7,1),(.68,.18,2.5),.06,.04,WOOD)
    return m


def gun():
    m=Mesh(); rust=(78,65,56)
    # Disabled 45 mm-like field gun: missing right wheel, slumped barrel, closed trails.
    m.limb((-.64,.45,0),(.62,.20,0),.10,.06,STEEL)
    for i in range(12):
        a=i*math.tau/12;b=(i+1)*math.tau/12
        p=(-.69,.45+.43*math.sin(a),.43*math.cos(a));q=(-.69,.45+.43*math.sin(b),.43*math.cos(b))
        m.limb(p,q,.065,.065,(34,33,31),6)
        if i%2==0:m.limb((-.69,.45,0),p,.04,.04,STEEL)
    for x in (-.25,.20):m.limb((x,.45,-.05),(x+.12,.08,-1.5),.065,.05,rust)
    m.box((-.05,.7,.12),(1.25,.72,.055),rust)
    m.limb((-.02,.73,-.25),(.13,.32,1.8),.085,.045,STEEL)
    m.box((-.02,.67,-.37),(.27,.24,.45),STEEL)
    return m


def build():
    result={}
    for i in range(6):result[f'casualty_{i}']=casualty(i)
    result.update(outpost=emplacement(),wreck_cart=cart(),wreck_gun45=gun())
    for i,size in enumerate([(8.5,5.2,6),(13.5,8.5,9),(5.4,2.8,4)]):
        m=Mesh();m.ellipsoid((0,size[1]*.40,0),size,(105+i*6,106+i*5,111+i*4),i+11,True)
        result[f'rock_{i}']=m
    return {name:mesh.write(ROOT/PREFIX/(name+'.glb')) for name,mesh in result.items()}


if __name__=='__main__':
    print(json.dumps(build(),indent=2))
