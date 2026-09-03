# Bake the new_tanks T-34/76 FBX into engine GLBs — hull / turret / gun with rig-matched origins.
#
#   blender --background --python tools/bake_t34.py
#
# Conventions (CLAUDE.md "Assets"): hull origin at ground contact (bottom-center); turret origin at
# the ring pivot; gun origin at the trunnion; barrel-forward = engine +Z, via Blender's default
# glTF Y-up export (engine = (blender_x, blender_z, -blender_y)) — so in Blender the barrel must
# point -Y before export. Prints the PIVOT REPORT used to update tank.rs::T34_PARTS.
#
# Includes the Cycles shim for the Blender 5.1.2 FBX addon (it pokes the removed
# CyclesLightSettings.cast_shadow when the FBX contains lights — this source does).
import bpy, mathutils, math, os, sys
from pathlib import Path

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
PROJECT_ROOT = Path(__file__).resolve().parents[1]
SRC = argv[0] if argv else str(
    PROJECT_ROOT / "assets/new_tanks/tank-t-3476-1942/source/T-34-76.fbx"
)
OUT = argv[1] if len(argv) > 1 else str(PROJECT_ROOT / "assets/tanks/T34-76/baked")

# ── Cycles shim (SPEC T1's "FBX importer broken" — it isn't, it just needs this) ──
lt = bpy.data.lights.new("probe", "POINT")
C = type(lt.cycles)
if not hasattr(C, "cast_shadow"):
    C.cast_shadow = property(lambda self: True, lambda self, v: None)
bpy.data.lights.remove(lt)

for o in list(bpy.data.objects):
    bpy.data.objects.remove(o)
bpy.ops.import_scene.fbx(filepath=SRC)

# Drop lights/cameras; keep meshes.
for o in list(bpy.data.objects):
    if o.type != "MESH":
        bpy.data.objects.remove(o)

TURRET = {"T34 Head", "T34 Body.001", "T34 Body.002", "Cube"}
GUN = {"T34 Gun"}
groups = {"hull": [], "turret": [], "gun": []}
for o in bpy.data.objects:
    if o.name in GUN:
        groups["gun"].append(o)
    elif o.name in TURRET:
        groups["turret"].append(o)
    else:
        groups["hull"].append(o)

def wbb(objs):
    pts = [ob.matrix_world @ mathutils.Vector(c) for ob in objs for c in ob.bound_box]
    mn = mathutils.Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    mx = mathutils.Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    return mn, mx

# ── Face the barrel down -Y: the source runs along X; the gun's long axis tells us the front. ──
gmn, gmx = wbb(groups["gun"])
hmn, hmx = wbb(groups["hull"])
gun_cx = (gmn.x + gmx.x) / 2
hull_cx = (hmn.x + hmx.x) / 2
front_is_plus_x = gun_cx > hull_cx  # the barrel overhangs the front
# Rotate about Z so front (+X or -X) → -Y.
ang = math.radians(-90 if front_is_plus_x else 90)
rot = mathutils.Matrix.Rotation(ang, 4, "Z")
for o in bpy.data.objects:
    o.matrix_world = rot @ o.matrix_world
bpy.context.view_layer.update()

# ── Join each group into one object ──
joined = {}
for name, objs in groups.items():
    bpy.ops.object.select_all(action="DESELECT")
    for ob in objs:
        ob.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.join()
    j = bpy.context.view_layer.objects.active
    j.name = f"t34_{name}"
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    joined[name] = j

hmn, hmx = wbb([joined["hull"]])
tmn, tmx = wbb([joined["turret"]])
gmn, gmx = wbb([joined["gun"]])

# ── Origins ──
# hull: ground contact, footprint center.
hull_org = mathutils.Vector(((hmn.x + hmx.x) / 2, (hmn.y + hmx.y) / 2, hmn.z))
# turret: ring pivot — turret bbox center in plan, at the hull's top deck height.
tur_org = mathutils.Vector(((tmn.x + tmx.x) / 2, (tmn.y + tmx.y) / 2, hmx.z * 0.985))
# gun: trunnion — the barrel's REAR (max y, since forward is -y), on the turret centerline,
# at the gun bbox vertical center.
gun_org = mathutils.Vector((tur_org.x, gmx.y, (gmn.z + gmx.z) / 2))

for name, org in [("hull", hull_org), ("turret", tur_org), ("gun", gun_org)]:
    j = joined[name]
    j.data.transform(mathutils.Matrix.Translation(-org))
    j.matrix_world = mathutils.Matrix.Identity(4)
bpy.context.view_layer.update()

os.makedirs(OUT, exist_ok=True)
for name in ["hull", "turret", "gun"]:
    bpy.ops.object.select_all(action="DESELECT")
    joined[name].select_set(True)
    bpy.ops.export_scene.gltf(
        filepath=f"{OUT}/t34_{name}.glb",
        use_selection=True,
        export_format="GLB",
        export_yup=True,
    )
    print(f"BAKED {OUT}/t34_{name}.glb")

# ── PIVOT REPORT (engine coords: (bx, bz, -by)) — feeds tank.rs::T34_PARTS ──
def eng(v):
    return (round(v.x, 3), round(v.z, 3), round(-v.y, 3))

tur_in_hull = tur_org - hull_org
gun_in_tur = gun_org - tur_org
gun_len = gun_org.y - gmn.y  # forward is -y: tip at min y
print("PIVOT turret_in_hull(engine)", eng(tur_in_hull))
print("PIVOT gun_in_turret(engine)", eng(gun_in_tur))
print("PIVOT gun_len", round(gun_len, 3))
print("PIVOT hull_size(engine x,y,z)",
      (round(hmx.x - hmn.x, 2), round(hmx.z - hmn.z, 2), round(hmx.y - hmn.y, 2)))
