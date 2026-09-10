#!/usr/bin/env python3
"""Read-only supplied-GLB ray audit from the actual hull-local driver eye to visor centre.

Memory maps buffers and tests only intersected node boxes. Embedded texture bytes are not decoded.
Triangle intersections are two-sided, matching the physical obstruction question rather than a
particular raster culling setting. This is source geometry evidence, not a rendered frame.
"""
import json
import mmap
from pathlib import Path
import struct
import numpy as np

ROOT=Path(__file__).resolve().parents[1]

def trs(node):
    if 'matrix' in node:return np.array(node['matrix'],float).reshape(4,4).T
    x,y,z,w=node.get('rotation',[0,0,0,1]);s=np.array(node.get('scale',[1,1,1]))
    r=np.array([[1-2*y*y-2*z*z,2*x*y-2*z*w,2*x*z+2*y*w],
                [2*x*y+2*z*w,1-2*x*x-2*z*z,2*y*z-2*x*w],
                [2*x*z-2*y*w,2*y*z+2*x*w,1-2*x*x-2*y*y]])
    m=np.eye(4);m[:3,:3]=r@np.diag(s);m[:3,3]=node.get('translation',[0,0,0]);return m

def hits(path, origin, direction, max_distance=3.0, triangle_transform=None):
    found=[]
    with path.open('rb') as f:
        header=f.read(12);assert header[:4]==b'glTF'
        size,kind=struct.unpack('<II',f.read(8));doc=json.loads(f.read(size))
        size,kind=struct.unpack('<II',f.read(8));start=f.tell();assert kind==0x004e4942
        with mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ) as data:
            def accessor(i):
                a=doc['accessors'][i];v=doc['bufferViews'][a['bufferView']]
                dtype={5120:'i1',5121:'u1',5122:'<i2',5123:'<u2',5125:'<u4',5126:'<f4'}[a['componentType']]
                width={'SCALAR':1,'VEC2':2,'VEC3':3,'VEC4':4,'MAT4':16}[a['type']]
                row=np.dtype(dtype).itemsize*width
                return np.ndarray((a['count'],width),dtype=dtype,buffer=data,
                    offset=start+v.get('byteOffset',0)+a.get('byteOffset',0),strides=(v.get('byteStride',row),np.dtype(dtype).itemsize)).copy()
            def visit(idx,parent):
                node=doc['nodes'][idx];m=parent@trs(node)
                if 'mesh' in node:
                    mesh=doc['meshes'][node['mesh']]
                    for pi,p in enumerate(mesh['primitives']):
                        a=doc['accessors'][p['attributes']['POSITION']]
                        corners=np.array([[x,y,z,1] for x in (a['min'][0],a['max'][0]) for y in (a['min'][1],a['max'][1]) for z in (a['min'][2],a['max'][2])])@m.T
                        lo=corners[:,:3].min(0);hi=corners[:,:3].max(0)
                        with np.errstate(divide='ignore',invalid='ignore'):
                            t0=(lo-origin)/direction;t1=(hi-origin)/direction
                        near=max(float(np.minimum(t0,t1).max()),0);far=min(float(np.maximum(t0,t1).min()),max_distance)
                        if near>far:continue
                        v=accessor(p['attributes']['POSITION']);v=v@m[:3,:3].T+m[:3,3]
                        idxs=accessor(p['indices']).ravel() if 'indices' in p else np.arange(len(v))
                        assert p.get('mode',4)==4
                        triangles=v[idxs.reshape(-1,3)]
                        if triangle_transform is not None:
                            triangles=triangle_transform(node.get('name',''),triangles)
                        e1=triangles[:,1]-triangles[:,0];e2=triangles[:,2]-triangles[:,0]
                        q=np.cross(np.broadcast_to(direction,e2.shape),e2);det=np.sum(e1*q,axis=1)
                        inv=np.divide(1,det,out=np.zeros_like(det),where=abs(det)>1e-10)
                        s=origin-triangles[:,0];u=np.sum(s*q,axis=1)*inv
                        cross=np.cross(s,e1);vv=cross@direction*inv;t=np.sum(e2*cross,axis=1)*inv
                        valid=(abs(det)>1e-10)&(u>=-1e-7)&(vv>=-1e-7)&(u+vv<=1+1e-7)&(t>1e-5)&(t<max_distance)
                        mat=doc.get('materials',[])[p.get('material',0)]
                        for ti in np.nonzero(valid)[0]:
                            normal=np.cross(e1[ti],e2[ti]);normal/=max(np.linalg.norm(normal),1e-12)
                            found.append(dict(file=str(path.relative_to(ROOT)),node=node.get('name',str(idx)),node_index=idx,
                                mesh=mesh.get('name'),primitive=pi,triangle=int(ti),distance_m=float(t[ti]),
                                point=(origin+direction*t[ti]).tolist(),triangle_vertices=triangles[ti].tolist(),normal=normal.tolist(),front_facing=bool(normal@direction<0),
                                material=mat.get('name'),alpha_mode=mat.get('alphaMode','OPAQUE'),double_sided=mat.get('doubleSided',False),
                                bounds_min=lo.tolist(),bounds_max=hi.tolist()))
                for c in node.get('children',[]):visit(c,m)
            for n in doc['scenes'][doc.get('scene',0)]['nodes']:visit(n,np.eye(4))
    return sorted(found,key=lambda h:h['distance_m'])

def main():
    origin=np.array([.535,1.68,1.82]);target=np.array([.5468,1.7376,2.3859]);direction=target-origin;direction/=np.linalg.norm(direction)
    output=[]
    for name in ('tiger_hull.glb','tiger_rtt_hull.glb'):
        output+=hits(ROOT/'assets/tanks/tiger/baked'/name,origin,direction)
    report=dict(eye_hull_local_m=origin.tolist(),target_hull_local_m=target.tolist(),direction=direction.tolist(),
        hits=sorted(output,key=lambda h:h['distance_m']),method='Supplied GLB scene hierarchy and accessor geometry; two-sided exact triangle ray; no texture decode or GPU.')
    out=ROOT/'artifacts/astra-audit-20260905/driver-visor-source-ray.json';out.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))

if __name__=='__main__':main()
