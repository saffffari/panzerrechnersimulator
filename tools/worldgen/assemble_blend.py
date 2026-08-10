# assemble_blend.py — Kessel world assembler (stage 2 of 2): headless bpy.
# Reads the manifest from gen_world.py, builds the DEM terrain mesh (with the act's real satellite
# imagery draped on it when the manifest names a texture), imports each unique PSX GLB ONCE into a
# hidden library collection, instances every placement as a collection-instance empty (cheap:
# thousands of instances share one mesh), adds landmark/spawn/goal marker empties + labels, then
# saves a .blend the user can open and fine-tune, plus a workbench overview render.
#
# Restored 2026-07-02 (deleted in the in-engine pivot d1ef1e4) for the Act-2 Blender-first build.
#
# Usage: blender --background --python tools/worldgen/assemble_blend.py -- \
#            <manifest.json> <out.blend> [render.png]
#
# Coordinates: manifest is in ENGINE space (x=east, y=up, z=south). Blender is Z-up:
#   blender_x = x,  blender_y = -z (north),  blender_z = y.  Yaw about engine +Y maps to +Z here.
import json
import math
import os
import sys

import numpy as np

import bpy

argv = sys.argv[sys.argv.index("--") + 1:]
MANIFEST, OUT_BLEND = argv[0], argv[1]
RENDER = argv[2] if len(argv) > 2 else None

with open(MANIFEST) as f:
    man = json.load(f)
mdir = os.path.dirname(os.path.abspath(MANIFEST))

# Hand edits from the in-engine worldbuilder (`cargo run -- edit`): an overlay of set/delete/add
# ops beside the manifest, applied here too so the .blend and the engine see the same world.
_epath = os.path.join(mdir, "manifest_edits.json")
if os.path.exists(_epath):
    _edits = json.load(open(_epath))
    _n = 0
    for _op in _edits.get("ops", []):
        _k = _op.get("op")
        if _k == "set" and _op["i"] < len(man["placements"]):
            _p = man["placements"][_op["i"]]
            _p[1], _p[2], _p[3], _p[4], _p[5] = _op["x"], _op["y"], _op["z"], _op["deg"], _op["s"]
            _n += 1
        elif _k == "delete" and _op["i"] < len(man["placements"]):
            man["placements"][_op["i"]] = None
            _n += 1
        elif _k == "add":
            _a = _op["asset"]
            _ai = man["assets"].index(_a) if _a in man["assets"] else len(man["assets"])
            if _ai == len(man["assets"]):
                man["assets"].append(_a)
            man["placements"].append([_ai, _op["x"], _op["y"], _op["z"], _op["deg"], _op["s"]])
            _n += 1
        elif _k == "marker_set" and _op["i"] < len(man.get("markers", [])):
            _m = man["markers"][_op["i"]]
            _m["x"], _m["y"], _m["z"] = _op["x"], _op["y"], _op["z"]
            _n += 1
        elif _k == "marker_add":
            man.setdefault("markers", []).append(
                dict(name=_op.get("name", ""), kind=_op.get("kind", "Site"),
                     x=_op["x"], y=_op["y"], z=_op["z"]))
            _n += 1
        elif _k == "marker_delete" and _op["i"] < len(man.get("markers", [])):
            man["markers"][_op["i"]] = None
            _n += 1
    man["placements"] = [p for p in man["placements"] if p is not None]
    man["markers"] = [m for m in man.get("markers", []) if m is not None]
    print(f"[blend] applied {_n} worldbuilder edits from manifest_edits.json")
heights = np.load(os.path.join(mdir, man["terrain"]["heights"]))
res = man["terrain"]["res"]
spacing = man["terrain"]["spacing"]

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.unit_settings.system = "METRIC"

root = bpy.data.collections.new("Kessel_" + man["act"].replace(" ", "_"))
scene.collection.children.link(root)


def coll(name, parent=root, hide=False):
    c = bpy.data.collections.new(name)
    parent.children.link(c)
    if hide:
        c.hide_viewport = True
        c.hide_render = True
    return c


c_terrain = coll("Terrain")
c_lib = coll("Library", hide=True)
c_world = coll("World")
c_landmarks = coll("Landmarks")
c_bunker = coll("Bunker")
c_markers = coll("Markers")

# ── Terrain mesh from the DEM window (numpy foreach_set — v2 grids are up to 768²) ────────────────
half = (res - 1) * 0.5 * spacing
ax = np.arange(res, dtype=np.float32) * spacing - half
vx = np.tile(ax, res)                                  # engine x per vertex (row-major j, i)
vy = -np.repeat(ax, res)                               # blender y = -engine z
vz = heights.astype(np.float32).ravel()
co = np.empty(res * res * 3, np.float32)
co[0::3] = vx
co[1::3] = vy
co[2::3] = vz
jj, ii = np.meshgrid(np.arange(res - 1), np.arange(res - 1), indexing="ij")
a0 = (jj * res + ii).ravel()
quads = np.stack([a0, a0 + 1, a0 + res + 1, a0 + res], axis=1).astype(np.int32)
nfaces = quads.shape[0]
tmesh = bpy.data.meshes.new("terrain")
tmesh.vertices.add(res * res)
tmesh.vertices.foreach_set("co", co)
tmesh.loops.add(nfaces * 4)
tmesh.loops.foreach_set("vertex_index", quads.ravel())
tmesh.polygons.add(nfaces)
tmesh.polygons.foreach_set("loop_start", np.arange(0, nfaces * 4, 4, dtype=np.int32))
tmesh.polygons.foreach_set("loop_total", np.full(nfaces, 4, dtype=np.int32))
tmesh.update()
tmesh.validate()
tobj = bpy.data.objects.new("Terrain_DEM", tmesh)

tcolor = tuple(man["terrain"].get("color", (0.82, 0.86, 0.92, 1.0)))  # default: act-1 snow
tmat = bpy.data.materials.new("ground")
tmat.diffuse_color = tcolor
tex_rel = man["terrain"].get("texture")
if tex_rel:
    # UVs: u east 0→1, v: image row 0 = north = j 0 → v = 1 at j 0. (vectorized)
    uvl = tmesh.uv_layers.new(name="UVMap")
    vidx = quads.ravel().astype(np.int64)
    uv = np.empty(vidx.size * 2, np.float32)
    uv[0::2] = (vidx % res) / (res - 1)
    uv[1::2] = 1.0 - (vidx // res) / (res - 1)
    uvl.data.foreach_set("uv", uv)
    img_path = os.path.join(os.getcwd(), tex_rel)
    if os.path.exists(img_path):
        img = bpy.data.images.load(img_path)
        tmat.use_nodes = True
        bsdf = tmat.node_tree.nodes["Principled BSDF"]
        bsdf.inputs["Roughness"].default_value = 1.0
        texn = tmat.node_tree.nodes.new("ShaderNodeTexImage")
        texn.image = img
        gam = tmat.node_tree.nodes.new("ShaderNodeGamma")
        # Sentinel TCI is authored dark; per-act grade (manifest terrain.gamma/.saturation)
        gam.inputs["Gamma"].default_value = man["terrain"].get("gamma", 0.8)
        sat = tmat.node_tree.nodes.new("ShaderNodeHueSaturation")
        sat.inputs["Saturation"].default_value = man["terrain"].get("saturation", 1.5)
        tmat.node_tree.links.new(texn.outputs["Color"], gam.inputs["Color"])
        tmat.node_tree.links.new(gam.outputs["Color"], sat.inputs["Color"])
        tmat.node_tree.links.new(sat.outputs["Color"], bsdf.inputs["Base Color"])
    else:
        print(f"[blend] MISSING terrain texture {tex_rel}")
tmesh.materials.append(tmat)
c_terrain.objects.link(tobj)

# ── Library: import each unique GLB once ──────────────────────────────────────────────────────────
lib_colls = []
repo_root = os.getcwd()
for idx, rel in enumerate(man["assets"]):
    path = os.path.join(repo_root, rel)
    name = os.path.splitext(os.path.basename(rel))[0]
    sub = bpy.data.collections.new(f"lib_{name}")
    c_lib.children.link(sub)
    lib_colls.append(sub)
    if not os.path.exists(path):
        print(f"[blend] MISSING {rel}")
        continue
    before = set(bpy.data.objects)
    try:
        bpy.ops.import_scene.gltf(filepath=path)
    except Exception as e:  # a bad GLB must not kill the whole build
        print(f"[blend] import failed {rel}: {e}")
        continue
    news = [o for o in bpy.data.objects if o not in before]
    for ob in news:
        for uc in ob.users_collection:
            uc.objects.unlink(ob)
        sub.objects.link(ob)
    # PSX Nature GLBs are authored Z-up with arbitrary mesh offsets — they import lying on their
    # side, floating. Normalize the library copy once: upright (+90° X) and bbox bottom-centre at
    # the origin, so instances ground cleanly. (Instances inherit this via the collection.)
    if "PSX Nature" in rel:
        from mathutils import Matrix as _M, Vector as _V
        fix = _M.Rotation(math.radians(90), 4, "X")
        meshes = [o for o in news if o.type == "MESH"]
        for ob in meshes:
            ob.matrix_world = fix @ ob.matrix_world
        bpy.context.view_layer.update()
        mn = _V((1e9,) * 3)
        mx = _V((-1e9,) * 3)
        for ob in meshes:
            for c in ob.bound_box:
                w = ob.matrix_world @ _V(c)
                mn = _V(map(min, mn, w))
                mx = _V(map(max, mx, w))
        off = _V((-(mn.x + mx.x) / 2, -(mn.y + mx.y) / 2, -mn.z))
        for ob in meshes:
            ob.matrix_world = _M.Translation(off) @ ob.matrix_world

# ── Placements: collection-instance empties ───────────────────────────────────────────────────────
counts = {}
for (ai, x, y, z, rot_deg, s) in man["placements"]:
    sub = lib_colls[ai]
    e = bpy.data.objects.new(f"i_{sub.name[4:]}", None)
    e.instance_type = "COLLECTION"
    e.instance_collection = sub
    e.location = (x, -z, y)
    e.rotation_euler = (0.0, 0.0, math.radians(rot_deg))
    if isinstance(s, (list, tuple)):  # per-axis (engine x,y-up,z) → blender (x, z, y-up)
        e.scale = (s[0], s[2], s[1])
    elif s != 1.0:
        e.scale = (s, s, s)
    # bunker pieces live below grade → their own collection so the user can solo it
    (c_bunker if y < -1.5 else c_world).objects.link(e)
    counts[sub.name] = counts.get(sub.name, 0) + 1

# ── Light + camera for the overview render ────────────────────────────────────────────────────────
from mathutils import Vector

ov = man.get("overview")
if ov:  # engine coords → blender
    cam_pos = Vector((ov["pos"][0], -ov["pos"][2], ov["pos"][1]))
    cam_tgt = Vector((ov["target"][0], -ov["target"][2], ov["target"][1]))
else:  # legacy act-1 framing: high oblique from the SE of the city district
    cam_pos = Vector((1050.0, -520.0, 210.0))
    cam_tgt = Vector((580.0, -80.0, 15.0))

# ── Markers: big empties + floating text labels (yawed to face the overview camera) ───────────────
for m in man["markers"]:
    e = bpy.data.objects.new(f"MARK_{m['kind']}_{m['name']}", None)
    e.empty_display_type = "SPHERE" if m["kind"] in ("Spawn", "Goal") else "PLAIN_AXES"
    e.empty_display_size = 18.0
    e.location = (m["x"], -m["z"], m.get("y", 0.0) + 12.0)
    c_markers.objects.link(e)
    txt = bpy.data.curves.new(m["name"], type="FONT")
    txt.body = m["name"]
    txt.size = 26.0
    txt.align_x = "CENTER"
    t = bpy.data.objects.new(f"LBL_{m['name']}", txt)
    t.location = (m["x"], -m["z"], m.get("y", 0.0) + 34.0)
    dx, dy = cam_pos.x - m["x"], cam_pos.y - (-m["z"])
    t.rotation_euler = (math.radians(90), 0, math.atan2(dx, -dy))
    t.visible_shadow = False  # a 26 m label must not project "Canal Bridge" onto the hillside
    c_markers.objects.link(t)

# ── Wires: sagging telephone lines between pole tops (manifest polylines, ENGINE coords) ──────────
wires = man.get("wires", [])
if wires:
    c_wires = coll("Wires")
    wmat = bpy.data.materials.new("wire")
    wmat.diffuse_color = (0.02, 0.02, 0.02, 1.0)
    for wi, poly in enumerate(wires):
        cu = bpy.data.curves.new(f"wire_{wi}", type="CURVE")
        cu.dimensions = "3D"
        cu.bevel_depth = 0.02
        cu.bevel_resolution = 0
        cu.resolution_u = 6
        sp = cu.splines.new("NURBS")
        sp.points.add(len(poly) - 1)
        for pi, (x, y, z) in enumerate(poly):
            sp.points[pi].co = (x, -z, y, 1.0)
        sp.use_endpoint_u = True
        cu.materials.append(wmat)
        ob = bpy.data.objects.new(f"Wire_{wi}", cu)
        ob.visible_shadow = False
        c_wires.objects.link(ob)

# Warm low morning sun from the east (+X), plus a Nishita sky for the hazy spring light.
sun = bpy.data.objects.new("Sun", bpy.data.lights.new("Sun", "SUN"))
sun.data.energy = 4.0
sun.data.color = (1.0, 0.9, 0.75)
sun.rotation_euler = Vector((-1.0, -0.15, -0.27)).normalized().to_track_quat("-Z", "Y").to_euler()
root.objects.link(sun)

world = bpy.data.worlds.new("SpringMorning")
world.use_nodes = True
wn = world.node_tree
sky = wn.nodes.new("ShaderNodeTexSky")
sky.sun_elevation = math.radians(12.0)
sky.sun_rotation = math.radians(270.0)  # sun in the +X east
if hasattr(sky, "altitude"):
    sky.altitude = 250.0  # lift the sky horizon toward the overview camera's
if hasattr(sky, "ground_albedo"):
    sky.ground_albedo = 0.5  # haze the below-horizon band instead of a dark void
bg = wn.nodes["Background"]
bg.inputs["Strength"].default_value = 0.45
wn.links.new(sky.outputs["Color"], bg.inputs["Color"])
scene.world = world
cam = bpy.data.objects.new("OverviewCam", bpy.data.cameras.new("OverviewCam"))
cam.location = cam_pos
cam.rotation_euler = (cam_tgt - cam_pos).to_track_quat("-Z", "Y").to_euler()
cam.data.clip_end = 15000.0
cam.data.lens = 32.0
root.objects.link(cam)
scene.camera = cam

n_place = len(man["placements"])
print(f"[blend] {n_place} instances of {len(man['assets'])} assets; terrain {res}²; "
      f"{len(man['markers'])} markers")
top = sorted(counts.items(), key=lambda kv: -kv[1])[:8]
print("[blend] top pieces:", ", ".join(f"{k[4:]}×{v}" for k, v in top))

bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(OUT_BLEND))
print(f"[blend] saved {OUT_BLEND}")

if RENDER:
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 1728
    scene.render.resolution_y = 972
    scene.render.filepath = os.path.abspath(RENDER)
    bpy.ops.render.render(write_still=True)
    print(f"[blend] rendered {RENDER}")
