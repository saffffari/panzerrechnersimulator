#!/usr/bin/env python3
"""What lies on the road: tabulate a strewn-field census by source kind, group and rule.

The engine writes the census (one row per placed object: position, distance to the route
centreline, source kind, rule, group, tier) when `KESSEL_SCATTER_CENSUS=FILE` is set for any run
that builds the field (docs/SCATTER.md section 1). This tool counts the rows that lie within the
road's half width and within a wider corridor of the route, for the whole route and for a stretch
of it given in route metres, and prints the tables.

    python tools/worldgen/scatter_road_census.py CENSUS.csv SCATTER.json [--stretch M0 M1 [NAME]]...
                                                 [--corridor 10] [--rules 14] [--markdown]

SCATTER.json is the layer file the census was built from (it supplies the route and the road's
half width). Route metres run from the first road segment. A row belongs to a stretch when the
nearest point of the route to it lies in the stretch.
"""
import argparse
import collections
import csv
import json
import math
from pathlib import Path

import numpy as np


def route_of(doc):
    a, b = [], []
    for s in doc['sources']:
        if s['kind'] != 'road':
            continue
        h = math.radians(s['heading_deg'])
        f = np.array([math.sin(h), math.cos(h)]) * s['half_length']
        c = np.array([s['x'], s['z']])
        a.append(c - f)
        b.append(c + f)
    a, b = np.array(a), np.array(b)
    length = np.linalg.norm(b - a, axis=1)
    return a, b, length, np.concatenate([[0.0], np.cumsum(length)])


def route_metre(points, a, b, length, cum, chunk=4096):
    """Route metre of the nearest route point to each (x, z)."""
    d = b - a
    out = np.empty(len(points))
    for i in range(0, len(points), chunk):
        p = points[i:i + chunk, None, :]
        t = np.clip(((p - a[None]) * d[None]).sum(2) / (length * length)[None], 0.0, 1.0)
        q = a[None] + d[None] * t[..., None]
        dist = np.linalg.norm(q - p, axis=2)
        k = dist.argmin(1)
        out[i:i + chunk] = cum[k] + t[np.arange(len(k)), k] * length[k]
    return out


def table(title, header, rows, markdown):
    print()
    if markdown:
        print(f'**{title}**\n')
        print('| ' + ' | '.join(header) + ' |')
        print('|' + '|'.join(['---'] + ['---:'] * (len(header) - 1)) + '|')
        for r in rows:
            print('| ' + ' | '.join(str(c) for c in r) + ' |')
        return
    print(title)
    widths = [max(len(str(r[i])) for r in [header] + rows) for i in range(len(header))]
    for r in [header] + rows:
        print('  ' + '  '.join(str(c).ljust(widths[i]) if i == 0 else str(c).rjust(widths[i]) for i, c in enumerate(r)))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('census', type=Path)
    ap.add_argument('scatter', type=Path)
    ap.add_argument('--stretch', nargs='+', action='append', default=[], metavar='M0 M1 [NAME]')
    ap.add_argument('--corridor', type=float, default=10.0, help='corridor half width, metres (default 10)')
    ap.add_argument('--rules', type=int, default=14, help='how many rules to list per table')
    ap.add_argument('--markdown', action='store_true')
    args = ap.parse_args()

    doc = json.loads(args.scatter.read_text())
    road_half = float(doc.get('road_half_m', 4.2))
    a, b, length, cum = route_of(doc)
    xs, zs, lane, kinds, rules, groups, tiers = [], [], [], [], [], [], []
    with args.census.open(newline='') as fh:
        for row in csv.DictReader(fh):
            d = float(row['lane_m'])
            if d < 0.0 or d > args.corridor:
                continue
            xs.append(float(row['x']))
            zs.append(float(row['z']))
            lane.append(d)
            kinds.append(row['kind'])
            rules.append(row['rule'])
            groups.append(row['group'])
            tiers.append(row['tier'])
    lane = np.array(lane)
    metre = route_metre(np.stack([xs, zs], 1), a, b, length, cum)
    kinds, rules, groups, tiers = (np.array(v) for v in (kinds, rules, groups, tiers))
    stretches = [('whole route', 0.0, float(cum[-1]))]
    for s in args.stretch:
        stretches.append((s[2] if len(s) > 2 else f'route {s[0]}-{s[1]} m', float(s[0]), float(s[1])))
    print(f'census {args.census} of {args.scatter}: route {cum[-1]:.0f} m, road half width {road_half} m, '
          f'corridor half width {args.corridor} m')
    for name, m0, m1 in stretches:
        span = m1 - m0
        sel = (metre >= m0) & (metre <= m1)
        on = sel & (lane < road_half)
        cor = sel
        road_area, cor_area = span * 2 * road_half, span * 2 * args.corridor
        print(f'\n=== {name}: route metres {m0:.0f} to {m1:.0f} ({span:.0f} m) ===')
        print(f'on the road (within {road_half} m of the centreline): {on.sum()} pieces, {on.sum() / road_area:.3f} per m2, '
              f'{on.sum() / span:.2f} per metre of road')
        print(f'in the corridor (within {args.corridor} m): {cor.sum()} pieces, {cor.sum() / cor_area:.3f} per m2')
        heavy = on & (tiers != 'small')
        print(f'medium or large tier on the road: {heavy.sum()}; in the corridor: {(cor & (tiers != "small")).sum()}')
        for label, column in (('source kind', kinds), ('group', groups), ('rule', rules)):
            c_on, c_cor = collections.Counter(column[on]), collections.Counter(column[cor])
            keys = sorted(c_cor, key=lambda k: (-c_on.get(k, 0), -c_cor[k]))
            if label == 'rule':
                keys = keys[:args.rules]
            rows = [[k, c_on.get(k, 0), f'{c_on.get(k, 0) / road_area:.3f}', c_cor[k], f'{c_cor[k] / cor_area:.3f}'] for k in keys]
            rows.append(['total', int(on.sum()), f'{on.sum() / road_area:.3f}', int(cor.sum()), f'{cor.sum() / cor_area:.3f}'])
            table(f'{name}, by {label}' + (f' (first {args.rules} by count on the road)' if label == 'rule' else ''),
                  [label, f'on road (<{road_half} m)', 'per m2', f'corridor (<{args.corridor:g} m)', 'per m2'], rows, args.markdown)


if __name__ == '__main__':
    main()
