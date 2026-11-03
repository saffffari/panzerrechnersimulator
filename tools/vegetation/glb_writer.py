"""A small dependency-free GLB writer for constant-material meshes (numpy only).

Coordinates are written as given: the generator builds in the engine's frame directly (Y up,
metres), which is what Blender's default glTF Y-up export produces from a Z-up scene
(engine = (blender_x, blender_z, -blender_y)), so these files load exactly like the Blender bakes.
"""
import json
import struct

import numpy as np

FLOAT, UINT32 = 5126, 5125
ARRAY_BUFFER, ELEMENT_ARRAY_BUFFER = 34962, 34963


def write_glb(path, name, primitives, materials, extras=None):
    """primitives: [(material_index, positions (N,3), normals (N,3), indices (M,))];
    materials: [dict(name, base (r,g,b) linear, roughness, metallic)]."""
    blob = bytearray()
    views, accessors, prims = [], [], []

    def add(array, target, comp, kind, bounds=False):
        while len(blob) % 4:
            blob.append(0)
        offset = len(blob)
        blob.extend(array.tobytes())
        views.append(dict(buffer=0, byteOffset=offset, byteLength=array.nbytes, target=target))
        acc = dict(bufferView=len(views) - 1, componentType=comp, count=int(array.shape[0]), type=kind)
        if bounds:
            acc['min'] = [float(v) for v in array.min(axis=0)]
            acc['max'] = [float(v) for v in array.max(axis=0)]
        accessors.append(acc)
        return len(accessors) - 1

    for material, pos, nrm, idx in primitives:
        if len(idx) == 0:
            continue
        pos = np.ascontiguousarray(pos, dtype=np.float32)
        nrm = np.ascontiguousarray(nrm, dtype=np.float32)
        idx = np.ascontiguousarray(idx, dtype=np.uint32)
        p = add(pos, ARRAY_BUFFER, FLOAT, 'VEC3', bounds=True)
        n = add(nrm, ARRAY_BUFFER, FLOAT, 'VEC3')
        i = add(idx, ELEMENT_ARRAY_BUFFER, UINT32, 'SCALAR')
        prims.append(dict(attributes=dict(POSITION=p, NORMAL=n), indices=i, material=material))
    doc = dict(
        asset=dict(version='2.0', generator='tools/vegetation/spring1943.py'),
        scene=0,
        scenes=[dict(nodes=[0])],
        nodes=[dict(name=name, mesh=0)],
        meshes=[dict(name=name, primitives=prims)],
        materials=[dict(name=m['name'], doubleSided=True,
                        pbrMetallicRoughness=dict(baseColorFactor=[round(float(c), 5) for c in m['base']] + [1.0],
                                                  metallicFactor=float(m.get('metallic', 0.0)),
                                                  roughnessFactor=float(m['roughness'])))
                   for m in materials],
        accessors=accessors,
        bufferViews=views,
        buffers=[dict(byteLength=len(blob))],
    )
    if extras:
        doc['nodes'][0]['extras'] = extras
    while len(blob) % 4:
        blob.append(0)
    js = json.dumps(doc, separators=(',', ':')).encode()
    js += b' ' * ((4 - len(js) % 4) % 4)
    total = 12 + 8 + len(js) + 8 + len(blob)
    with open(path, 'wb') as f:
        f.write(struct.pack('<III', 0x46546C67, 2, total))
        f.write(struct.pack('<II', len(js), 0x4E4F534A))
        f.write(js)
        f.write(struct.pack('<II', len(blob), 0x004E4942))
        f.write(bytes(blob))
