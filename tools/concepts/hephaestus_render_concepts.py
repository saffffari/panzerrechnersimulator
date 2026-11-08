"""Geometry-derived standalone concept renders; never opens or saves a user .blend.

blender --background --factory-startup --threads 4 --python \
    tools/concepts/hephaestus_render_concepts.py -- --scene all --samples 16

Only writes PNGs in the owner's flat screenshot folder and an asset validation
receipt in the Hephaestus concept folder. No runtime GPU or engine dependencies.
"""
import argparse
import hashlib
import json
import math
import pathlib
import sys
import time

import bpy
from mathutils import Vector

ROOT=pathlib.Path(__file__).resolve().parents[2]
ASSETS=ROOT/'assets/concepts/primary-accounts-20261107/hephaestus'
CAPTURES=pathlib.Path('/windows/shop/panzerrechnersystem/captures/screenshots')


def aim(obj,target):
    obj.rotation_euler=(Vector(target)-obj.location).to_track_quat('-Z','Y').to_euler()


def render(name,samples,detail=False):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    source=ASSETS/(name+'.voxels.json')
    data=json.loads(source.read_text())
    p=data['preview']
    started=time.monotonic()
    bpy.ops.import_scene.gltf(filepath=str(ASSETS/(name+'.glb')))
    imported=list(bpy.context.scene.objects)
    mesh_count=sum(o.type=='MESH' for o in imported)
    tris=sum(len(o.data.polygons) for o in imported if o.type=='MESH')
    assert mesh_count==len(data['parts']), (mesh_count,len(data['parts']))
    scene=bpy.context.scene
    scene.render.engine='CYCLES'
    scene.cycles.device='CPU'
    scene.cycles.samples=samples
    scene.cycles.use_adaptive_sampling=True
    scene.cycles.adaptive_threshold=.04
    scene.cycles.use_denoising=True
    scene.cycles.max_bounces=5
    scene.cycles.diffuse_bounces=3
    scene.cycles.glossy_bounces=2
    scene.render.threads_mode='FIXED'
    scene.render.threads=4
    scene.render.resolution_x=1600 if not detail else 1440
    scene.render.resolution_y=1280 if not detail else 1080
    scene.render.resolution_percentage=100
    scene.render.image_settings.file_format='PNG'
    scene.render.image_settings.color_mode='RGBA'
    scene.render.film_transparent=False
    scene.world=bpy.data.worlds.new('cold_studio_world')
    scene.world.use_nodes=True
    bg=scene.world.node_tree.nodes.get('Background')
    bg.inputs['Color'].default_value=(*p['world'],1)
    bg.inputs['Strength'].default_value=.52
    scene.view_settings.view_transform='AgX'
    scene.view_settings.look='AgX - Medium High Contrast'
    scene.view_settings.exposure=1.15 if name=='factory-cellar-ward' else .85
    scene.view_settings.gamma=1
    # Orthographic camera: documentation of actual voxel geometry, no painted backdrop.
    bpy.ops.object.camera_add(location=p['camera'])
    cam=bpy.context.object
    cam.name='concept_inspection_camera'
    cam.data.type='ORTHO'
    cam.data.ortho_scale=p['ortho']
    aim(cam,p['target']); scene.camera=cam
    if detail and name=='factory-cellar-ward':
        cam.location=(15,-10,12)
        cam.data.ortho_scale=12.9
        aim(cam,(6.7,6.8,2.2))
    if detail and name=='no-departure-airfield':
        cam.location=(25,-18,21)
        cam.data.ortho_scale=24
        aim(cam,(12.6,11.2,1.6))
    ld=bpy.data.lights.new('cold_open_sky','AREA')
    ld.energy=p['key_power']; ld.color=p['key_color']; ld.shape='DISK';ld.size=12
    lo=bpy.data.objects.new(ld.name,ld);scene.collection.objects.link(lo);lo.location=p['key'];aim(lo,p['target'])
    # Soft rim separates the cut rubble from the quiet dark background.
    ld=bpy.data.lights.new('sky_rim','AREA');ld.energy=1100;ld.color=(.40,.58,.85);ld.size=10
    lo=bpy.data.objects.new(ld.name,ld);scene.collection.objects.link(lo)
    lo.location=(p['target'][0]+5,p['target'][1]+15,16);aim(lo,p['target'])
    for i,q in enumerate(p['warm_lights']):
        ld=bpy.data.lights.new(f'floor_fire_light_{i}','POINT');ld.energy=q['power'];ld.color=(1,.27,.045);ld.shadow_soft_size=q['size']
        lo=bpy.data.objects.new(ld.name,ld);scene.collection.objects.link(lo);lo.location=q['location']
    # A render-only studio surface receives contact shadow. It is absent from the assets.
    bpy.ops.mesh.primitive_plane_add(size=200,location=(0,0,-.035))
    plane=bpy.context.object;plane.name='render_only_studio_floor'
    mat=bpy.data.materials.new('render_only_slate');mat.diffuse_color=(.012,.018,.028,1);mat.use_nodes=True
    bsdf=mat.node_tree.nodes.get('Principled BSDF');bsdf.inputs['Base Color'].default_value=(.012,.018,.028,1);bsdf.inputs['Roughness'].default_value=.93
    plane.data.materials.append(mat)
    suffix='detail' if detail else 'overview'
    destination=CAPTURES/f'voxel-runtime-concept-hephaestus-{name}-{suffix}-20261107.png'
    scene.render.filepath=str(destination)
    bpy.ops.render.render(write_still=True)
    receipt={'asset':name,'preview':str(destination),'renderer':bpy.app.version_string,
             'engine':'Cycles CPU','threads':4,'samples':samples,'resolution':[scene.render.resolution_x,scene.render.resolution_y],
             'imported_semantic_meshes':mesh_count,'imported_triangles':tris,
             'elapsed_import_and_render_s':round(time.monotonic()-started,3),
             'geometry_source':str(ASSETS/(name+'.glb')),
             'geometry_sha256':hashlib.sha256((ASSETS/(name+'.glb')).read_bytes()).hexdigest(),
             'preview_sha256':hashlib.sha256(destination.read_bytes()).hexdigest(),
             'render_only_geometry':['studio floor'],
             'ai_visual_review':'pending image inspection','human_review':'not performed'}
    (ASSETS/(name+'.'+suffix+'.render-receipt.json')).write_text(json.dumps(receipt,indent=2)+'\n')
    print('HEPHAESTUS_RENDER_RECEIPT '+json.dumps(receipt),flush=True)


args=sys.argv[sys.argv.index('--')+1:] if '--' in sys.argv else []
parser=argparse.ArgumentParser();parser.add_argument('--scene',choices=['all','cellar','airfield'],default='all')
parser.add_argument('--samples',type=int,default=16);parser.add_argument('--detail',action='store_true')
opt=parser.parse_args(args)
if opt.scene in ('all','cellar'):render('factory-cellar-ward',opt.samples,opt.detail)
if opt.scene in ('all','airfield'):render('no-departure-airfield',opt.samples,opt.detail)
