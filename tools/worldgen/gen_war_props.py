"""Generate the Eastern-Front war props the PSX packs do not contain.

    blender --background --python tools/worldgen/gen_war_props.py -- assets/warprops

The commercial PSX libraries in `assets/pizza` are generic urban/nature kits: they have no
vehicles, no field artillery, no obstacles, no draught animals and no grave markers. Those are
exactly the objects that make an Eastern-Front battlefield legible, so they are authored here as
deterministic low-poly geometry in the same visual register (flat materials, 60-500 tris, no
normal maps) and consumed by `gen_world.py`'s warscape layer.

Every prop is a *material* object that a real photograph of the 1941-43 Eastern Front would show.
Provenance for the classes, kept here rather than in player-facing text per CLAUDE.md:

  panje wagon      the ubiquitous Russian one-horse farm cart both armies ran on once motor
                   transport froze; German units requisitioned them in tens of thousands.
  draught horse    the Sixth Army took ~52,000 horses into the Stalingrad pocket and ate them;
                   survivor accounts describe axes rebounding off carcasses frozen stone-hard
                   (Bechler, in Beevor, *Stalingrad*), hence the butchered variant.
  Czech hedgehog   welded rail-steel anti-tank obstacle, Soviet standard issue from 1941.
  grave marker     birch cross with the dead man's helmet, the standard German field burial;
                   a rifle planted bayonet-down marks a body awaiting collection.
  supply canister  Luftwaffe Abwurfbehälter, the airlift's delivery container.
  field gun        divisional/anti-tank piece abandoned with a broken wheel or trail.

Axis convention: model Z-up in Blender; the default glTF +Y-up export yields the engine's
`engine = (blender_x, blender_z, -blender_y)`. Origins sit at ground contact so `place_ground`
can drop them straight onto the heightfield.
"""

import bmesh
import bpy
import math
import os
import sys
from mathutils import Euler, Vector


def as_rot(rot):
    """Accept a plain (rx, ry, rz) tuple as well as a real Euler/Quaternion."""
    if rot is None:
        return None
    if isinstance(rot, (tuple, list)):
        return Euler(rot, "XYZ")
    return rot

# ---------------------------------------------------------------------------- deterministic noise


def h01(n: int) -> float:
    """Same integer hash the rest of the project uses; keeps props stable across regenerations."""
    n = (n * 2654435761) & 0xFFFFFFFF
    n ^= n >> 13
    n = (n * 1274126177) & 0xFFFFFFFF
    return ((n >> 8) & 0xFFFFFF) / float(0xFFFFFF)


def jitter(n: int, amt: float) -> float:
    return (h01(n) - 0.5) * 2.0 * amt


# ---------------------------------------------------------------------------- materials

# Base colours are linear-ish sRGB values chosen to read under the game's cold grade and
# posterisation: nothing fully black, nothing saturated.
MATERIALS = {
    "steel_rust":   ((0.223, 0.128, 0.078, 1.0), 0.86, 0.55),
    "steel_burnt":  ((0.072, 0.066, 0.063, 1.0), 0.94, 0.42),
    "steel_grey":   ((0.183, 0.190, 0.183, 1.0), 0.72, 0.70),
    "steel_dark":   ((0.108, 0.112, 0.112, 1.0), 0.80, 0.65),
    "wood_weather": ((0.239, 0.196, 0.143, 1.0), 0.92, 0.0),
    "wood_dark":    ((0.148, 0.117, 0.083, 1.0), 0.93, 0.0),
    "wood_birch":   ((0.601, 0.573, 0.506, 1.0), 0.88, 0.0),
    "wood_char":    ((0.055, 0.050, 0.047, 1.0), 0.96, 0.0),
    "canvas":       ((0.318, 0.301, 0.236, 1.0), 0.95, 0.0),
    "canvas_dirty": ((0.212, 0.199, 0.161, 1.0), 0.96, 0.0),
    "feldgrau":     ((0.152, 0.170, 0.147, 1.0), 0.93, 0.0),
    "khaki":        ((0.231, 0.199, 0.132, 1.0), 0.93, 0.0),
    "hide_bay":     ((0.171, 0.108, 0.070, 1.0), 0.90, 0.0),
    "hide_dark":    ((0.098, 0.072, 0.055, 1.0), 0.90, 0.0),
    "flesh_frozen": ((0.284, 0.155, 0.135, 1.0), 0.84, 0.0),
    "brass":        ((0.382, 0.297, 0.126, 1.0), 0.48, 0.85),
    "rubber":       ((0.055, 0.055, 0.058, 1.0), 0.95, 0.0),
    "earth_spoil":  ((0.145, 0.121, 0.096, 1.0), 0.97, 0.0),
    "paper":        ((0.520, 0.499, 0.451, 1.0), 0.94, 0.0),
    "snow_dirty":   ((0.545, 0.556, 0.568, 1.0), 0.88, 0.0),
}

_mat_cache = {}


def mat(name: str):
    if name in _mat_cache:
        return _mat_cache[name]
    base, rough, metal = MATERIALS[name]
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = base
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    _mat_cache[name] = m
    return m


# ---------------------------------------------------------------------------- mesh helpers


def new_mesh(name: str):
    me = bpy.data.meshes.new(name)
    ob = bpy.data.objects.new(name, me)
    bpy.context.collection.objects.link(ob)
    return ob


def box(bm, cx, cy, cz, sx, sy, sz, rot=None, mat_index=0):
    """Axis-aligned (optionally rotated) box centred at (cx,cy,cz) with full sizes sx,sy,sz."""
    rot = as_rot(rot)
    verts = []
    for ix in (-0.5, 0.5):
        for iy in (-0.5, 0.5):
            for iz in (-0.5, 0.5):
                v = Vector((ix * sx, iy * sy, iz * sz))
                if rot is not None:
                    v.rotate(rot)
                verts.append(bm.verts.new((cx + v.x, cy + v.y, cz + v.z)))
    idx = [(0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1),
           (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)]
    for f in idx:
        try:
            face = bm.faces.new([verts[i] for i in f])
            face.material_index = mat_index
        except ValueError:
            pass
    return verts


def cyl(bm, cx, cy, cz, r, h, axis="z", seg=8, rot=None, mat_index=0):
    """Low-segment cylinder; PSX kits use 6-10 sides, so 8 matches the library."""
    rot = as_rot(rot)
    ring_a, ring_b = [], []
    for i in range(seg):
        a = math.tau * i / seg
        u, v = math.cos(a) * r, math.sin(a) * r
        if axis == "z":
            pa, pb = Vector((u, v, -h / 2)), Vector((u, v, h / 2))
        elif axis == "y":
            pa, pb = Vector((u, -h / 2, v)), Vector((u, h / 2, v))
        else:
            pa, pb = Vector((-h / 2, u, v)), Vector((h / 2, u, v))
        if rot is not None:
            pa.rotate(rot)
            pb.rotate(rot)
        ring_a.append(bm.verts.new((cx + pa.x, cy + pa.y, cz + pa.z)))
        ring_b.append(bm.verts.new((cx + pb.x, cy + pb.y, cz + pb.z)))
    for i in range(seg):
        j = (i + 1) % seg
        try:
            f = bm.faces.new([ring_a[i], ring_a[j], ring_b[j], ring_b[i]])
            f.material_index = mat_index
        except ValueError:
            pass
    for ring in (ring_a, list(reversed(ring_b))):
        try:
            f = bm.faces.new(ring)
            f.material_index = mat_index
        except ValueError:
            pass
    return ring_a, ring_b


def finish(ob, mats):
    """Attach materials, recalculate normals, and drop the origin to ground contact."""
    for name in mats:
        ob.data.materials.append(mat(name))
    me = ob.data
    bm = bmesh.new()
    bm.from_mesh(me)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(me)
    bm.free()
    me.shade_flat()
    return ob


def build(name, fn, mats):
    ob = new_mesh(name)
    bm = bmesh.new()
    fn(bm)
    bm.to_mesh(ob.data)
    bm.free()
    return finish(ob, mats)


# ---------------------------------------------------------------------------- the props


def p_hedgehog(bm):
    """Czech hedgehog: three lengths of angle iron welded through a common centre."""
    L, T = 1.45, 0.085
    for i, ax in enumerate(((1, 1, 0.55), (1, -1, -0.55), (-0.5, 0.35, 1.0))):
        d = Vector(ax).normalized()
        rot = Vector((0, 0, 1)).rotation_difference(d)
        cyl(bm, 0, 0, 0, T, L, axis="z", seg=4, rot=rot.to_euler(), mat_index=0)
    box(bm, 0, 0, 0, 0.16, 0.16, 0.16, mat_index=0)


def p_grave_cross(bm):
    """Birch cross with a steel helmet hung on the head — the standard German field burial."""
    box(bm, 0, 0, 0.44, 0.075, 0.062, 0.88, mat_index=0)          # upright
    box(bm, 0, 0, 0.66, 0.46, 0.055, 0.070, mat_index=0)          # crossbar
    # helmet: shallow dome of two stacked rings plus a brim
    cyl(bm, 0, 0, 0.90, 0.125, 0.085, axis="z", seg=8, mat_index=1)
    cyl(bm, 0, 0, 0.955, 0.088, 0.055, axis="z", seg=8, mat_index=1)
    cyl(bm, 0, 0, 0.862, 0.146, 0.018, axis="z", seg=8, mat_index=1)
    box(bm, 0, 0, 0.04, 0.30, 0.30, 0.08, mat_index=2)            # disturbed earth at the foot


def p_rifle_marker(bm):
    """A rifle driven bayonet-first into the ground: a body here, awaiting collection."""
    tilt = math.radians(12)
    rot = (tilt, 0, 0)
    cyl(bm, 0, 0, 0.62, 0.021, 1.02, axis="z", seg=6, rot=rot, mat_index=0)   # barrel
    box(bm, 0, 0.03, 0.30, 0.055, 0.075, 0.42, rot=rot, mat_index=1)          # stock
    cyl(bm, 0, 0, 1.18, 0.012, 0.30, axis="z", seg=4, rot=rot, mat_index=0)   # bayonet up
    cyl(bm, 0, -0.02, 1.30, 0.115, 0.075, axis="z", seg=8, mat_index=2)       # helmet on top
    cyl(bm, 0, -0.02, 1.258, 0.134, 0.016, axis="z", seg=8, mat_index=2)


def p_horse_dead(bm, butchered=False):
    """Draught horse dead on its side, legs out stiff. ~2.2 m nose to tail."""
    box(bm, 0, 0, 0.36, 0.62, 1.35, 0.60, mat_index=0)                   # barrel of the body
    box(bm, 0, 0.82, 0.40, 0.44, 0.42, 0.44, mat_index=0)                # shoulder
    box(bm, 0, -0.78, 0.34, 0.40, 0.40, 0.40, mat_index=0)               # haunch
    # neck and head, thrown back as rigor sets
    box(bm, 0, 1.12, 0.30, 0.30, 0.52, 0.30, rot=(math.radians(-24), 0, 0), mat_index=0)
    box(bm, 0, 1.44, 0.20, 0.22, 0.40, 0.22, rot=(math.radians(-38), 0, 0), mat_index=1)
    # four legs projecting sideways, slightly splayed
    for i, (ly, lz, ang) in enumerate(((0.72, 0.58, 14), (0.60, 0.50, -8),
                                       (-0.64, 0.56, 10), (-0.74, 0.48, -12))):
        cyl(bm, 0.30 + i * 0.01, ly, lz, 0.062, 0.80, axis="x",
            rot=(0, 0, math.radians(ang)), mat_index=1)
        cyl(bm, 0.72, ly + jitter(i * 7, 0.05), lz + 0.04, 0.048, 0.36, axis="x", mat_index=1)
    if butchered:
        # a haunch cut away with a saw, the cut face frozen dark
        box(bm, 0.0, -0.80, 0.30, 0.46, 0.30, 0.42, mat_index=2)
        box(bm, 0.24, -0.96, 0.22, 0.10, 0.22, 0.26, mat_index=2)


def p_horse_dead_butchered(bm):
    p_horse_dead(bm, butchered=True)


def p_panje_wagon(bm, wrecked=False):
    """Russian one-horse farm cart: plank bed, four spoked wheels, two shafts."""
    tilt = (math.radians(19), 0, math.radians(7)) if wrecked else None
    bedz = 0.52 if not wrecked else 0.40
    for i in range(5):                                                    # plank bed
        box(bm, 0, -0.60 + i * 0.30, bedz, 1.02, 0.26, 0.05,
            rot=tilt, mat_index=0)
    box(bm, 0, 0, bedz + 0.16, 1.06, 1.52, 0.05, rot=tilt, mat_index=0)   # side rails
    for sy in (-0.74, 0.74):                                              # end boards
        box(bm, 0, sy, bedz + 0.16, 1.02, 0.05, 0.30, rot=tilt, mat_index=0)
    for sx in (-0.52, 0.52):
        box(bm, sx, 0, bedz + 0.18, 0.05, 1.50, 0.28, rot=tilt, mat_index=0)
    wheels = ((-0.58, 0.52), (0.58, 0.52), (-0.58, -0.58), (0.58, -0.58))
    for i, (wx, wy) in enumerate(wheels):
        if wrecked and i == 1:
            # thrown wheel lying flat beside the cart
            cyl(bm, wx + 0.95, wy - 0.30, 0.05, 0.40, 0.07, axis="z", seg=10, mat_index=1)
            continue
        cyl(bm, wx, wy, 0.40, 0.40, 0.08, axis="x", seg=10, mat_index=1)
        for s in range(6):                                                # spokes
            a = math.tau * s / 6
            cyl(bm, wx, wy + math.cos(a) * 0.20, 0.40 + math.sin(a) * 0.20,
                0.022, 0.40, axis="z", seg=4,
                rot=(a, math.pi / 2, 0), mat_index=1)
    for sx in (-0.34, 0.34):                                              # draught shafts
        cyl(bm, sx, 1.42, 0.46, 0.038, 1.60, axis="y", mat_index=0)


def p_panje_wagon_wrecked(bm):
    p_panje_wagon(bm, wrecked=True)


def _truck_chassis(bm, mi_frame, mi_wheel):
    for sx in (-0.52, 0.52):
        box(bm, sx, -0.20, 0.52, 0.12, 4.30, 0.16, mat_index=mi_frame)
    for i, (wx, wy) in enumerate(((-0.92, 1.30), (0.92, 1.30),
                                  (-0.92, -1.28), (0.92, -1.28),
                                  (-0.92, -1.74), (0.92, -1.74))):
        cyl(bm, wx, wy, 0.46, 0.46, 0.24, axis="x", seg=10, mat_index=mi_wheel)


def p_truck_burned(bm):
    """Three-ton lorry burned to the frame: cab skeleton, bed ribs, tyres gone to wire."""
    _truck_chassis(bm, 0, 0)
    box(bm, 0, 1.28, 1.05, 1.80, 1.30, 0.90, mat_index=0)          # cab shell
    box(bm, 0, 1.94, 1.28, 1.70, 0.10, 0.62, mat_index=0)          # windscreen frame, glass gone
    box(bm, 0, 2.34, 0.92, 1.40, 0.72, 0.64, mat_index=0)          # engine bay
    for i in range(6):                                              # bed ribs, planking burned off
        box(bm, 0, -0.30 - i * 0.42, 1.12, 1.84, 0.07, 0.62, mat_index=1)
    box(bm, 0, -1.55, 0.70, 1.86, 2.60, 0.06, mat_index=1)         # bed floor
    for sx in (-0.95, 0.95):
        box(bm, sx, -1.55, 0.86, 0.06, 2.60, 0.30, mat_index=1)


def p_truck_he_hit(bm):
    """Lorry opened by a high-explosive shell: bed torn apart, cargo thrown, frame folded."""
    _truck_chassis(bm, 0, 0)
    box(bm, 0, 1.28, 1.02, 1.80, 1.28, 0.86, rot=(0, 0, math.radians(-6)), mat_index=0)
    box(bm, 0, 2.30, 0.92, 1.40, 0.70, 0.62, mat_index=0)
    # the bed: three surviving ribs, then splintered stubs leaning outward
    for i in range(3):
        box(bm, 0, -0.34 - i * 0.44, 1.10, 1.80, 0.07, 0.58, mat_index=1)
    for i, (sx, ang) in enumerate(((-0.92, 34), (0.88, -41), (-0.80, 58))):
        box(bm, sx, -1.70 - i * 0.30, 0.95, 0.08, 0.66, 0.54,
            rot=(0, math.radians(ang), 0), mat_index=1)
    box(bm, 0.10, -1.50, 0.62, 1.70, 1.70, 0.06,
        rot=(math.radians(7), 0, 0), mat_index=1)
    # spilled cargo: crates burst out of the back into the snow
    for k in range(5):
        cx = jitter(k * 13 + 1, 1.5)
        cy = -2.6 - h01(k * 17) * 2.4
        box(bm, cx, cy, 0.18 + h01(k * 23) * 0.10, 0.44, 0.38, 0.32,
            rot=(0, 0, h01(k * 29) * math.tau), mat_index=2)


def p_field_gun_wrecked(bm):
    """Divisional gun abandoned with a wheel off and one trail leg buckled."""
    cyl(bm, 0, 0.95, 0.95, 0.072, 2.30, axis="y", mat_index=0)          # barrel
    cyl(bm, 0, 1.95, 0.95, 0.098, 0.34, axis="y", mat_index=0)          # muzzle thickening
    box(bm, 0, -0.22, 0.88, 0.46, 0.66, 0.40, mat_index=0)              # breech/cradle
    box(bm, 0, 0.12, 1.16, 1.36, 0.09, 0.78,
        rot=(math.radians(-14), 0, 0), mat_index=1)                     # gun shield
    cyl(bm, -0.74, 0.02, 0.56, 0.56, 0.14, axis="x", seg=10, mat_index=1)  # left wheel
    cyl(bm, 0.86, -0.34, 0.07, 0.56, 0.14, axis="z", seg=10, mat_index=1)  # right wheel, thrown
    box(bm, -0.30, -1.20, 0.34, 0.14, 1.70, 0.16,
        rot=(0, 0, math.radians(9)), mat_index=0)                       # trail leg
    box(bm, 0.34, -1.05, 0.26, 0.14, 1.50, 0.16,
        rot=(math.radians(11), 0, math.radians(-21)), mat_index=0)      # buckled leg


def p_supply_canister(bm):
    """Luftwaffe Abwurfbehälter with its parachute collapsed alongside."""
    cyl(bm, 0, 0, 0.26, 0.26, 0.52, axis="y", seg=10, mat_index=0)
    cyl(bm, 0, 0.76, 0.26, 0.20, 0.36, axis="y", seg=10, mat_index=0)
    box(bm, 0, -0.34, 0.26, 0.30, 0.20, 0.30, mat_index=0)
    # canopy: a shallow, irregular fan of cloth panels lying in the snow
    for k in range(9):
        a = math.tau * k / 9
        d = 0.95 + h01(k * 31) * 0.85
        box(bm, math.cos(a) * d, -1.15 + math.sin(a) * d * 0.7,
            0.035 + h01(k * 37) * 0.03, 0.62, 0.62, 0.045,
            rot=(0, 0, a + jitter(k * 41, 0.4)), mat_index=1)


def p_fuel_drum_burst(bm):
    """Ruptured 200-litre drum, split down one seam."""
    cyl(bm, 0, 0, 0.30, 0.29, 0.60, axis="y", seg=10, mat_index=0)
    box(bm, 0.26, 0.10, 0.34, 0.10, 0.34, 0.30,
        rot=(0, math.radians(26), 0), mat_index=0)
    cyl(bm, 0, 0, 0.30, 0.31, 0.05, axis="y", seg=10, mat_index=0)


def p_ammo_crate_burst(bm):
    """Ammunition crate broken open, rounds spilled into the snow."""
    box(bm, 0, 0, 0.16, 0.86, 0.40, 0.32, mat_index=0)
    box(bm, 0.12, 0.34, 0.06, 0.80, 0.34, 0.04,
        rot=(0, 0, math.radians(17)), mat_index=0)          # lid thrown off
    for k in range(7):
        cyl(bm, jitter(k * 11 + 3, 0.62), 0.30 + h01(k * 19) * 0.50, 0.035,
            0.034, 0.17, axis="y", seg=6,
            rot=(0, 0, h01(k * 23) * math.tau), mat_index=1)


def p_sandbag_wall(bm):
    """Three courses of sandbags, settled and irregular."""
    for course in range(3):
        n = 5 - (course > 1)
        for i in range(n):
            x = (i - (n - 1) / 2) * 0.46 + jitter(course * 31 + i, 0.05)
            box(bm, x, jitter(course * 17 + i, 0.04), 0.11 + course * 0.20,
                0.44, 0.28, 0.20,
                rot=(0, 0, jitter(course * 13 + i, 0.16)), mat_index=0)


def p_wire_stake(bm):
    """Barbed-wire picket with three sagging strands — one bay of an entanglement."""
    cyl(bm, 0, 0, 0.52, 0.030, 1.04, axis="z", seg=5, mat_index=0)
    for strand in range(3):
        z = 0.34 + strand * 0.26
        for seg in range(6):
            t0 = seg / 6.0
            y0 = -1.5 + t0 * 3.0
            sag = math.sin(t0 * math.pi) * 0.10
            cyl(bm, jitter(strand * 7 + seg, 0.03), y0 + 0.25, z - sag,
                0.010, 0.52, axis="y", seg=4, mat_index=0)


def p_stretcher(bm, occupied=False):
    """Field stretcher: two poles, canvas, low feet."""
    for sx in (-0.29, 0.29):
        cyl(bm, sx, 0, 0.17, 0.028, 2.20, axis="y", seg=6, mat_index=0)
    box(bm, 0, 0, 0.17, 0.56, 1.70, 0.035, mat_index=1)
    for sy in (-0.72, 0.72):
        for sx in (-0.29, 0.29):
            cyl(bm, sx, sy, 0.08, 0.022, 0.17, axis="z", seg=4, mat_index=0)
    if occupied:
        # a body under a blanket: the shape alone carries it, no anatomy needed
        box(bm, 0, -0.10, 0.30, 0.46, 1.58, 0.24, mat_index=2)
        box(bm, 0, 0.76, 0.27, 0.26, 0.28, 0.22, mat_index=2)
        cyl(bm, 0, -0.96, 0.24, 0.10, 0.22, axis="y", seg=6, mat_index=2)


def p_stretcher_occupied(bm):
    p_stretcher(bm, occupied=True)


def p_helmet_ground(bm):
    """A single steel helmet upturned in the snow."""
    cyl(bm, 0, 0, 0.075, 0.128, 0.115, axis="z", seg=9, mat_index=0)
    cyl(bm, 0, 0, 0.022, 0.150, 0.020, axis="z", seg=9, mat_index=0)


def p_chimney_stack(bm):
    """A Russian stove chimney standing alone over the ash of a burned izba — the
    single most recognisable image of scorched earth on the Eastern Front."""
    box(bm, 0, 0, 0.95, 0.86, 0.86, 1.90, mat_index=0)              # stove mass
    box(bm, 0.04, 0.02, 2.35, 0.52, 0.52, 0.94, mat_index=0)        # flue
    box(bm, -0.02, 0.0, 3.00, 0.40, 0.40, 0.42,
        rot=(0, math.radians(4), 0), mat_index=0)                   # broken top
    # ash and charred sill timbers where the walls stood
    for k in range(10):
        a = math.tau * k / 10
        d = 1.9 + h01(k * 43) * 0.8
        box(bm, math.cos(a) * d, math.sin(a) * d, 0.055,
            1.20, 0.16, 0.11, rot=(0, 0, a + math.pi / 2), mat_index=1)
    box(bm, 0, 0, 0.02, 4.6, 4.6, 0.035, mat_index=2)               # ash footprint


def p_grave_mound(bm):
    """A shallow field grave: heaped earth, one board, a helmet."""
    box(bm, 0, 0, 0.13, 0.95, 1.95, 0.26, mat_index=0)
    box(bm, 0, 0.98, 0.26, 0.40, 0.05, 0.36, mat_index=1)
    cyl(bm, 0, 0.70, 0.31, 0.118, 0.085, axis="z", seg=8, mat_index=2)


def p_at_ditch_spoil(bm):
    """Spoil bank thrown up along an anti-tank ditch, cut by hand labour."""
    for k in range(7):
        y = -3.0 + k * 1.0
        box(bm, jitter(k * 53, 0.22), y, 0.30 + h01(k * 59) * 0.16,
            1.5 + h01(k * 61) * 0.5, 1.05, 0.62 + h01(k * 67) * 0.3,
            rot=(0, 0, jitter(k * 71, 0.2)), mat_index=0)


PROPS = [
    ("hedgehog",            p_hedgehog,            ["steel_rust"]),
    ("grave_cross",         p_grave_cross,         ["wood_birch", "steel_grey", "earth_spoil"]),
    ("rifle_marker",        p_rifle_marker,        ["steel_dark", "wood_dark", "steel_grey"]),
    ("horse_dead",          p_horse_dead,          ["hide_bay", "hide_dark", "flesh_frozen"]),
    ("horse_dead_butchered", p_horse_dead_butchered, ["hide_bay", "hide_dark", "flesh_frozen"]),
    ("panje_wagon",         p_panje_wagon,         ["wood_weather", "wood_dark"]),
    ("panje_wagon_wrecked", p_panje_wagon_wrecked, ["wood_weather", "wood_dark"]),
    ("truck_burned",        p_truck_burned,        ["steel_burnt", "wood_char"]),
    ("truck_he_hit",        p_truck_he_hit,        ["steel_burnt", "wood_char", "wood_weather"]),
    ("field_gun_wrecked",   p_field_gun_wrecked,   ["steel_grey", "steel_rust"]),
    ("supply_canister",     p_supply_canister,     ["steel_grey", "canvas"]),
    ("fuel_drum_burst",     p_fuel_drum_burst,     ["steel_rust"]),
    ("ammo_crate_burst",    p_ammo_crate_burst,    ["wood_weather", "brass"]),
    ("sandbag_wall",        p_sandbag_wall,        ["canvas_dirty"]),
    ("wire_stake",          p_wire_stake,          ["steel_rust"]),
    ("stretcher",           p_stretcher,           ["wood_weather", "canvas_dirty"]),
    ("stretcher_occupied",  p_stretcher_occupied,  ["wood_weather", "canvas_dirty", "feldgrau"]),
    ("helmet_ground",       p_helmet_ground,       ["steel_grey"]),
    ("chimney_stack",       p_chimney_stack,       ["steel_dark", "wood_char", "earth_spoil"]),
    ("grave_mound",         p_grave_mound,         ["earth_spoil", "wood_weather", "steel_grey"]),
    ("at_ditch_spoil",      p_at_ditch_spoil,      ["earth_spoil"]),
]


def clear_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    _mat_cache.clear()


def main():
    argv = sys.argv
    outdir = argv[argv.index("--") + 1] if "--" in argv else "assets/warprops"
    os.makedirs(outdir, exist_ok=True)

    report = []
    for name, fn, mats in PROPS:
        clear_scene()
        ob = build(name, fn, mats)
        bpy.ops.object.select_all(action="DESELECT")
        ob.select_set(True)
        bpy.context.view_layer.objects.active = ob
        path = os.path.join(outdir, f"{name}.glb")
        bpy.ops.export_scene.gltf(
            filepath=path,
            export_format="GLB",
            use_selection=True,
            export_yup=True,
            export_apply=True,
        )
        tris = sum(len(p.vertices) - 2 for p in ob.data.polygons)
        report.append((name, tris, len(ob.data.vertices)))
        print(f"[warprops] {name}: {tris} tris, {len(ob.data.vertices)} verts -> {path}")

    total = sum(r[1] for r in report)
    print(f"[warprops] wrote {len(report)} props, {total} triangles total, into {outdir}")


if __name__ == "__main__":
    main()
