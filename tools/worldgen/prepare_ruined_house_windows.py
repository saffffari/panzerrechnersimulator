#!/usr/bin/env python3
"""Private Pizza derivative: remove only the opaque window infill, keeping frame and sill.

The purchased GLB is read-only. Geometry buffers, textures and retained primitives keep their
original bytes. A receipt records the exact removed source primitive and both file hashes.
"""
import hashlib
import json
from pathlib import Path
import struct

ROOT = Path('/home/alex/panzer/tank_simulator')
source = next((ROOT/'assets/pizza/_extracted').rglob('window_hr_1.glb'))
output = ROOT/'assets/authoring/pizza_variants/window_hr_1_open.glb'
raw = source.read_bytes()
assert struct.unpack_from('<4sII',raw) == (b'glTF',2,len(raw))
length,kind = struct.unpack_from('<II',raw,12)
assert kind == 0x4e4f534a
doc = json.loads(raw[20:20+length])
removed=[]
for mesh in doc['meshes']:
    retained=[]
    for primitive in mesh['primitives']:
        name = doc['materials'][primitive['material']]['name']
        if name == 'window_hm_2':
            removed.append({'material':name,'position_accessor':primitive['attributes']['POSITION'],
                            'vertices':doc['accessors'][primitive['attributes']['POSITION']]['count']})
        else:
            retained.append(primitive)
    assert retained
    mesh['primitives']=retained
assert len(removed)==1 and removed[0]['vertices']==8
blob=json.dumps(doc,separators=(',',':')).encode()
blob += b' '*((-len(blob))%4)
tail=raw[20+length:]
result=struct.pack('<4sII',b'glTF',2,20+len(blob)+len(tail))+struct.pack('<II',len(blob),kind)+blob+tail
output.parent.mkdir(parents=True,exist_ok=True)
if output.exists():
    assert output.read_bytes()==result, 'Existing derivative differs; preserve it'
else:
    output.write_bytes(result)
receipt={'source':source.relative_to(ROOT).as_posix(),'source_sha256':hashlib.sha256(raw).hexdigest(),
         'output':output.relative_to(ROOT).as_posix(),'output_sha256':hashlib.sha256(result).hexdigest(),
         'removed':removed,'retained_binary_chunks_identical':True,'license':'pizzadoggy-local-purchase',
         'distribution':'owner-private game authoring derivative'}
output.with_suffix('.receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
