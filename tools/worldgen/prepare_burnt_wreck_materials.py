#!/usr/bin/env python3
"""Author private soot-darkened GLB material variants; keep original and detail meshes intact."""
from pathlib import Path
import hashlib
import json
import struct

DEST = Path('/home/alex/panzer/tank_simulator/assets/authoring/battlefield')


def main():
    receipt = DEST / 'burnt-material.receipt.json'
    outputs = [DEST / f'{name}_wreck_burnt.glb' for name in ('kv1', 'pz3')] + [receipt]
    if any(path.exists() for path in outputs):
        raise SystemExit('Preserving existing variants/receipt; choose a new revision to regenerate.')
    receipts = []
    for name, body in [('kv1', [.065,.076,.057,1]), ('pz3', [.065,.059,.050,1])]:
        source = DEST / f'{name}_wreck_detail.glb'
        target = DEST / f'{name}_wreck_burnt.glb'
        if target.exists():
            raise SystemExit(f'Preserving existing {target}; choose a new revision to regenerate.')
        original = source.read_bytes()
        assert original[:4] == b'glTF' and struct.unpack_from('<I', original, 4)[0] == 2
        length, kind = struct.unpack_from('<II', original, 12)
        assert kind == 0x4E4F534A
        data = json.loads(original[20:20+length])
        for i, material in enumerate(data.get('materials', [])):
            pbr = material.setdefault('pbrMetallicRoughness', {})
            pbr.update(baseColorFactor=body if i == 0 else [.032,.028,.024,1],
                       metallicFactor=.25, roughnessFactor=.94)
        encoded = json.dumps(data, separators=(',', ':')).encode()
        encoded += b' ' * (-len(encoded) % 4)
        binary_chunks = original[20+length:]
        output = struct.pack('<4sII', b'glTF', 2, 20+len(encoded)+len(binary_chunks))
        output += struct.pack('<II', len(encoded), kind) + encoded + binary_chunks
        target.write_bytes(output)
        receipts.append({'source':source.name, 'derived':target.name,
            'source_sha256':hashlib.sha256(original).hexdigest(),
            'sha256':hashlib.sha256(output).hexdigest(),
            'operation':'Authored soot-darkened rough material factors; geometry unchanged; source contained no texture images.'})
    receipt.write_text(json.dumps(receipts, indent=2)+'\n')


if __name__ == '__main__':
    main()
