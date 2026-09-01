# Extract the Tiger interior RIG from the authored .blend into engine-space metadata.
#
#   blender --background <blend> --python tools/bake_tiger_interior.py -- <out.json>
#
# This is the metadata pass (Phase 1): pivots, animated-part transforms, the shell rack (filtered
# from the trays it's mixed with), the walkable floor footprint, the breech collision box, and the
# interior lights — everything the engine needs to PLACE and ANIMATE the interior. The heavy
# geometry re-bake (hull/turret/gun GLBs + separated animated-part GLBs) is a later phase.
#
# Conventions (CLAUDE.md "Assets"): engine = (blender_x, blender_z, -blender_y); barrel-forward is
# already -Y in this file (= engine +Z). Turret parts are emitted in TURRET-local engine coords
# (relative to Turret_Pivot, which has identity rotation) so the engine places them via
# turret_model() * local; hull parts relative to the hull ground origin.

import bpy, mathutils, math, json, sys
from pathlib import Path

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
PROJECT_ROOT = Path(__file__).resolve().parents[1]
OUT = argv[0] if argv else str(
    PROJECT_ROOT / "assets/tanks/tiger/baked/tiger_interior.json"
)

def obj(n): return bpy.data.objects.get(n)
def eng(v):  # blender vec -> engine tuple
    return [round(v.x, 4), round(v.z, 4), round(-v.y, 4)]

# ---- hull ground origin: footprint centre X/Y at the lowest point ----
# EXCLUDE the gun subtree — the barrel juts ~4.8 m forward and would drag the fore-aft centre off
# (the T-34 bake computes the origin from the hull group only for the same reason).
gun_subtree = set(obj("Gun_pivot").children_recursive) if obj("Gun_pivot") else set()
allpts = [o.matrix_world @ mathutils.Vector(c)
          for o in bpy.data.objects if o.type == 'MESH' and o not in gun_subtree for c in o.bound_box]
gmin = mathutils.Vector((min(p.x for p in allpts), min(p.y for p in allpts), min(p.z for p in allpts)))
gmax = mathutils.Vector((max(p.x for p in allpts), max(p.y for p in allpts), max(p.z for p in allpts)))
hull_org = mathutils.Vector(((gmin.x + gmax.x) / 2, (gmin.y + gmax.y) / 2, gmin.z))

TP = obj("Turret_Pivot").matrix_world.translation.copy()   # turret ring pivot (world)
GP = obj("Gun_pivot").matrix_world.translation.copy()        # gun trunnion (world)

def hull_local(world_v):   # engine coords relative to hull ground origin
    return eng(world_v - hull_org)
def turret_local(world_v): # engine coords relative to Turret_Pivot (turret frame)
    return eng(world_v - TP)

data = {"convention": "engine=(bx,bz,-by); turret parts turret-local, hull parts hull-local"}

# ---- core pivots (feed / cross-check TIGER_PARTS) ----
data["hull_origin_world"] = [round(hull_org.x,4), round(hull_org.y,4), round(hull_org.z,4)]
data["turret_pivot"] = hull_local(TP)              # hull-local (engine)
data["gun_trunnion"] = turret_local(GP)            # turret-local (engine) = gun_pivot - turret_pivot

def part(name, frame="turret"):
    o = obj(name)
    if not o: return None
    w = o.matrix_world
    loc = w.translation
    e = mathutils.Euler(w.to_euler())
    d = {
        "pos": (turret_local(loc) if frame == "turret" else hull_local(loc)),
        "rot_deg": [round(math.degrees(a), 2) for a in (e.x, e.y, e.z)],
        "frame": frame,
    }
    if o.type == 'MESH':
        d["dim"] = [round(x, 4) for x in o.dimensions]
    return d

# ---- hatches (each with its pivot + open axis) ----
data["hatches"] = {
    "cupola":       {**part("cupola hatch hinge"), "open": "lift 0.145 up then swing 90 about Z (two-step)"},
    "gunner":       {**part("Gunner Hatch Pivot"), "open": "up to 95 about X"},
    "turret_escape":{**part("turret escape hatch pivot"), "open": "swing"},
    "driver":       {**part("Drivers Hatch Pivot", frame="hull"), "open": "90"},
}

# ---- handwheels (spin with aim) ----
data["wheels"] = {"Gun wheel1": part("Gun wheel1"), "wheel2": part("wheel2")}

# ---- TPZ turret-position dial: needle spins by turret-vs-hull yaw ----
needle = obj("TPZ dial indicator")
backing = obj("turret position indicator backing")
data["tpz"] = {
    "needle": part("TPZ dial indicator"),
    "backing": part("turret position indicator backing"),
    "note": "needle origin ~= dial centre; spin about the dial-face normal by turret_yaw. Group with turret (currently unparented).",
}

# ---- round: seat pose (chambered) + ready pose (1.13 m back along its local Y) ----
rc = obj("Round in the chamber")
if rc:
    seat = rc.matrix_world.translation.copy()
    local_y = (rc.matrix_world.to_3x3() @ mathutils.Vector((0, 1, 0))).normalized()  # round long axis
    ready = seat + local_y * 1.13
    data["round"] = {
        "seat": turret_local(seat),
        "ready": turret_local(ready),
        "ram_axis_engine": eng(local_y),
        "length_m": round(max(rc.dimensions), 4),
        "mesh": "Round in the chamber",
    }

# ---- shell rack: the Shells collection is shells + trays; keep only shell-shaped members ----
shells = []
col = bpy.data.collections.get("Shells")
if col:
    for o in col.all_objects:
        if o.type != 'MESH': continue
        dims = sorted(o.dimensions)
        is_shell = dims[2] > 0.7 and dims[1] < 0.25       # long + thin = a round, not a flat tray
        if not is_shell: continue
        w = o.matrix_world
        e = mathutils.Euler(w.to_euler())
        # hull- vs turret-stowed by which subtree it hangs under
        p = o.parent
        frame = "hull"
        while p:
            if p.name == "Turret_Pivot": frame = "turret"; break
            p = p.parent
        shells.append({
            "pos": (turret_local(w.translation) if frame == "turret" else hull_local(w.translation)),
            "rot_deg": [round(math.degrees(a), 1) for a in (e.x, e.y, e.z)],
            "frame": frame,
        })
data["shells"] = {"count": len(shells), "slots": shells}

# ---- TURRET FLOOR: walk boundary footprint (turret-local) + stand height ----
fl = obj("TURRET FLOOR")
if fl:
    pts = [fl.matrix_world @ mathutils.Vector(c) for c in fl.bound_box]
    fmin = mathutils.Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    fmax = mathutils.Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    data["turret_floor"] = {
        "min": turret_local(fmin), "max": turret_local(fmax),
        "stand_y_engine": round((fmax.z - TP.z), 4),   # top surface, turret-local Y (engine up)
    }

# ---- breech collision box (turret-local AABB at rest gun pose) ----
br = obj("Breech")
if br:
    pts = [br.matrix_world @ mathutils.Vector(c) for c in br.bound_box]
    bmin = mathutils.Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    bmax = mathutils.Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    data["breech_box"] = {"min": turret_local(bmin), "max": turret_local(bmax)}

# ---- interior point lights (whitelist by type; skip the stray high-power scene light) ----
lights = []
for o in bpy.data.objects:
    if o.type != 'LIGHT': continue
    L = o.data
    if getattr(L, "energy", 0) > 100:   # a 1000W scene light is not interior
        continue
    p = o.parent; frame = "hull"
    while p:
        if p.name == "Turret_Pivot": frame = "turret"; break
        p = p.parent
    lights.append({
        "name": o.name,
        "pos": (turret_local(o.matrix_world.translation) if frame == "turret" else hull_local(o.matrix_world.translation)),
        "color": [round(c, 3) for c in L.color],
        "energy": round(getattr(L, "energy", 0), 3),
        "frame": frame,
    })
data["lights"] = lights

# ---- material keys the engine treats specially ----
data["material_keys"] = {
    "daylight_aperture": "periscope glass",
    "emissive_dial": ["Illuminated Dial", "dial red"],
    "ivory_paint": "Interior Paint",
    "floor_steel": "Turret Floor",
}

with open(OUT, "w") as f:
    json.dump(data, f, indent=2)

# ---- console summary ----
print("\n===== TIGER INTERIOR RIG =====")
print(f"hull ground origin (world): {data['hull_origin_world']}")
print(f"turret_pivot (hull-local, engine): {data['turret_pivot']}   [TIGER_PARTS has (0.0, 0.524, 0.019)]")
print(f"gun_trunnion (turret-local, engine): {data['gun_trunnion']}   [TIGER_PARTS has (0.004, 1.845, 1.040)]")
print(f"shells kept: {len(shells)}  (filtered from the trays)")
print(f"lights: {[l['name'] for l in lights]}")
print(f"round seat {data.get('round',{}).get('seat')}  ready {data.get('round',{}).get('ready')}")
print(f"turret_floor: {data.get('turret_floor')}")
print(f"wrote {OUT}")
