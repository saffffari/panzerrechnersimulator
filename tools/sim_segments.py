#!/usr/bin/env python3
"""Per-segment simulation cost from a KESSEL_SESSION_LOG run: `game.sim_phases_ms.seg` (the
`sim_phase` checkpoints over the whole `Game::update`), ms per update averaged over seconds and
the worst single update, split by the nearest enemy's range and by the crew station.

usage: python tools/sim_segments.py <session.jsonl>
"""
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1])]
rows = [r for r in rows if r.get('game') and (r['game'].get('sim_phases_ms') or {}).get('seg')]
def nearest(r):
    en = ((r['game'].get('combat') or {}).get('enemies') or [])
    d = [e.get('range_m') for e in en if e.get('alive', True) and e.get('range_m') is not None]
    return min(d) if d else 1e9
def band(r):
    d = nearest(r)
    return 'enemy < 150 m' if d < 150 else 'enemy 150-420 m' if d < 420 else 'enemy > 420 m'
groups = {}
for r in rows:
    for key in ('all', band(r), 'station ' + str(r['game'].get('station'))):
        groups.setdefault(key, []).append(r)
labels = []
for r in rows:
    for k in r['game']['sim_phases_ms']['seg']:
        if k not in labels: labels.append(k)
print(f"{len(rows)} seconds with segment timing; ms per update (mean over seconds) / worst single update ms\n")
order = ['all', 'enemy > 420 m', 'enemy 150-420 m', 'enemy < 150 m'] + sorted(k for k in groups if k.startswith('station'))
hdr = '| segment | ' + ' | '.join(f"{g} ({len(groups[g])} s)" for g in order if g in groups) + ' |'
print(hdr); print('|' + '---|' * (hdr.count('|') - 1))
tot = {}
for lab in labels:
    cells = []
    for g in order:
        if g not in groups: continue
        rs = groups[g]
        mean = sum(r['game']['sim_phases_ms']['seg'].get(lab, 0) for r in rs) / len(rs)
        mx = max(r['game']['sim_phases_ms']['seg_max'].get(lab, 0) for r in rs)
        tot[g] = tot.get(g, 0) + mean
        cells.append(f"{mean:.2f} / {mx:.0f}")
    print(f"| {lab} | " + ' | '.join(cells) + ' |')
print('| **sum** | ' + ' | '.join(f"{tot[g]:.2f}" for g in order if g in groups) + ' |')
