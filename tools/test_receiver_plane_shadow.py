#!/usr/bin/env python3
"""CPU geometric checks for the raster receiver-plane PCF correction; no game process.

Samples depth from analytic planes in orthographic and perspective cubemap light cameras,
independently of the shader's receiver depth construction. Old and corrected PCF share samples.
This establishes the numerical cause; an actual GPU scene capture remains a separate check.
"""
import argparse
import json
import math
from pathlib import Path
import numpy as np


def normalize(v):
    v=np.asarray(v,dtype=float)
    return v/np.linalg.norm(v)


def light_matrix(sun,half,depth_range):
    sun=normalize(sun)
    right=normalize(np.cross(-sun,[0,1,0]))
    up=np.cross(right,-sun)
    return np.array([right/half,up/half,-sun/depth_range])


def shader_gradient(matrix,normal):
    plane=(matrix@normal)/np.sum(matrix*matrix,axis=1)
    return np.array([-2*plane[0],2*plane[1]])/plane[2]


def exact_depth(matrix,normal,uv):
    # Orthographic ray parallel to inverse(M)[:,2], solved against n.world == 0.
    inverse=np.linalg.inv(matrix)
    xy=np.stack([2*uv[...,0]-1,1-2*uv[...,1],np.zeros(uv.shape[:-1])],axis=-1)
    at_zero=xy@inverse.T
    return -(at_zero@normal)/(inverse[:,2]@normal)


def pcf(matrix,normal,uv,size,bias_depth,corrected,occluder_depth=0):
    grad=shader_gradient(matrix,normal)
    reference=exact_depth(matrix,normal,uv)-bias_depth
    total=np.zeros(uv.shape[:-1])
    for ox,oy in [(-.45,-.45),(.45,-.45),(-.45,.45),(.45,.45)]:
        pixel=uv*size+np.array([ox,oy])-.5
        first=np.floor(pixel)
        fraction=pixel-first
        for x,y in [(0,0),(1,0),(0,1),(1,1)]:
            texel=np.clip(first+[x,y],0,size-1)
            sample_uv=(texel+.5)/size
            stored=exact_depth(matrix,normal,sample_uv)-occluder_depth
            ref=reference+((sample_uv-uv)@grad if corrected else 0)
            weight=(fraction[...,0] if x else 1-fraction[...,0])*(fraction[...,1] if y else 1-fraction[...,1])
            total+=.25*weight*(ref<=stored)
    return total


def cube_frames():
    # Same look directions/up axes as the six raster cameras in cabin_shadows.rs.
    forwards=np.array([[1,0,0],[-1,0,0],[0,1,0],[0,-1,0],[0,0,1],[0,0,-1]])
    ups=np.array([[0,-1,0],[0,-1,0],[0,0,1],[0,0,-1],[0,-1,0],[0,-1,0]])
    right=np.cross(forwards,ups)
    return forwards,right,np.cross(right,forwards)


def cube_project(direction):
    a=np.abs(direction)
    axis=np.argmax(a,axis=-1)
    faces=axis*2+(np.take_along_axis(direction,axis[...,None],axis=-1)[...,0]<0)
    f,r,u=cube_frames()
    major=np.sum(direction*f[faces],axis=-1)
    uv=np.stack([np.sum(direction*r[faces],axis=-1),
                 np.sum(direction*u[faces],axis=-1)],axis=-1)/major[...,None]*.5+.5
    return faces,uv


def cube_shader_ray(faces,uv):
    # Explicit inverse used by the shader; checked against independent look-at camera bases.
    x,y=np.moveaxis(2*uv-1,-1,0)
    one=np.ones_like(x)
    choices=np.stack([np.stack(v,axis=-1) for v in [
        (one,-y,-x),(-one,-y,x),(x,one,y),(x,-one,-y),(x,-y,one),(-x,-y,-one)]])
    return np.take_along_axis(choices,np.broadcast_to(faces[None,...,None],(1,)+choices.shape[1:]),axis=0)[0]


def cube_camera_ray(faces,uv):
    f,r,u=cube_frames()
    st=2*uv-1
    return f[faces]+r[faces]*st[...,0,None]+u[faces]*st[...,1,None]


def cube_pcf(world,normal,plane_distance,corrected,caster_gap=0):
    size=512
    near,far=.035,8.
    distance=np.linalg.norm(world,axis=-1)
    ndl=np.maximum(np.sum(-world*normal,axis=-1)/distance,0)
    bias=np.maximum(.0007,np.minimum(distance,8)*(2/size)*.22)*(1+.5*(1-ndl))
    offset=world+normal*bias[...,None]
    faces,uv=cube_project(offset)
    pixel=uv*size-.5
    first=np.floor(pixel)
    fraction=pixel-first
    visibility=np.zeros(world.shape[:-1])
    for x,y in [(0,0),(1,0),(0,1),(1,1)]:
        texel=np.clip(first+[x,y],0,size-1)
        sample_uv=(texel+.5)/size
        ray=cube_shader_ray(faces,sample_uv)
        camera_ray=cube_camera_ray(faces,sample_uv)
        assert np.allclose(ray,camera_ray,atol=1e-12),'cubemap face disagrees with raster camera'
        # Rendered perspective depth of an independent ray/plane intersection. Moving the
        # entire plane towards the light models a real nearer caster, not extra receiver bias.
        intersection=camera_ray*((plane_distance+caster_gap)/(camera_ray@normal))[...,None]
        stored_z=np.sum(intersection*cube_frames()[0][faces],axis=-1)
        stored=far/(far-near)-far*near/((far-near)*stored_z)
        reference_z=(np.sum(offset*normal,axis=-1)/(ray@normal) if corrected
                     else np.max(np.abs(offset),axis=-1))
        reference=far/(far-near)-far*near/((far-near)*np.maximum(reference_z-bias,near))
        weight=(fraction[...,0] if x else 1-fraction[...,0])*(fraction[...,1] if y else 1-fraction[...,1])
        visibility+=weight*(np.minimum(reference,.999999)<=stored)
    return visibility,bias


def run_cube():
    phase=(np.arange(64)+.5)/64
    xx,yy=np.meshgrid(phase,phase)
    phases=np.stack([xx,yy],axis=-1)
    # Centre, four edges and four corners, including biased rays that switch cube face.
    anchors=[(256,256),(0,256),(511,256),(256,0),(256,511),
             (0,0),(0,511),(511,0),(511,511)]
    f,r,u=cube_frames()
    records=[]
    for face in range(6):
        for distance in [.25,1.5,5.5]:
            for tilt in [.5,2.]:
                for anchor in anchors:
                    uv=(np.array(anchor)+phases)/512
                    center_uv=(np.array(anchor)+.5)/512
                    center=cube_camera_ray(face,center_uv)*distance
                    # Rotate the plane about its centre; always face the light, even at a
                    # cube corner. This exercises steep wall planes without back-face cases.
                    direction=normalize(center)
                    tangent=normalize(r[face]-direction*(r[face]@direction))
                    normal=normalize(-direction+tangent*tilt)
                    plane_distance=normal@center
                    rays=cube_camera_ray(face,uv)
                    world=rays*(plane_distance/(rays@normal))[...,None]
                    projected_face,projected_uv=cube_project(rays)
                    assert np.all(projected_face==face)
                    assert np.allclose(projected_uv,uv,atol=1e-12)
                    old,bias=cube_pcf(world,normal,plane_distance,False)
                    new,_=cube_pcf(world,normal,plane_distance,True)
                    assert np.allclose(new,1,atol=1e-12),'cubemap plane falsely shadows itself'
                    gap=max(.02,float(bias.max())*4)
                    blocked,_=cube_pcf(world,normal,plane_distance,True,gap)
                    assert np.allclose(blocked,0,atol=1e-12),'nearer cube caster was erased'
                    records.append({'face':face,'major_distance_m':distance,'tilt':tilt,'anchor_texel':anchor,
                        'bias_m_max':float(bias.max()),'old_visibility_min':float(old.min()),
                        'old_false_shadow_fraction':float(np.mean(old<.999999)),
                        'corrected_visibility_min':float(new.min()),'corrected_visibility_max':float(new.max()),
                        'nearer_caster_gap_m':gap,'nearer_caster_visibility_max':float(blocked.max())})
    assert max(x['old_false_shadow_fraction'] for x in records)>.5
    return records


def run():
    sun=normalize([-.5,.55,.25])
    phase=(np.arange(64)+.5)/64
    xx,yy=np.meshgrid(phase,phase)
    size=2048
    uv=(np.stack([xx,yy],axis=-1)+size*.5)/size
    records=[]
    for half,depth in [(6,119.9),(20,119.9),(60,179.9)]:
        matrix=light_matrix(sun,half,depth)
        world_texel=2*half/size
        for normal in [[0,1,0],[.2,1,.1],[-.4,1,.2],[.15,1,-.35]]:
            normal=normalize(normal)
            ndl=max(normal@sun,0)
            # Existing normal offset changes UV as well as depth. Its displacement from the
            # tilted receiver plane is normal_bias / NdotL, plus the direct depth bias.
            normal_bias=max(.0005,world_texel*(.12+.18*(1-ndl)))
            depth_bias=max(.00035,world_texel*(.10+.35*(1-ndl)))
            bias_m=normal_bias/ndl+depth_bias
            bias=bias_m/depth
            # The row-derived shader gradient must equal a separate ray/plane intersection.
            delta=np.array([.00013,-.00021])
            expected=exact_depth(matrix,normal,uv+delta)-exact_depth(matrix,normal,uv)
            assert np.max(np.abs(expected-shader_gradient(matrix,normal)@delta))<1e-12
            old=pcf(matrix,normal,uv,size,bias,False)
            new=pcf(matrix,normal,uv,size,bias,True)
            assert np.allclose(new,1,atol=1e-12), 'unoccluded plane falsely shadows itself'
            occluder_gap=max(.05,bias_m*3)
            blocked=pcf(matrix,normal,uv,size,bias,True,occluder_gap/depth)
            assert np.allclose(blocked,0,atol=1e-12), 'real nearer caster was erased'
            records.append({'half_extent_m':half,'depth_range_m':depth,'normal':normal.tolist(),
                'bias_m':bias_m,'old_visibility_min':float(old.min()),'old_visibility_max':float(old.max()),
                'old_false_shadow_fraction':float(np.mean(old<.999999)),
                'corrected_visibility_min':float(new.min()),'corrected_visibility_max':float(new.max()),
                'nearer_caster_gap_m':occluder_gap,'nearer_caster_visibility_max':float(blocked.max())})
    assert max(x['old_false_shadow_fraction'] for x in records)>.9
    cube=run_cube()
    return {'status':'PASS','scope':'CPU analytic sun and cubemap receiver-plane comparisons; not a scene render',
        'cases':len(records),'subtexel_phases_per_case':len(phase)**2,
        'cube_cases':len(cube),'cube_subtexel_phases_per_case':len(phase)**2,
        'tests':['row gradient agrees with independent ray-plane intersection',
                 'unoccluded tilted planes remain fully visible in every sampled phase',
                 'nearer caster remains fully shadowing after plane correction',
                 'all six cube face rays agree with independent raster camera bases',
                 'cube centres, edges and corners remain fully lit on steep unoccluded planes',
                 'nearer cube casters remain fully shadowing'],
        'results':records,'cube_results':cube}

if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--output',type=Path)
    args=parser.parse_args()
    result=run()
    if args.output:
        assert not args.output.exists(),'choose a collision-safe receipt path'
        args.output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({key:value for key,value in result.items() if key not in ('results','cube_results')},indent=2))
