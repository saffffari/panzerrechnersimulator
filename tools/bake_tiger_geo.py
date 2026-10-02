# Re-bake the Tiger hull/turret/gun GLBs from the interior-complete .blend (Phase 2).
#
#   blender --background <blend> --python tools/bake_tiger_geo.py -- <out_dir>
#
# Groups every mesh by subtree — gun (Gun_pivot descendants), turret (Turret_Pivot descendants +
# turret orphan roots), hull (the rest) — and exports three GLBs with rig-matched origins (hull at
# ground contact, turret at Turret_Pivot, gun at Gun_pivot), Blender default glTF Y-up so
# engine = (bx, bz, -by). The GLB loader merges all meshes in a file, so no join needed.
#
# EXCLUDES the real cupola-hatch lid group — the engine articulates a stand-in box for it, so baking
# the real lid in would double it. (Phase 5 swaps the real lid in and drops the box.) All other
# would-be-animated parts (other hatches, wheels, TPZ needle, shells) bake in STATIC for now: they
# render in place; articulation comes later.

import bpy, mathutils, sys, os
from pathlib import Path

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
REPO_ROOT = Path(__file__).resolve().parents[1]
OUT = argv[0] if argv else str(REPO_ROOT / "assets/tanks/tiger/baked")

def O(n): return bpy.data.objects.get(n)

# ── VISION HOLES: AUTHORED IN THE .BLEND, NOT CUT HERE (policy change 2026-07-06) ──────────────
# The bake used to bore all 12 direct-view ports through the armor and drop their glass. That's
# retired: the ONLY through-holes are the THREE the user hand-booleaned in the authored .blend —
# the LEFT and RIGHT turret vision ports and the TZF gun-optic bore — and every glass block
# (`periscope glass` material) now EXPORTS with the bake. The remaining ports are periscopes:
# their blocks stay glassed and get the Schutzglas treatment engine-side (stipple-transparency
# GLASS mesh mode + sun glow — docs/EMBODIED_INTERIOR.md "Vision-slit glass blocks"), NOT holes.
# This function is retired rather than deleted so the port list + cutter survive for reference
# (e.g. if a future variant model arrives unbooleaned).
DIRECT_PORTS = ["Hull_0241", "Hull_0189", "Hull_0300", "Turret_0069", "Turret_0079",
                "Turret_0129", "Turret_0139", "Turret_0140", "Turret_0163", "Turret_0164",
                "Turret_0205", "Turret_0206"]
CUT_AT_BAKE = False  # authored cuts only — flip ONLY for an unbooleaned source model

def _wbb(o):
    pts = [o.matrix_world @ mathutils.Vector(c) for c in o.bound_box]
    mn = mathutils.Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    mx = mathutils.Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    return mn, mx

def cut_vision_holes():
    glassy = {o.name for o in bpy.data.objects if o.type == 'MESH'
              and any(ms and ms.name == "periscope glass" for ms in o.data.materials)}
    for name in DIRECT_PORTS:
        g = O(name)
        if not g:
            print(f"HOLE {name}: MISSING — skipped")
            continue
        # facing = area-weighted glass-face normal, flipped outward (away from the local centre:
        # the cupola ring for the cupola blocks, the hull centreline otherwise)
        mw3 = g.matrix_world.to_3x3()
        n = mathutils.Vector((0, 0, 0))
        for p in g.data.polygons:
            n += (mw3 @ p.normal) * max(p.area, 1e-9)
        n.normalize()
        gl = g.matrix_world.translation
        centre = mathutils.Vector((0.56, 0.42, gl.z)) if name in DIRECT_PORTS[5:] else mathutils.Vector((0.0, 0.0, gl.z))
        outv = gl - centre
        if outv.length > 1e-6 and n.dot(outv.normalized()) < 0:
            n = -n
        mn, mx = _wbb(g)
        c = (mn + mx) / 2
        u = n.cross(mathutils.Vector((0, 0, 1)))
        if u.length < 0.1: u = n.cross(mathutils.Vector((0, 1, 0)))
        u.normalize(); v = n.cross(u).normalized()
        ext = mx - mn
        hu = max(abs(ext.dot(u)) / 2, 0.02) * 0.92
        hv = max(abs(ext.dot(v)) / 2, 0.02) * 0.92
        bpy.ops.mesh.primitive_cube_add(size=1)
        cut = bpy.context.active_object
        cut.name = f"CUT_{name}"
        cc = c + n * 0.35  # reach 1.0 m out / 0.3 m in
        cut.matrix_world = mathutils.Matrix(((u.x, v.x, n.x, cc.x),
                                             (u.y, v.y, n.y, cc.y),
                                             (u.z, v.z, n.z, cc.z),
                                             (0, 0, 0, 1))) \
            @ mathutils.Matrix.Diagonal((hu * 2, hv * 2, 1.3, 1.0))
        cmn, cmx = _wbb(cut)
        n_cut = 0
        for o in list(bpy.data.objects):
            if o.type != 'MESH' or o.name in glassy or o.name.startswith("CUT_"): continue
            omn, omx = _wbb(o)
            if (omx.x < cmn.x or omn.x > cmx.x or omx.y < cmn.y or omn.y > cmx.y
                or omx.z < cmn.z or omn.z > cmx.z): continue
            if max(o.dimensions) < 0.12: continue  # tiny fittings
            mod = o.modifiers.new("cut", 'BOOLEAN')
            mod.operation = 'DIFFERENCE'
            mod.object = cut
            mod.solver = 'EXACT'
            bpy.context.view_layer.objects.active = o
            try:
                bpy.ops.object.modifier_apply(modifier=mod.name)
                n_cut += 1
            except Exception:
                o.modifiers.remove(mod)
        bpy.data.objects.remove(cut)
        print(f"HOLE {name}: cut {n_cut} meshes")
    # drop the direct-port glass blocks so the openings are genuinely open
    for name in DIRECT_PORTS:
        g = O(name)
        if g: bpy.data.objects.remove(g)

if CUT_AT_BAKE:
    cut_vision_holes()
else:
    n_glass = sum(1 for o in bpy.data.objects if o.type == 'MESH'
                  and any(ms and ms.name == "periscope glass" for ms in o.data.materials))
    print(f"VISION PORTS: authored-in-blend policy — no bake-time cuts; {n_glass} glass blocks export")

# ── ANIMATED PARTS: excluded from the static bake, exported as pivot-origin GLBs the engine
# drives (TPZ needle by turret bearing, wheels by lay, the round instanced across the rack +
# chamber). Shells-collection members are excluded too (the engine instances the textured round
# at each slot transform from tiger_interior.json; the rack TRAYS stay static geometry).
ANIMATED = ["TPZ dial indicator", "Gun wheel1", "wheel2", "Round in the chamber",
            "GUN LEVER", "Breech Cover", "hatch lever"]  # breech lever + cover: the manual breech ritual
shell_col = bpy.data.collections.get("Shells")
shell_excl = set()
if shell_col:
    for o in shell_col.all_objects:
        if o.type == 'MESH' and 0.7 < max(o.dimensions) < 1.2 and sorted(o.dimensions)[1] < 0.25:
            shell_excl.add(o)  # shell-shaped only; trays stay

all_mesh = set(o for o in bpy.data.objects if o.type == 'MESH')
anim_objs = {n: O(n) for n in ANIMATED if O(n)}
# Fuzzy fallback for the HATCH LEVER (rename in flux): any mesh with "hatch"+"lever" (or
# "luken"+"hebel") in the name that is NOT part of the cupola-lid group.
if "hatch lever" not in anim_objs:
    for o in bpy.data.objects:
        if o.type != 'MESH':
            continue
        # match the OBJECT name or the MESH-DATA name — the lever lives as object 'Turret_0144'
        # with mesh data 'Hatch Lever' (user, 2026-07-07), which object-only matching missed.
        names = [o.name.lower(), o.data.name.lower()]
        if any(("hatch" in nl and "lever" in nl) or ("luke" in nl and "hebel" in nl) for nl in names) \
           and "cupola hatch" not in o.name.lower():
            anim_objs["hatch lever"] = o
            print(f"HATCH LEVER fuzzy-matched: object {o.name!r} (data {o.data.name!r})")
            break
all_mesh -= set(anim_objs.values())
all_mesh -= shell_excl

# ---- exclusions: the cupola-hatch lid (engine uses a stand-in box) + Turret_0232 (the
# extinguisher bracket cylinder — the engine mounts its INTERACTIVE bottle there instead) ----
excl = set(o for o in bpy.data.objects if "cupola hatch" in o.name.lower())
if O("Turret_0232"):
    excl.add(O("Turret_0232"))
# The cupola AA-MG assembly (empty 'cupola_gun_pivot': mount + MG34, ~1.2 m of it hanging
# forward over the roof): DELETED from the game (user 2026-07-11) — it sat dead in the
# forward periscope's line and blocked the view. Excluded whole; the .blend keeps it.
_mg = O("cupola_gun_pivot")
if _mg:
    mg_meshes = set(o for o in _mg.children_recursive if o.type == 'MESH')
    excl |= mg_meshes
    print(f"CUPOLA MG excluded: {len(mg_meshes)} meshes under 'cupola_gun_pivot'")

# ---- gun: Gun_pivot descendants ----
gun_set = set(o for o in O("Gun_pivot").children_recursive if o.type == 'MESH')

# ---- turret: Turret_Pivot descendants + turret orphan-root subtrees, minus gun/excl ----
turret_set = set(o for o in O("Turret_Pivot").children_recursive if o.type == 'MESH')
for rn in ["Kommandantenkuppel", "turret escape hatch pivot", "Turret_0143",
           "TPZ dial indicator", "turret position indicator backing"]:
    r = O(rn)
    if not r: continue
    if r.type == 'MESH': turret_set.add(r)
    turret_set.update(o for o in r.children_recursive if o.type == 'MESH')
turret_set -= gun_set
turret_set -= excl
turret_set -= set(anim_objs.values())
turret_set -= shell_excl
# Turret_0125 is the long fitting above the fixed sponson racks, not a cartridge.
# Its inherited source parent is incorrect for these fixed trays.
if O("Turret_0125"): turret_set.discard(O("Turret_0125"))
gun_set -= set(anim_objs.values())

# ---- hull: everything else ----
hull_set = all_mesh - gun_set - turret_set - excl

# ---- origins (world) ----
# hull ground origin: footprint centre X/Y (barrel excluded — it skews fore-aft), lowest Z.
pts = [o.matrix_world @ mathutils.Vector(c) for o in all_mesh if o not in gun_set for c in o.bound_box]
gmin = mathutils.Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
gmax = mathutils.Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
hull_org = mathutils.Vector(((gmin.x + gmax.x) / 2, (gmin.y + gmax.y) / 2, gmin.z))
tur_org = O("Turret_Pivot").matrix_world.translation.copy()
gun_org = O("Gun_pivot").matrix_world.translation.copy()

# ---- shift-all / export-selected / shift-back (origin at pivot, no join) ----
roots = [o for o in bpy.data.objects if o.parent is None]
def shift(v):
    for r in roots:
        r.location = r.location + v
    bpy.context.view_layer.update()

os.makedirs(OUT, exist_ok=True)
def export(group, org, name):
    shift(-org)
    bpy.ops.object.select_all(action='DESELECT')
    for o in group:
        o.select_set(True)
    path = f"{OUT}/tiger_{name}.glb"
    # FIDELITY GUARANTEE: no decimation anywhere in this pipeline — the export carries every
    # vertex (no draco, no simplify; apply_modifiers only realizes the authored booleans). The
    # count printed here IS the .blend's count for the group; the engine loader keeps all of it.
    verts = sum(len(o.data.vertices) for o in group)
    bpy.ops.export_scene.gltf(filepath=path, use_selection=True,
                              export_format="GLB", export_yup=True)
    shift(org)
    sz = os.path.getsize(path) / 1e6
    print(f"BAKED {name}: {len(group)} meshes, {verts} verts (FULL RES, no decimation) -> {sz:.1f} MB")

# ---- GLASS SPLIT (user convention 2026-07-06) ----
# `Glass` material  -> SEE-THROUGH ports (engine GLASS mode, params.w=7)… except the
#                      `gunner sight lens`, which is an RTT screen (the TZF is an offset optic).
# `periscope glass` -> RTT SCREENS (mirror periscopes): each object's faces get UVs mapped to a
#                      tile of the engine's RTT atlas; a JSON records centroid/bearing/tile.
import json as _json
def _mat_objs(mname):
    return set(o for o in bpy.data.objects if o.type == 'MESH'
               and any(ms.material and ms.material.name == mname for ms in o.material_slots))
# USER (final): Left/Right Port Glass AND the gunner sight lens are FULLY TRANSPARENT
# (see-through slits/optic) — only the `periscope glass` faces are RTT screens.
port_objs = _mat_objs("Glass")
rtt_objs = _mat_objs("periscope glass")
glassy = port_objs | rtt_objs
glass_split = {}
for tag, group in (("port", port_objs), ("rtt", rtt_objs)):
    glass_split[tag] = {
        "hull": group & hull_set, "turret": group & turret_set, "gun": group & gun_set}
hull_set -= glassy
turret_set -= glassy
gun_set -= glassy

# RTT atlas: 4x4 tiles. Assign each screen object a tile; UV-map its faces onto that tile by
# planar projection along the object's DERIVED bearing. Emit the screens JSON the engine reads.
TILES = 4
screens = []
def _frame_org(fr):
    return {"hull": hull_org, "turret": tur_org, "gun": gun_org}[fr]

# ── BEARINGS FROM GEOMETRY (2026-07-07) ─────────────────────────────────────────────────────────
# The old per-object area-weighted normal was garbage for every periscope whose housing faces
# dominate its glass (gunner periscope averaged to straight UP; four cupola blocks pointed at the
# floor) — and that bearing feeds BOTH the aperture camera and the engine's gaze-matched tile
# update, so those screens froze on their warm-start frame. Centroids are trustworthy; normals
# aren't. Rules, in priority order:
#   1. CUPOLA RING — the turret screens clustered at the ring's height: bearing = horizontally
#      OUT from the ring centre (the blocks are literally arranged radially).
#   2. Gunner periscope (Turret_0003*): hard FORWARD (blender -Y) — its offset from the turret
#      centreline is sideways, so no radial rule can recover it.
#   3. Everything else: keep the mesh normal ONLY if it is believably horizontal (|z| <= 0.5,
#      outward-flipped); a vertical-dominated normal falls back to the horizontal radial.
def _centroid_w(o):
    mn, mx = _wbb(o)
    return (mn + mx) / 2

rtt_bearing = {}
_tur = sorted(glass_split["rtt"]["turret"], key=lambda x: x.name)
if _tur:
    ztop = max(_centroid_w(o).z for o in _tur)
    ring = [o for o in _tur if _centroid_w(o).z > ztop - 0.12]
    if len(ring) >= 3:
        rcx = sum(_centroid_w(o).x for o in ring) / len(ring)
        rcy = sum(_centroid_w(o).y for o in ring) / len(ring)
        for o in ring:
            c = _centroid_w(o)
            v = mathutils.Vector((c.x - rcx, c.y - rcy, 0.0))
            if v.length > 1e-4:
                rtt_bearing[o.name] = v.normalized()
        print(f"CUPOLA RING: {len(ring)} blocks around ({rcx:.3f}, {rcy:.3f})")
for fr in ("hull", "turret", "gun"):
    for o in glass_split["rtt"][fr]:
        if o.name in rtt_bearing:
            continue
        if o.name.startswith("Turret_0003"):
            rtt_bearing[o.name] = mathutils.Vector((0.0, -1.0, 0.0))  # gunner periscope: forward
            continue
        mw3 = o.matrix_world.to_3x3()
        n = mathutils.Vector((0.0, 0.0, 0.0))
        for pl in o.data.polygons:
            n += (mw3 @ pl.normal) * max(pl.area, 1e-9)
        n.normalize()
        c = _centroid_w(o)
        org = _frame_org(fr)
        outv = mathutils.Vector((c.x - org.x, c.y - org.y, 0.0))
        if outv.length > 1e-6 and n.dot(outv.normalized()) < 0:
            n = -n
        if abs(n.z) > 0.5:  # housing-dominated normal — fall back to the horizontal radial
            n = outv.normalized() if outv.length > 1e-4 else mathutils.Vector((0.0, -1.0, 0.0))
        rtt_bearing[o.name] = n.normalized()

# ── PANE/HOUSING SPLIT (2026-07-11) ─────────────────────────────────────────────────────────────
# The whole periscope object used to export as the RTT screen; its housing faces clamped to the
# tile centre — which samples bright sky/snow, and under the engine's own-colour emissive the
# housings rendered BLOWN WHITE frames. Split each screen object: only the broadside glass PANES
# stay in the RTT export; the housing faces move to the parent frame set (hull/turret/gun) so
# they render with their authored interior metal, lit like the rest of the compartment.
import bmesh as _bmesh
_frame_sets = {"hull": hull_set, "turret": turret_set, "gun": gun_set}
for fr in ("hull", "turret", "gun"):
    for o in list(glass_split["rtt"][fr]):
        n = rtt_bearing[o.name]
        mw3 = o.matrix_world.to_3x3()
        gi = {i for i, ms in enumerate(o.material_slots)
              if ms.material and ms.material.name == "periscope glass"}
        pane_m = [pl.material_index in gi for pl in o.data.polygons]
        pane_b = [abs((mw3 @ pl.normal).normalized().dot(n)) > 0.5 for pl in o.data.polygons]
        if any(pane_m):
            # Glass-tagged faces exist: broadside ones are the pane; if none are broadside
            # (Hull_0241 — angled eyepiece), the glass faces ARE the pane regardless of angle.
            pane = [m and b for m, b in zip(pane_m, pane_b)]
            if not any(pane):
                pane = pane_m
        else:
            # Glass slot present but ZERO faces assigned (the Turret_0003 TZF pieces): a BIG
            # such object is housing mis-listed as a screen — demote it whole to the frame
            # set (plain metal, no atlas, no emissive). Small ones split by orientation.
            if len(pane_b) > 600 or not any(pane_b) or all(pane_b):
                _frame_sets[fr].add(o)
                glass_split["rtt"][fr].remove(o)
                print(f"DEMOTED {o.name}: {len(pane_b)} faces, no glass faces — metal, not a screen")
                continue
            pane = pane_b
        if all(pane):
            continue  # all-pane — nothing to move to the frame set
        # Housing copy: keeps every NON-pane face, joins the frame set (authored material).
        h = o.copy(); h.data = o.data.copy()
        bpy.context.scene.collection.objects.link(h)
        h.name = o.name + "_housing"
        # The copy must ride the export's shift-all pass: `roots` was captured before this
        # split, so a parentless copy would NOT shift with the frame — it exported displaced
        # by the frame origin (the 'transparent optics' hole: housing drawn offset, leaving
        # the see-through port glass exposed where the TZF body should be).
        if h.parent is None:
            roots.append(h)
        bm = _bmesh.new(); bm.from_mesh(h.data); bm.faces.ensure_lookup_table()
        for f_ in [f for i, f in enumerate(bm.faces) if pane[i]]:
            bm.faces.remove(f_)
        bm.to_mesh(h.data); bm.free()
        _frame_sets[fr].add(h)
        # Screen object: keeps ONLY the pane faces (the UV fit below then spans the tile exactly).
        bm = _bmesh.new(); bm.from_mesh(o.data); bm.faces.ensure_lookup_table()
        for f_ in [f for i, f in enumerate(bm.faces) if not pane[i]]:
            bm.faces.remove(f_)
        bm.to_mesh(o.data); bm.free()
        print(f"PANE SPLIT {o.name}: {sum(pane)} pane / {len(pane) - sum(pane)} housing faces")

tile_i = 0
for fr in ("hull", "turret", "gun"):
    for o in sorted(glass_split["rtt"][fr], key=lambda x: x.name):
        n = rtt_bearing[o.name].copy()
        wc = o.matrix_world.translation
        org = _frame_org(fr)
        # UVs in the VIEWER's screen basis — the commander looks roughly ALONG the bearing at the
        # eyepiece, so u = viewer-right (up × n) and v = screen-down (-up). Projecting along the
        # raw bearing axis mirrored the inner face; and fitting the range over the WHOLE mesh
        # squeezed wide housings (the gunner periscope head) into an edge sliver of the tile.
        # Only faces BROADSIDE to the bearing (the glass panes) take the projection and define
        # the fit range; housing side faces clamp to the tile centre (a harmless solid sample).
        up = mathutils.Vector((0.0, 0.0, 1.0))
        u_ax = n.cross(up).normalized()  # viewer-right SEEN FROM INSIDE (pattern-verified: up×n mirrored)
        v_ax = -up
        uv = o.data.uv_layers.active or o.data.uv_layers.new(name="rtt")
        mw3 = o.matrix_world.to_3x3()
        # The SCREEN is the glass-material pane(s), broadside to the bearing — the fit range comes
        # from THEM alone so the pane spans the whole tile (fitting over the full block squeezed
        # the pane into the tile's snow-slice — pattern-verified). Housing faces clamp to centre.
        gi = {i for i, ms in enumerate(o.material_slots)
              if ms.material and ms.material.name == "periscope glass"}
        aligned = [pl.material_index in gi and abs((mw3 @ pl.normal).normalized().dot(n)) > 0.5
                   for pl in o.data.polygons]
        if not any(aligned):  # no broadside glass faces — fall back to any broadside face
            aligned = [abs((mw3 @ pl.normal).normalized().dot(n)) > 0.5 for pl in o.data.polygons]
        if not any(aligned):
            aligned = [True] * len(o.data.polygons)  # degenerate: fall back to everything
        us, vs = [], []
        for pi, pl in enumerate(o.data.polygons):
            if not aligned[pi]:
                continue
            for li in range(pl.loop_start, pl.loop_start + pl.loop_total):
                pt = o.matrix_world @ o.data.vertices[o.data.loops[li].vertex_index].co
                us.append(pt.dot(u_ax)); vs.append(pt.dot(v_ax))
        u0, u1 = min(us), max(us); v0, v1 = min(vs), max(vs)
        du = max(u1 - u0, 1e-6); dv = max(v1 - v0, 1e-6)
        tx, ty = tile_i % TILES, tile_i // TILES
        pad = 0.06
        for pi, pl in enumerate(o.data.polygons):
            for li in range(pl.loop_start, pl.loop_start + pl.loop_total):
                if aligned[pi]:
                    pt = o.matrix_world @ o.data.vertices[o.data.loops[li].vertex_index].co
                    fu = min(max((pt.dot(u_ax) - u0) / du, 0.0), 1.0)
                    fv = min(max((pt.dot(v_ax) - v0) / dv, 0.0), 1.0)
                else:
                    fu = fv = 0.5
                # Blender UV space is v=0-at-BOTTOM; the glTF exporter converts (v -> 1-v) to the
                # v=0-at-TOP space the engine samples, so write the flipped value.
                uvy = (ty + pad + fv * (1 - 2 * pad)) / TILES
                uv.data[li].uv = ((tx + pad + fu * (1 - 2 * pad)) / TILES, 1.0 - uvy)
        rel = wc - org
        screens.append({
            "name": o.name, "frame": fr, "tile": tile_i,
            "centroid": [round(rel.x, 4), round(rel.z, 4), round(-rel.y, 4)],
            "bearing": [round(n.x, 4), round(n.z, 4), round(-n.y, 4)],
        })
        tile_i += 1
        if tile_i >= TILES * TILES:
            break
with open(f"{OUT}/tiger_rtt_screens.json", "w") as f:
    _json.dump(screens, f, indent=1)
print(f"RTT screens: {len(screens)} -> tiger_rtt_screens.json; ports: "
      f"{sum(len(v) for v in glass_split['port'].values())}")

# ── PORT APERTURES (tiger_ports.json): the see-through openings (Glass material) as light
# apertures for the engine's interior light-shaft pass — centroid + outward bearing per port,
# same sanitized-normal rule as the screens (vertical-dominated normal → horizontal radial).
ports_json = []
for fr in ("hull", "turret", "gun"):
    for o in sorted(glass_split["port"][fr], key=lambda x: x.name):
        mw3 = o.matrix_world.to_3x3()
        pn = mathutils.Vector((0.0, 0.0, 0.0))
        for pl in o.data.polygons:
            pn += (mw3 @ pl.normal) * max(pl.area, 1e-9)
        pn.normalize()
        c = _centroid_w(o)
        org = _frame_org(fr)
        outv = mathutils.Vector((c.x - org.x, c.y - org.y, 0.0))
        if outv.length > 1e-6 and pn.dot(outv.normalized()) < 0:
            pn = -pn
        if abs(pn.z) > 0.5:
            pn = outv.normalized() if outv.length > 1e-4 else mathutils.Vector((0.0, -1.0, 0.0))
        rel = c - org
        ports_json.append({
            "name": o.name, "frame": fr,
            "centroid": [round(rel.x, 4), round(rel.z, 4), round(-rel.y, 4)],
            "bearing": [round(pn.x, 4), round(pn.z, 4), round(-pn.y, 4)],
        })
with open(f"{OUT}/tiger_ports.json", "w") as f:
    _json.dump(ports_json, f, indent=1)
print(f"PORT apertures: {len(ports_json)} -> tiger_ports.json")
# main bodies (glass already split out above) — these were accidentally dropped in the
# glass-split rework; without them the hull/turret/gun GLBs go stale on re-bake.
export(hull_set, hull_org, "hull")
export(turret_set, tur_org, "turret")
export(gun_set, gun_org, "gun")
for fr, org in (("hull", hull_org), ("turret", tur_org), ("gun", gun_org)):
    if glass_split["port"][fr]:
        export(glass_split["port"][fr], org, f"port_{fr}")
    if glass_split["rtt"][fr]:
        export(glass_split["rtt"][fr], org, f"rtt_{fr}")
# each animated part: origin at its OWN object origin (dial centre / wheel hub / round base pose)
PART_FILE = {"TPZ dial indicator": "part_tpz", "Gun wheel1": "part_wheel1",
             "wheel2": "part_wheel2", "Round in the chamber": "part_round",
             "GUN LEVER": "part_gunlever", "Breech Cover": "part_breechcover",
             "hatch lever": "part_hatchlever"}
for name, o in anim_objs.items():
    export({o}, o.matrix_world.translation.copy(), PART_FILE[name])
print("DONE")
