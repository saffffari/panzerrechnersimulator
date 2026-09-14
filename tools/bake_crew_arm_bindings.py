#!/usr/bin/env python3
"""Export original seated-pose skin bindings, without changing any reviewed runtime geometry.
Run in Blender: --python tools/bake_crew_arm_bindings.py -- REVIEWED_EXTRACTION OUTPUT.json
The metadata maps each existing GLB split vertex to its exact evaluated source vertex; it preserves
all original influences, not nearest-bone weights. A changed mesh/pose rejects the bake.
"""
import hashlib
import json
import struct
import sys
from pathlib import Path
from math import pi
import bpy
from mathutils import Matrix, Vector
sys.path.insert(0, str(Path(__file__).resolve().parent))
import bake_tiger_crew_kit as source


def glb_positions(path):
    data=path.read_bytes(); size=struct.unpack_from('<I',data,12)[0]
    doc=json.loads(data[20:20+size]); binary=data[28+size:]
    def accessor(index):
        a=doc['accessors'][index];v=doc['bufferViews'][a['bufferView']]
        assert a['componentType']==5126 and a['type']=='VEC3'
        start=v.get('byteOffset',0)+a.get('byteOffset',0);stride=v.get('byteStride',12)
        return [Vector(struct.unpack_from('<3f',binary,start+i*stride)) for i in range(a['count'])]
    result=[]
    for node_index in doc['scenes'][0]['nodes']:
        node=doc['nodes'][node_index]
        assert not any(k in node for k in ['matrix','translation','rotation','scale'])
        for primitive in doc['meshes'][node['mesh']]['primitives']:
            result.append((node['name'].removesuffix('_Static'),accessor(primitive['attributes']['POSITION'])))
    return result


def main():
    args=sys.argv[sys.argv.index('--')+1:]; extraction=Path(args[0]);output=Path(args[1])
    source.verify_extraction(extraction)
    source.import_source(extraction/source.SOURCE_FBX_RELATIVE)
    source.validate_source_scene()
    scale=source.character_rest_normalization()['uniform_scale']
    armature=bpy.data.objects[source.ARMATURE_NAME]
    source.reset_and_apply_pose(armature,'seated')
    deps=bpy.context.evaluated_depsgraph_get()
    evaluated={name:bpy.data.objects[name].evaluated_get(deps) for name in source.CHARACTER_NAMES}
    points=[obj.matrix_world@v.co*scale for obj in evaluated.values() for v in obj.data.vertices]
    lo=Vector(tuple(min(v[i] for v in points) for i in range(3)))
    hi=Vector(tuple(max(v[i] for v in points) for i in range(3)))
    normalization=Matrix.Rotation(-pi/2,4,'X')@Matrix.Translation(Vector((-(lo.x+hi.x)/2,0,-lo.z)))@Matrix.Scale(scale,4)
    bones=list(armature.pose.bones);bone_index={b.name:i for i,b in enumerate(bones)}
    bindings=[]; max_error=0
    base=Path('assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/baked/german_panzertruppe_face01_seated_static.glb')
    for name, split_vertices in glb_positions(base):
        original=bpy.data.objects[name];posed=evaluated[name]
        positions=[normalization@posed.matrix_world@v.co for v in posed.data.vertices]
        group_names={group.index:group.name for group in original.vertex_groups}
        for point in split_vertices:
            errors=[(point-v).length for v in positions];idx=min(range(len(errors)),key=errors.__getitem__)
            max_error=max(max_error,errors[idx]);assert errors[idx]<0.00001,(name,errors[idx])
            influences=[(bone_index[group_names[g.group]],g.weight) for g in original.data.vertices[idx].groups if g.weight>0 and group_names[g.group] in bone_index]
            assert influences and len(influences)<=8
            total=sum(weight for _,weight in influences)
            bindings.append([[joint,weight/total] for joint,weight in influences])
    def matrix_cols(matrix):return [matrix[row][col] for col in range(4) for row in range(4)]
    output.parent.mkdir(parents=True,exist_ok=True)
    result={'schema':'panzer.source-crew-arm-bindings.v1','source_fbx_sha256':source.recipe.SOURCE_FBX_SHA256,
        'neutral_mesh_sha256':hashlib.sha256(base.read_bytes()).hexdigest(),
        'pose':'seated','vertex_count':len(bindings),'max_neutral_match_error_m':max_error,
        'bones':[{'name':b.name,'parent':bone_index[b.parent.name] if b.parent else None,
            'matrix':matrix_cols(normalization@armature.matrix_world@b.matrix)} for b in bones],
        'weights':bindings}
    output.write_text(json.dumps(result,separators=(',',':'))+'\n')
    print('CREW_BINDINGS',len(bindings),'vertices',len(bones),'bones','max_error_m',max_error)

main()
