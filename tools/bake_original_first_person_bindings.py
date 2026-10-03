#!/usr/bin/env python3
"""Retain original CC0 source skin weights for existing unmodified static poses."""
import sys,json,struct,hashlib
from pathlib import Path
from math import pi
import bpy
from mathutils import Matrix,Vector
from mathutils.kdtree import KDTree
sys.path.insert(0,str(Path(__file__).resolve().parent))
import bake_crew_soldier_variants as variants

def main():
    base=Path('assets/flavor/Characters/german-ww2-soldier-cc0')
    output=base/'animation';output.mkdir(exist_ok=True)
    for pose in ['seated','relaxed']:
        arm=variants.import_character(base/'source/LowpolySoldier.fbx')
        getattr(variants,'pose_'+pose)(arm)
        deps=bpy.context.evaluated_depsgraph_get()
        objects={o.name:o for o in bpy.context.scene.objects if o.type=='MESH'}
        evaluated={name:o.evaluated_get(deps) for name,o in objects.items()}
        points=[o.matrix_world@v.co for o in evaluated.values() for v in o.data.vertices]
        lo=Vector(tuple(min(v[i] for v in points) for i in range(3)))
        hi=Vector(tuple(max(v[i] for v in points) for i in range(3)))
        scale=1.53/(hi.z-lo.z)
        normalization=Matrix.Rotation(-pi/2,4,'X')@Matrix.Scale(scale,4)@Matrix.Translation(Vector((-(lo.x+hi.x)/2,0,-lo.z)))
        bones=list(arm.pose.bones);bone_index={b.name:i for i,b in enumerate(bones)}
        path=base/'baked'/('german_ww2_soldier_'+pose+'_static.glb')
        data=path.read_bytes();n=struct.unpack_from('<I',data,12)[0];doc=json.loads(data[20:20+n]);binary=data[28+n:]
        weights=[];max_error=0
        for ni in doc['scenes'][0]['nodes']:
            node=doc['nodes'][ni];name=node['name'].removesuffix('_Static');obj=objects[name];posed=evaluated[name]
            kd=KDTree(len(posed.data.vertices))
            for i,v in enumerate(posed.data.vertices):kd.insert(normalization@posed.matrix_world@v.co,i)
            kd.balance();groups={g.index:g.name for g in obj.vertex_groups}
            for prim in doc['meshes'][node['mesh']]['primitives']:
                accessor=doc['accessors'][prim['attributes']['POSITION']];view=doc['bufferViews'][accessor['bufferView']]
                start=view.get('byteOffset',0)+accessor.get('byteOffset',0);stride=view.get('byteStride',12)
                for i in range(accessor['count']):
                    point=Vector(struct.unpack_from('<3f',binary,start+i*stride));_,idx,error=kd.find(point)
                    max_error=max(max_error,error);assert error<1e-5,(pose,name,error)
                    influence=[(bone_index[groups[g.group]],g.weight) for g in obj.data.vertices[idx].groups if g.weight>0 and groups[g.group] in bone_index]
                    total=sum(w for _,w in influence);assert total>0
                    weights.append([[j,w/total] for j,w in influence])
        def cols(m):return [m[r][c] for c in range(4) for r in range(4)]
        result={'schema':'panzer.source-original-first-person-bindings.v1','source_fbx_sha256':hashlib.sha256((base/'source/LowpolySoldier.fbx').read_bytes()).hexdigest(),
            'neutral_mesh_sha256':hashlib.sha256(data).hexdigest(),'pose':pose,'vertex_count':len(weights),'max_neutral_match_error_m':max_error,
            'bones':[{'name':b.name,'parent':bone_index[b.parent.name] if b.parent else None,'matrix':cols(normalization@arm.matrix_world@b.matrix)} for b in bones],
            'weights':weights}
        (output/(pose+'_first_person_bindings.v1.json')).write_text(json.dumps(result,separators=(',',':'))+'\n')
        print('ORIGINAL_BINDINGS',pose,len(weights),len(bones),max_error,[b.name for b in bones])
main()
