#!/usr/bin/env python3
"""Interior parts the kit CAD registration did not make components of, taken from the game's own
meshes: the turret GLB's named nodes and the separately baked handwheel parts. The game meshes are
tessellations of the same kit CAD (tools/tiger_cad_register.py, "register"), at the game's scale.

    python3 -I tools/tiger_game_part_components.py [--baked DIR] [--meshes assets/fea/tiger_cad/components]
                                                   [--catalog catalog/tiger_cad_components.v1.json]

Not identified: the gunner's own traverse handwheel (Turret_0086, kit 06U19 'HAND', is a 1.75 m rail
across the turret, not a handwheel).

Writes assets/fea/tiger_cad/components/<id>.obj (gitignored; engine metres, turret-local at 0 deg gun
elevation, as the CAD components) and adds or replaces each part's entry in the catalog's
`components` list (id, frame, the game nodes it is made of, its closed-mesh volume). Run
tools/tiger_cad_voxelize.py afterwards for the envelopes. Deterministic.
"""
import argparse
import json
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from tiger_cad_register import dump, glb_node_meshes  # noqa: E402

# Handwheel hubs, turret-local (src/interior_rig.rs WHEEL1_POS / WHEEL2_POS): the part GLBs are
# baked about their hubs.
WHEEL1_POS = (0.255, 1.317, 0.333)
WHEEL2_POS = (0.912, 1.949, -0.386)

PARTS = [
    {"id": "traverse_gearbox", "name": "Commander's turret traverse gearbox (kit 05J27 'MOTOR')",
     "nodes": ["Turret_0137"],
     "evidence": "Cast gearbox under the commander's traverse flywheel at the turret's right rear (game node "
                 "Turret_0137, kit 05J27 'MOTOR', iron_cast_machinery in docs/fea/tiger_parts.csv)."},
    {"id": "traverse_flywheel", "name": "Commander's auxiliary traverse flywheel (kit 05J55 'ROLLOR')",
     "glb": "tiger_part_wheel2.glb", "offset": WHEEL2_POS,
     "evidence": "The game's separately baked wheel2 (src/interior_rig.rs: the commander's auxiliary traverse "
                 "flywheel on top of the traverse gearbox, 595 turns per turret revolution)."},
    {"id": "traverse_pedal", "name": "Hydraulic traverse foot pedal (kit 05J26 'PEDLAL')",
     "nodes": ["Turret_0068"],
     "evidence": "Rocker pedal on the basket floor at the gunner's feet (game node Turret_0068, kit 05J26)."},
    {"id": "elevation_gearbox", "name": "Gun elevation gearbox (kit 06J56 'BLOCK')",
     "nodes": ["Turret_0084"],
     "evidence": "Block beside the elevation handwheel on the cradle's right (game node Turret_0084, kit "
                 "06J56 'BLOCK')."},
    {"id": "elevation_handwheel", "name": "Gunner's elevation handwheel (kit 05J55 'ROLLOR')",
     "glb": "tiger_part_wheel1.glb", "offset": WHEEL1_POS,
     "evidence": "The game's separately baked wheel1 (src/interior_rig.rs: the gunner's elevation wheel)."},
    {"id": "basket_floor", "name": "Turret basket floor and its plates (kit 05U25 'BASE', 25H10/25H11 'PLATE')",
     "nodes": ["TURRET FLOOR", "Turret_0007", "Turret_0204"],
     "evidence": "The turning floor of the fighting compartment (game nodes TURRET FLOOR, Turret_0007, "
                 "Turret_0204). Its drive is the hydraulic traverse motor (component turret_traverse_motor)."},
]


def closed_volume(points, tris):
    """Signed volume of a closed triangle mesh (divergence theorem), absolute."""
    a, b, c = points[tris[:, 0]], points[tris[:, 1]], points[tris[:, 2]]
    return abs(float(np.einsum("ij,ij->i", a, np.cross(b, c)).sum()) / 6.0)


def write_obj(path, objects):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        base = 1
        for name, pts, tris in objects:
            fh.write(f"o {name}\n")
            for p in pts:
                fh.write(f"v {p[0]:.6f} {p[1]:.6f} {p[2]:.6f}\n")
            for t in tris:
                fh.write(f"f {t[0] + base} {t[1] + base} {t[2] + base}\n")
            base += len(pts)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--baked", default="/windows/shop/panzerrechnersystem/assets/tanks/tiger/baked")
    ap.add_argument("--meshes", default="assets/fea/tiger_cad/components")
    ap.add_argument("--catalog", default="catalog/tiger_cad_components.v1.json")
    args = ap.parse_args()
    turret = {m["name"]: (m["P"], m["T"]) for m in glb_node_meshes(os.path.join(args.baked, "tiger_turret.glb"))}
    with open(args.catalog, "r", encoding="utf-8") as fh:
        catalog = json.load(fh)
    os.makedirs(args.meshes, exist_ok=True)
    for part in PARTS:
        objects = []
        if "nodes" in part:
            for node in part["nodes"]:
                pts, tris = turret[node]
                objects.append((node, np.asarray(pts, float), np.asarray(tris, int)))
        else:
            for m in glb_node_meshes(os.path.join(args.baked, part["glb"])):
                objects.append((m["name"], np.asarray(m["P"], float) + np.asarray(part["offset"]), np.asarray(m["T"], int)))
        volume = sum(closed_volume(p, t) for _, p, t in objects)
        write_obj(os.path.join(args.meshes, part["id"] + ".obj"), objects)
        entry = {"id": part["id"], "name": part["name"], "frame": "turret", "confidence": "game_mesh_node",
                 "game_nodes": part.get("nodes", [part.get("glb")]), "evidence": part["evidence"],
                 "doubts": "Taken from the game's mesh, not a registered kit solid; its volume is the mesh's.",
                 "volume_m3": {"solids": round(volume, 6)}}
        catalog["components"] = [c for c in catalog["components"] if c["id"] != part["id"]] + [entry]
        print(f"{part['id']:22s} {sum(len(t) for _, _, t in objects):6d} triangles, {volume * 1e3:8.3f} litres")
    dump(args.catalog, catalog)


if __name__ == "__main__":
    main()
