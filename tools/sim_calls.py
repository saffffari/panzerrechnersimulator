#!/usr/bin/env python3
"""Probed function costs from a KESSEL_SESSION_LOG run: `game.sim_phases_ms.calls` (inclusive time of
the functions holding a `sim_phase::Probe`), ranked by the worst single call, with the seconds in
which the slowest calls happened.

usage: python tools/sim_calls.py <session.jsonl> [top-n]
"""
import json
import sys

rows = [json.loads(line) for line in open(sys.argv[1])]
rows = [r for r in rows if r.get('game') and (r['game'].get('sim_phases_ms') or {}).get('calls') is not None]
top = int(sys.argv[2]) if len(sys.argv) > 2 else 12
stats = {}
for r in rows:
    sp = r['game']['sim_phases_ms']
    updates = max(1, sp.get('updates', 1))
    for label, per_update in sp['calls'].items():
        s = stats.setdefault(label, {'total_ms': 0.0, 'calls': 0, 'worst': 0.0, 'worst_t': None})
        s['total_ms'] += per_update * updates
        s['calls'] += sp['calls_n'].get(label, 0)
        worst = sp['calls_max'].get(label, 0.0)
        if worst > s['worst']:
            s['worst'], s['worst_t'] = worst, r.get('t_s')
print(f"{len(rows)} seconds with probes; inclusive time (a probed callee also counts in its caller)\n")
print('| function | calls | total ms | mean ms/call | worst call ms | at t (s) |')
print('|---|---|---|---|---|---|')
for label, s in sorted(stats.items(), key=lambda kv: -kv[1]['worst']):
    mean = s['total_ms'] / max(1, s['calls'])
    t = f"{s['worst_t']:.0f}" if s['worst_t'] is not None else '-'
    print(f"| `{label}` | {s['calls']} | {s['total_ms']:.1f} | {mean:.3f} | {s['worst']:.1f} | {t} |")

# The seconds with the slowest advance_projectiles call, and every probe's worst call in them.
worst_rows = sorted(rows, key=lambda r: -r['game']['sim_phases_ms']['calls_max'].get('advance_projectiles', 0))[:top and 5]
print('\nSeconds with the slowest `advance_projectiles` call (worst single call of each probe in that second, ms):\n')
for r in worst_rows:
    mx = r['game']['sim_phases_ms']['calls_max']
    parts = ', '.join(f"{k} {v:.1f}" for k, v in sorted(mx.items(), key=lambda kv: -kv[1])[:top] if v >= 0.5)
    print(f"- t {r.get('t_s', 0):.0f} s: {parts}")
