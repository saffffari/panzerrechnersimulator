#!/usr/bin/env python3
"""Tiny reusable geometry for snow accumulation and impact scars; no source assets are altered."""
import json
import math
from pathlib import Path
import struct

DEST=Path('/home/alex/panzer/tank_simulator/assets/authoring/battlefield')


def disk(name, color, radii, heights, vertical=False):
    positions=[[0,heights[0],0]]
    normals=[[0,1,0]]
    count=24
    for ring,(radius,height) in enumerate(zip(radii,heights[1:])):
        for i in range(count):
            angle=i*math.tau/count
            irregular=1+.12*math.sin(angle*5+.7)+.06*math.cos(angle*9)
            positions.append([math.cos(angle)*radius*irregular,height,math.sin(angle)*radius*irregular])
            normals.append([0,1,0])
    faces=[]
    for i in range(count):faces.extend([0,1+(i+1)%count,1+i])
    for ring in range(len(radii)-1):
        for i in range(count):
            a=1+ring*count+i;b=1+ring*count+(i+1)%count;c=a+count;d=b+count
            faces.extend([a,b,d,a,d,c])
    if vertical:
        positions=[[x,z,y] for x,y,z in positions]
        normals=[[0,0,1] for _ in normals]
    pb=b''.join(struct.pack('<3f',*p) for p in positions)
    nb=b''.join(struct.pack('<3f',*p) for p in normals)
    ib=struct.pack('<'+'I'*len(faces),*faces)
    data=pb+nb+ib
    j={'asset':{'version':'2.0','generator':'Panzer winter battlefield authoring'},
       'scene':0,'scenes':[{'nodes':[0]}],'nodes':[{'mesh':0,'name':name}],
       'meshes':[{'primitives':[{'attributes':{'POSITION':0,'NORMAL':1},'indices':2,'material':0}]}],
       'materials':[{'name':name,'pbrMetallicRoughness':{'baseColorFactor':[*color,1],
          'metallicFactor':0,'roughnessFactor':1},'doubleSided':True}],
       'buffers':[{'byteLength':len(data)}],
       'bufferViews':[{'buffer':0,'byteOffset':0,'byteLength':len(pb)},
          {'buffer':0,'byteOffset':len(pb),'byteLength':len(nb)},
          {'buffer':0,'byteOffset':len(pb)+len(nb),'byteLength':len(ib)}],
       'accessors':[{'bufferView':0,'componentType':5126,'count':len(positions),'type':'VEC3',
          'min':[min(p[i] for p in positions) for i in range(3)],
          'max':[max(p[i] for p in positions) for i in range(3)]},
          {'bufferView':1,'componentType':5126,'count':len(normals),'type':'VEC3'},
          {'bufferView':2,'componentType':5125,'count':len(faces),'type':'SCALAR'}]}
    jb=json.dumps(j,separators=(',',':')).encode();jb+=b' '*((-len(jb))%4);data+=b'\0'*((-len(data))%4)
    blob=struct.pack('<III',0x46546c67,2,28+len(jb)+len(data))+struct.pack('<II',len(jb),0x4e4f534a)+jb+struct.pack('<II',len(data),0x004e4942)+data
    p=DEST/f'{name}.glb'
    if p.exists():raise RuntimeError(f'Preserving {p}')
    p.write_bytes(blob)
    return {'asset':str(p),'triangles':len(faces)//3,'source':'authored procedural geometry'}


if __name__=='__main__':
    DEST.mkdir(parents=True,exist_ok=True)
    report=[disk('snow_drift',[.76,.80,.84],[.3,.7,1],[.10,.13,.06,0]),
        disk('scorched_earth',[.08,.065,.05],[.35,.8,1],[.015,.02,.07,0]),
        disk('shell_chip',[.095,.07,.052],[.45,1],[.022,.012,0],vertical=True)]
    (DEST/'winter-surfaces.receipt.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report))
