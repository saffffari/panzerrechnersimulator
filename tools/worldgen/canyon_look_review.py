#!/usr/bin/env python3
"""Saved Canyon camera treatment and heavy weather; preserve prior study workshops."""
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[2]
DISPLAY = dict(width=640, height=360, palette_levels=23, ordered_dither=1.0,
               retro_strength=1.0, ldr_dither=1.0, ambient_white_balance=True)


def apply_look(edits, utc='1943-01-20T13:20:00Z'):
    edits['display_tuning'] = dict(DISPLAY)
    edits['environment_lighting'].update(utc=utc, fog_density=.0046, overcast=.98, storm=.58)


if __name__ == '__main__':
    source = ROOT/'assets/worldgen/workshops/gaea-canyon-run-v2'
    for name, utc in [('day','1943-01-20T06:35:00Z'), ('snowfall','1943-01-20T13:20:00Z')]:
        dest = source.with_name('gaea-canyon-look-'+name)
        dest.mkdir(exist_ok=True)
        for filename in ['manifest.v1.json','heights.npy']:
            shutil.copyfile(source/filename,dest/filename)
        edits = json.loads((source/'manifest_edits.json').read_text())
        apply_look(edits,utc)
        (dest/'manifest_edits.json').write_text(json.dumps(edits,indent=2)+'\n')
        print(dest)
