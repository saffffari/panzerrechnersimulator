#!/usr/bin/env python3
"""Isolated weather studies on the current Gaea v2 geometry; no source map mutation."""
import hashlib
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/worldgen/workshops/gaea-canyon-run-v2'
PROFILES = {
    'day': '1943-01-20T06:35:00Z',
    'dusk': '1943-01-20T12:45:00Z',
    'bluehour': '1943-01-20T14:15:00Z',
    'snowfall': '1943-01-20T13:20:00Z',
}


def make_studies():
    receipts = []
    for name, utc in PROFILES.items():
        dst = SOURCE.with_name('gaea-canyon-storm-' + name)
        dst.mkdir(exist_ok=True)
        for filename in ['manifest.v1.json', 'heights.npy']:
            shutil.copy2(SOURCE / filename, dst / filename)
        edits = json.loads((SOURCE / 'manifest_edits.json').read_text())
        edits['environment_lighting'].update(fog_density=.0046, overcast=.98, storm=.58, utc=utc)
        (dst / 'manifest_edits.json').write_text(json.dumps(edits, indent=2) + '\n')
        receipts.append(dict(name=name, source=str(SOURCE), path=str(dst),
            lighting=edits['environment_lighting'],
            sha256={p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in [dst/'manifest.v1.json', dst/'heights.npy', dst/'manifest_edits.json']}))
    return receipts


if __name__ == '__main__':
    print(json.dumps(make_studies(), indent=2))
