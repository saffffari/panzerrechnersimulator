"""urban_intro_massing.py — an offline massing preview of the urban-intro district (role L, review aid).

The runtime does not build ``building_instances`` yet, so a headless game render of the district
workshop shows its terrain (pads, mounds, ridge, craters) and dressing, but no buildings. This
Blender script draws every instance of a workshop's ``manifest_edits.json`` as a plain box (intact)
or a roofless shell (burned, shelled, partly collapsed; a burned timber house as its stove stack) on
the workshop heightfield, from the same cameras as the evidence renders, so the district's
composition can be compared with the photographs P names before the runtime exists.

It is NOT the building grammar's geometry and NOT the game's light transport (Cycles, a grey sky
and a low sun): a review picture only, never evidence of what the game shows. Storey heights are
R1's typical values (merchant ground storey 4.5 m, upper 4.0 m; apartment plinth 1.0 m and 3.2 m
storeys; parapets 0.8 m; timber walls 3.0 m; workshop 6.0 m).

    blender --background --factory-startup --python tools/worldgen/urban_intro_massing.py -- \\
        [--workshop DIR] [--prefix captures/urban_district_massing_] [VIEW ...]
    python3 tools/worldgen/urban_intro_massing.py --commands   # the matching headless game renders
"""
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKSHOP = "assets/worldgen/workshops/act1-urban-intro-20260928"
REVIEW = WORKSHOP + "-review-daylight"
EDITS_KEY = "urban_intro_district_20260928"
# name: engine camera x, y, z, yaw, pitch (degrees; yaw 0 looks north, -z), as the evidence renders
VIEWS = {
    "tram_street_north": (1850, 8.5, 285, 0, -2),
    "aerial": (1560, 135, 560, 45, -24),
    "square": (1745, 9, 345, 45, -6),
    "fallen_facade": (1970, 13, 262, 180, -9),
    "fringe_lane": (1610, 15, 150, 0, -4),
    "open_ground_north": (1760, 19, -235, 160, -5),
    "between_ruins": (1790, 12.3, 150, 285, -3),
}
FOV_Y_DEG = 55.0   # the recon screenshot camera (src/main.rs)
FOG = 0.002


def args_after_dashes(argv):
    return argv[argv.index("--") + 1:] if "--" in argv else []


def commands():
    for name, (x, y, z, yaw, pitch) in VIEWS.items():
        print(f"KESSEL_RECON_MANIFEST={REVIEW}/manifest.v1.json cargo run -- screenshot "
              f"captures/urban_district_{name}.png {x} {y} {z} {yaw} {pitch} {FOG}")


def storey_height(typology, storeys):
    if typology == "merchant_brick_house":
        return 4.5 + 4.0 * (storeys - 1) + 0.8
    if typology == "apartment_block":
        return 1.0 + 3.2 * storeys + 0.8
    if typology == "wooden_house":
        return 3.0
    return 6.0


def masses(instances, pad_y, ground):
    """(material, (min xyz, max xyz)) boxes in engine coordinates for every instance."""
    out = []
    t_wall = 0.6
    for inst in instances:
        t, cond = inst["typology"], inst["condition"]
        quarter = int(round(inst["yaw_deg"] / 90.0)) % 4
        w, d = (inst["width_m"], inst["depth_m"]) if quarter % 2 == 0 else (inst["depth_m"], inst["width_m"])
        cx, cz = inst["x"], inst["z"]
        base = pad_y.get(inst["id"], ground(cx, cz)) - 1.0
        x0, x1, z0, z1 = cx - w / 2, cx + w / 2, cz - d / 2, cz + d / 2
        h = storey_height(t, inst["storeys"])
        top = base + 1.0 + h
        if t == "wooden_house":
            if cond == "intact":
                out.append(("timber", (x0, base, z0), (x1, top, z1)))
                out.append(("snow", (x0 - 0.3, top, z0 - 0.3), (x1 + 0.3, top + 1.8, z1 + 0.3)))
            else:   # burned timber: the stove and its stack remain (P L2 [04-27])
                out.append(("brick", (cx - 0.6, base, cz - 0.6), (cx + 0.6, base + 6.0, cz + 0.6)))
            continue
        mat = "workshop" if t == "workshop" else "brick"
        if cond == "intact":
            out.append((mat, (x0, base, z0), (x1, top, z1)))
            out.append(("snow", (x0, top, z0), (x1, top + 0.3, z1)))
            continue
        mat = "burnt" if cond == "burned" else mat
        walls = {"+z": ((x0, z1 - t_wall), (x1, z1)), "-z": ((x0, z0), (x1, z0 + t_wall)),
                 "+x": ((x1 - t_wall, z0), (x1, z1)), "-x": ((x0, z0), (x0 + t_wall, z1))}
        facade = ["+z", "+x", "-z", "-x"][quarter]   # local +Z is the street facade
        for side, (lo, hi) in walls.items():
            hh = h
            if cond == "partly_collapsed" and side != facade:
                hh = h * (0.35 if side in ("+x", "-x") else 0.5)
            elif cond == "shelled" and side != facade:
                hh = h * 0.85
            out.append((mat, (lo[0], base, lo[1]), (hi[0], base + 1.0 + hh, hi[1])))
    return out


def render(workshop, prefix, only):
    import bpy
    import numpy as np
    from mathutils import Vector

    ws = ROOT / workshop
    heights = np.load(ws / "heights.npy").astype(np.float64)
    manifest = json.loads((ws / "manifest.v1.json").read_text())
    res, sp = int(manifest["terrain"]["res"]), float(manifest["terrain"]["spacing"])
    half = (res - 1) * sp / 2.0
    edits = json.loads((ws / "manifest_edits.json").read_text())
    feats = json.loads((ws / "features.json").read_text())[EDITS_KEY]
    pad_y = {p["id"]: p["pad_y"] for p in feats["plots"]}

    def ground(x, z):
        return float(heights[int(round((z + half) / sp)), int(round((x + half) / sp))])

    def b(x, y, z):   # engine (x, y up, z south) -> Blender (x, -z, y)
        return (x, -z, y)

    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene

    def material(name, rgb):
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        bsdf = m.node_tree.nodes.get("Principled BSDF")
        bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Roughness"].default_value = 0.9
        return m

    mats = {"snow": material("snow", (0.80, 0.81, 0.83)), "brick": material("brick", (0.30, 0.25, 0.22)),
            "burnt": material("burnt", (0.11, 0.10, 0.09)), "timber": material("timber", (0.24, 0.19, 0.14)),
            "workshop": material("workshop", (0.28, 0.27, 0.25))}

    def add_mesh(name, verts, faces, mat):
        me = bpy.data.meshes.new(name)
        me.from_pydata(verts, [], faces)
        me.update()
        ob = bpy.data.objects.new(name, me)
        ob.data.materials.append(mat)
        scene.collection.objects.link(ob)
        return me

    ext = feats["ring"]
    c0 = max(0, int((ext[0] - 150.0 + half) / sp))
    c1 = min(res - 1, int((ext[1] + 150.0 + half) / sp) + 1)
    r0 = max(0, int((ext[2] - 150.0 + half) / sp))
    r1 = min(res - 1, int((ext[3] + 150.0 + half) / sp) + 1)
    nc = c1 - c0 + 1
    verts = [b(c * sp - half, float(heights[r, c]), r * sp - half) for r in range(r0, r1 + 1) for c in range(c0, c1 + 1)]
    faces = [(i * nc + j, i * nc + j + 1, (i + 1) * nc + j + 1, (i + 1) * nc + j)
             for i in range(r1 - r0) for j in range(c1 - c0)]
    me = add_mesh("terrain", verts, faces, mats["snow"])
    for poly in me.polygons:
        poly.use_smooth = True
    boxes = {}
    for mat, lo, hi in masses(edits["building_instances"], pad_y, ground):
        boxes.setdefault(mat, []).append((lo, hi))
    for mat, lst in boxes.items():
        verts, faces = [], []
        for lo, hi in lst:
            k = len(verts)
            for (x, y, z) in [(lo[0], lo[1], lo[2]), (hi[0], lo[1], lo[2]), (hi[0], lo[1], hi[2]), (lo[0], lo[1], hi[2]),
                              (lo[0], hi[1], lo[2]), (hi[0], hi[1], lo[2]), (hi[0], hi[1], hi[2]), (lo[0], hi[1], hi[2])]:
                verts.append(b(x, y, z))
            faces += [(k, k + 1, k + 2, k + 3), (k + 4, k + 7, k + 6, k + 5), (k, k + 4, k + 5, k + 1),
                      (k + 1, k + 5, k + 6, k + 2), (k + 2, k + 6, k + 7, k + 3), (k + 3, k + 7, k + 4, k)]
        add_mesh("mass_" + mat, verts, faces, mats[mat])

    world = bpy.data.worlds.new("overcast")
    world.use_nodes = True
    bg = world.node_tree.nodes.get("Background")
    bg.inputs["Color"].default_value = (0.55, 0.57, 0.60, 1.0)
    scene.world = world
    sun = bpy.data.lights.new("sun", "SUN")
    sun.energy = 1.2
    sun.angle = math.radians(8.0)
    sun_ob = bpy.data.objects.new("sun", sun)
    az, el = math.radians(110.0), math.radians(20.0)   # clockwise from north (+y in Blender)
    to_sun = Vector((math.sin(az) * math.cos(el), math.cos(az) * math.cos(el), math.sin(el)))
    sun_ob.rotation_euler = (-to_sun).to_track_quat("-Z", "Y").to_euler()
    scene.collection.objects.link(sun_ob)
    cam = bpy.data.cameras.new("cam")
    cam.sensor_fit = "VERTICAL"
    cam.angle = math.radians(FOV_Y_DEG)
    cam.clip_end = 5000.0
    cam_ob = bpy.data.objects.new("cam", cam)
    scene.collection.objects.link(cam_ob)
    scene.camera = cam_ob
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 24
    scene.cycles.use_denoising = True
    scene.cycles.seed = 0
    scene.render.resolution_x, scene.render.resolution_y = 1280, 720
    scene.render.image_settings.file_format = "PNG"
    scene.view_settings.view_transform = "AgX"
    for name, (x, y, z, yaw, pitch) in VIEWS.items():
        if only and name not in only:
            continue
        yw, pt = math.radians(yaw), math.radians(pitch)
        fwd = Vector((math.sin(yw) * math.cos(pt), math.cos(yw) * math.cos(pt), math.sin(pt)))
        cam_ob.location = b(x, y, z)
        cam_ob.rotation_euler = fwd.to_track_quat("-Z", "Y").to_euler()
        scene.render.filepath = str(ROOT / (prefix + name + ".png"))
        bpy.ops.render.render(write_still=True)
        print("[massing] wrote", scene.render.filepath)


def main(argv):
    if "--commands" in argv:
        commands()
        return
    rest = args_after_dashes(argv)
    workshop, prefix, only = WORKSHOP, "captures/urban_district_massing_", []
    i = 0
    while i < len(rest):
        if rest[i] == "--workshop":
            workshop, i = rest[i + 1], i + 2
        elif rest[i] == "--prefix":
            prefix, i = rest[i + 1], i + 2
        else:
            only.append(rest[i])
            i += 1
    unknown = [v for v in only if v not in VIEWS]
    if unknown:
        raise SystemExit(f"unknown views {unknown}; known: {sorted(VIEWS)}")
    render(workshop, prefix, only)


if __name__ == "__main__":
    main(sys.argv)
