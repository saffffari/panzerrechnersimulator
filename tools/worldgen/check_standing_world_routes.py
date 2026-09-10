#!/usr/bin/env python3
"""Source-mesh standing-volume gates; no renderer or outcome substitution."""
import json
from pathlib import Path
import numpy as np
import world_authoring as world
from cook_playtest_ground_detail import mesh_triangles, transformed
from check_playtest_metric_clearance import inspect, HEIGHT, EYE, hits

CELLAR='assets/worldgen/revisions/cellar-study/20260905T114300928810Z_38668d5d/manifest.v1.json'
PLAYTEST='assets/worldgen/revisions/playtest-ground-detail/20260905T151851623908Z_be1c0a5f/manifest.v1.json'

def intersects(tris,lo,hi):
    """Complete triangle/AABB separating-axis test (three box, normal, nine edge cross axes)."""
    if not len(tris): return np.zeros(0,dtype=bool)
    center=(lo+hi)*.5;half=(hi-lo)*.5;v=tris-center
    live=np.ones(len(v),dtype=bool)
    edges=np.roll(v,-1,axis=1)-v
    axes=[np.tile(axis,(len(v),1)) for axis in np.eye(3)]
    axes.append(np.cross(edges[:,0],edges[:,1]))
    for edge in range(3):
        for axis in np.eye(3):axes.append(np.cross(edges[:,edge],axis))
    for axis in axes:
        projected=np.einsum('nvi,ni->nv',v,axis);radius=np.sum(np.abs(axis)*half,axis=1)
        live &= (projected.min(axis=1)<=radius+1e-9)&(projected.max(axis=1)>=-radius-1e-9)
    return live

def load_triangles(path):
    manifest,h,edits=world.load_world(path)
    assert not edits.get('scene_blocks'), 'requires materialized scene'
    cache={};tris=[];ids=[]
    for identity,p in zip(manifest['placement_ids'],manifest['placements']):
        asset=manifest['assets'][p[0]]
        if asset not in cache:cache[asset]=mesh_triangles(asset)
        t=transformed(cache[asset],p);tris.append(t);ids.extend([(identity,asset)]*len(t))
    return manifest,h,np.concatenate(tris),ids

def route(path,points,label):
    m,h,tri,ids=load_triangles(path); tlo=tri.min(axis=1);thi=tri.max(axis=1)
    floor_tri=tri[np.array(['floor_ceiling_' in asset for _,asset in ids])]
    def supported_y(point):
        y=world.sample_height(h,m['terrain']['spacing'],float(point[0]),float(point[1]))
        if not len(floor_tri):return y
        near=floor_tri[(floor_tri[:,:,0].min(1)<=point[0])&(floor_tri[:,:,0].max(1)>=point[0])
            &(floor_tri[:,:,2].min(1)<=point[1])&(floor_tri[:,:,2].max(1)>=point[1])]
        # Current reviewed slabs have downward undersides below the terrain and upward tops
        # within one 30 cm step. The runtime additionally uses authored normals to reject undersides.
        support=hits(near,*point,y-.02,y+.30) if len(near) else []
        return max(support,default=y)
    records=[];blocked=[]
    for a,b in zip(points[:-1],points[1:]):
        ya=supported_y(a)
        yb=supported_y(b)
        # Exact triangle tests against a conservative swept upper-body box. The lower 0.35 m is
        # the step/terrain-support zone, not a claim to have tested shoe collision here.
        lo=np.array([min(a[0],b[0])-.28,min(ya,yb)+.35,min(a[1],b[1])-.28])
        hi=np.array([max(a[0],b[0])+.28,max(ya,yb)+HEIGHT,max(a[1],b[1])+.28])
        candidate=np.flatnonzero(np.all(thi>=lo,axis=1)&np.all(tlo<=hi,axis=1))
        hit=candidate[intersects(tri[candidate],lo,hi)]
        if len(hit):blocked.append({'from_xz':list(a),'to_xz':list(b),'source_hits':sorted(set(ids[i] for i in hit))})
        records.append({'from_xz':list(a),'to_xz':list(b),'feet_y_m':[ya,yb],'candidate_triangles':len(candidate),'body_triangle_hits':len(hit)})
    return {'label':label,'world':path,'segments':len(records),'radius_m':.28,'standing_height_m':HEIGHT,
            'step_zone_excluded_m':.35,'feet_support':'actual source slab top within a 30 cm step or terrain','blocked_segments':blocked,'samples':records}

def main():
    metrics=inspect()
    cellar=route(CELLAR,[(0.0,float(z)) for z in np.linspace(-15,2,341)],'standing cellar approach and earthen descent')
    # The rigid floor opening/roof dimensions are sampled separately in inspect(). These source
    # paths are the actual passage centerlines; independently moving leaves need live-door gates.
    garage=route(PLAYTEST,[(-67.,float(z)) for z in np.linspace(98.2,93.8,89)],'garage passage')
    warehouse=route(PLAYTEST,[(-112.,float(z)) for z in np.linspace(40.2,35.8,89)],'warehouse passage')
    report={'schema':'kessel.standing-world-route-source.v1','metrics':metrics,'routes':[cellar,garage,warehouse],
       'scope':'Conservative swept upper-body boxes against actual source triangles; low foot/step collision and moving door runtime outcomes are separate gates.'}
    out=world.ROOT/'artifacts/astra-audit-20260905/standing-world-routes-v15-source.json'
    if out.exists():raise ValueError('preserving existing receipt')
    out.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'receipt':str(out),'routes':[{'label':r['label'],'segments':r['segments'],'blocked':len(r['blocked_segments'])} for r in report['routes']],
                     'standing_height_m':HEIGHT,'eye_m':EYE,'door_clearance_m':metrics['new_door_clearance_m'],'roof_samples':metrics['roof_samples']}))
    assert all(not r['blocked_segments'] for r in report['routes']), 'source standing route has actual obstacles'
if __name__=='__main__':main()
