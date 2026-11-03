#!/usr/bin/env python3
"""Build every Berlin 1945 landmark model (headless Blender) into the asset root.

    KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem python3 tools/berlin1945/build_models.py \
        [--only NAME[,NAME]] [--preview DIR]

Each model script writes <name>.glb and <name>.colliders.json into
$KESSEL_ASSET_ROOT/assets/worlds/berlin1945/ (gitignored). --preview DIR also renders Workbench
stills of each model for checking it against the reference photographs (author's use only).
"""
import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
MODELS = ['model_reichstag', 'model_brandenburg_gate', 'model_landmarks', 'model_city_blocks', 'model_props']


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--only')
    ap.add_argument('--preview', type=Path)
    ap.add_argument('--out', type=Path)
    a = ap.parse_args()
    root = Path(os.environ.get('KESSEL_ASSET_ROOT', '.'))
    out = a.out or root/'assets/worlds/berlin1945'
    out.mkdir(parents=True, exist_ok=True)
    names = a.only.split(',') if a.only else MODELS
    reports = []
    for name in names:
        script = HERE/f'{name}.py'
        if not script.exists():
            print(f'[build_models] skip {name}: no script')
            continue
        cmd = ['nice', '-n', '10', 'blender', '--background', '--factory-startup', '--python', str(script), '--', str(out)]
        if a.preview:
            a.preview.mkdir(parents=True, exist_ok=True)
            cmd.append(str(a.preview/name))
        r = subprocess.run(cmd, capture_output=True, text=True)
        lines = [l for l in r.stdout.splitlines()+r.stderr.splitlines()
                 if l.startswith('[berlin1945]') or 'Error' in l or 'Traceback' in l or l.startswith('  File')]
        print('\n'.join(lines))
        if r.returncode != 0 or any('Traceback' in l for l in lines):
            sys.exit(f'[build_models] {name} failed')
        reports += [json.loads(l.split(' ', 1)[1]) for l in lines if l.startswith('[berlin1945] {')]
    (out/'models.receipt.json').write_text(json.dumps(reports, indent=1)+'\n')


if __name__ == '__main__':
    main()
