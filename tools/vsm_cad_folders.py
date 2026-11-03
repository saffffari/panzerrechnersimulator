#!/usr/bin/env python3
"""The vehicle surface model's CAD evidence (docs/VEHICLE_SURFACE_MODEL_20261102.md, phase 1).

Reads the damage session's registration of the Rye Field RM-5010 kit CAD against the game's Tiger
GLBs (`solid_index.json`: each STEP solid's GLB node and its SolidWorks mass candidates;
`sw_parts.json`: every SolidWorks part's assembly folder) and writes, per registered GLB node, the
assembly folder its candidates most often belong to, weighted 1/candidates, with that share as the
confidence. `src/vehicle_surface.rs` uses only the running gear's and the barrel's folders, where
the evidence is unambiguous; the rest is recorded for review.

    python3 tools/vsm_cad_folders.py /home/alex/panzer/fea-damage/assets/fea/tiger_cad \
        catalog/tiger_surface_cad_folders.v1.json
"""
import collections, json, re, sys

src, out = sys.argv[1], sys.argv[2]
si = json.load(open(f"{src}/solid_index.json"))
sp = json.load(open(f"{src}/sw_parts.json"))
folders = collections.defaultdict(set)
for p in sp["parts"]:
    stem = re.sub(r"^TG_RM5010-", "", p["file"].rsplit(".", 1)[0])
    folders[stem].add(p["folder"].split("/")[-1])
by_node = collections.defaultdict(list)
for s in si["solids"]:
    if s.get("glb"):
        by_node[s["glb"]].append(s)
nodes = {}
for node, solids in sorted(by_node.items()):
    votes = collections.Counter()
    for s in solids:
        cands = s["sw_mass_candidates"]
        for c in cands:
            for f in folders.get(c, ()):
                votes[f] += 1.0 / len(cands)
    if not votes:
        continue
    folder, w = votes.most_common(1)[0]
    nodes[node] = {"folder": folder, "share": round(w / sum(votes.values()), 3),
                   "solids": [s["id"] for s in solids]}
json.dump({
    "schema": "kessel.vsm-cad-folders.v1",
    "source": "registration of the RM-5010 kit CAD to tiger_{hull,turret,gun}.glb (damage session, 2026-11-02)",
    "method": "per GLB node: the SolidWorks assembly folder of its solids' mass candidates, weighted 1/candidates",
    "nodes": nodes,
}, open(out, "w"), indent=1, sort_keys=True)
open(out, "a").write("\n")
print(f"{len(nodes)} nodes -> {out}")
