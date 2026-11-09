"""Hex and square heads model-wide: every one read by the full-CAD census becomes a bolt object
(the owner, 2026-11-08: "lot of screws on this model that will need to be properly integrated as
screw objects"). Round heads are left moulded in until each part is judged rivet or screw: the
census file lists them per part.

    import fasten_all; fasten_all.run(census_path, first=0, budget_s=35)   # repeat with the returned index

A part already carrying fastener objects is skipped. The set is the owning control's name in
lower case, or '<kit number>_bolts' where no control owns the part.
"""
import json
import re
import time

import bpy

import fasteners

PRE = 'TG_RM5010-'


def _set_of(o, kit):
    p = o
    while p is not None:
        if p.name.startswith('CTRL.'):
            return '_'.join(t if re.fullmatch(r'[LR]\d*', t) else t.lower() for t in p.name[5:].split('_')) + '_bolts'
        p = p.parent
    return kit.split('_')[0] + '_bolts'


def run(census_path, first=0, budget_s=35.0):
    data = json.load(open(census_path))
    kits = sorted(v['object'].replace(PRE, '').split('_îAÔO')[0] for v in data.values() if any(r['shape'] in ('hex', 'square') for r in v['rows']))
    began = time.time()
    done, i = [], first
    kind_of = lambda r: ('bolt', True) if r['shape'] in ('hex', 'square') and 0.006 <= r['size'] <= 0.05 else None
    while i < len(kits) and time.time() - began < budget_s:
        kit = kits[i]
        rows = next(v['rows'] for v in data.values() if v['object'].replace(PRE, '').split('_îAÔO')[0] == kit)
        objs = sorted([o for o in bpy.data.objects if o.type == 'MESH' and o.name.split('_îAÔO')[0] == PRE + kit], key=lambda o: o.name)
        cut, made, bad = set(), 0, 0
        for o in objs:
            if any(c.name.startswith('FASTENER.') for c in o.children) or o.get('split_from') or o.get('split_into'):
                continue
            rep = fasteners.apply_rows(o, rows, kind_of, set_name=_set_of(o, kit), cut=o.data.name not in cut)
            cut.add(o.data.name)
            made += rep['objects_made']
            bad += rep.get('ring_open', 0)
        done.append((kit, len(objs), made, bad))
        i += 1
    return {'next': i, 'count': len(kits), 'done': done}
