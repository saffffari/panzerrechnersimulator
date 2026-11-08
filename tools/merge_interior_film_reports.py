#!/usr/bin/env python3
"""Combine a finite set of offline render workers in the original camera-plan order."""
from pathlib import Path
import argparse
import json

p = argparse.ArgumentParser()
p.add_argument('run', type=Path)
args = p.parse_args()
plan = json.loads((args.run / 'plan.json').read_text())
names = [job['name'] for job in plan['jobs']]
assert len(set(names)) == len(names)
clips = {}
report = None
for path in sorted(args.run.glob('worker-*/report.json')):
    worker = json.loads(path.read_text())
    if report is None:
        report = {key: value for key, value in worker.items() if key != 'clips'}
    for key in ('fps', 'internal_pixels', 'output_scale', 'samples_per_frame'):
        assert worker[key] == report[key], (path, key)
    for clip in worker['clips']:
        assert clip['complete'] and clip['name'] in names and clip['name'] not in clips
        clips[clip['name']] = clip
assert report is not None, 'No worker receipt exists yet'
report['clips'] = [clips[name] for name in names if name in clips]
report['complete'] = len(clips) == len(names)
report['workers'] = [str(path) for path in sorted(args.run.glob('worker-*/report.json'))]
temp = args.run / 'report.json.tmp'
temp.write_text(json.dumps(report, indent=2))
temp.replace(args.run / 'report.json')
print(f'{len(clips)} / {len(names)} clips complete')
