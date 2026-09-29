# preview_blueprint.py — render a generated building blueprint from the kit GLBs (headless bpy).
#
# The grammar's CLI writes the data this script reads (all in captures/, flat):
#   cargo run -- urban-grammar captures
#     captures/urban_grammar_kit.json                       kit asset → GLB parts, filters, placement
#     captures/urban_grammar_<typology>_<condition>.preview.json   modules, transforms, failed state
#
# Usage (from the repository root):
#   blender --background --python tools/worldgen/preview_blueprint.py -- \
#       captures/urban_grammar_kit.json captures/urban_grammar_<t>_<c>.preview.json \
#       captures/urban_grammar_<t>_<c>.png
#
# The output PNG holds two 1280×720 views side by side: an exterior three-quarter view from the
# street (+Z) side at the building's mid-height, and a cutaway from above with the street facade,
# the roof and the street halves of the gables removed. Each camera sits at the nearest distance
# that keeps every shown module inside the frame. Modules the spec's condition_damage failed are
# left out, exactly as the runtime hides a failed module (their rubble is the runtime's business).
# Each kit asset is rebuilt as the engine builds it (building_kit::load_mesh): its GLB parts, the
# same material filter, the same half-space cuts with their caps and the same part placement, then
# instanced by the module matrices. This is an Eevee preview for judging form against the
# photographs (tools/worldgen/grammar_boards.py), not the shipping renderer. Deterministic: fixed
# cameras, sun and sampling; no randomness.
import json
import math
import os
import re
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

argv = sys.argv[sys.argv.index("--") + 1:]
KIT_JSON, PREVIEW_JSON, OUT = argv[0], argv[1], argv[2]
WIDTH, HEIGHT = 1280, 720

# Engine space (x east, y up, z toward the street) → Blender (x, -z, y), the glTF importer's own
# convention, so an imported GLB's mesh is already C · v.
C = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
CI = C.inverted()


def engine_matrix(cols):
    return Matrix([[cols[c * 4 + r] for c in range(4)] for r in range(4)])


def base_name(name):
    return re.sub(r"\.\d{3}$", "", name)


bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
kit = {asset["id"]: asset for asset in json.load(open(KIT_JSON))}
preview = json.load(open(PREVIEW_JSON))
spec = preview["spec"]

source_cache = {}


def source_object(path):
    """Import a GLB once and bake it into one hidden mesh object (node transforms applied)."""
    if path in source_cache:
        return source_cache[path]
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=os.path.abspath(path))
    new = [o for o in bpy.data.objects if o not in before and o.type == "MESH"]
    for extra in [o for o in bpy.data.objects if o not in before and o.type != "MESH"]:
        bpy.data.objects.remove(extra, do_unlink=True)
    for obj in new:
        obj.data = obj.data.copy()
        obj.data.transform(obj.matrix_world)
        obj.matrix_world = Matrix.Identity(4)
    if len(new) > 1:
        bpy.ops.object.select_all(action="DESELECT")
        for obj in new:
            obj.select_set(True)
        bpy.context.view_layer.objects.active = new[0]
        bpy.ops.object.join()
    obj = new[0]
    obj.hide_render = True
    obj.hide_viewport = True
    source_cache[path] = obj
    return obj


kit_meshes = {}


def cap_cut(bm, cut_edges, normal):
    """Close a cut the way building_kit::clip_mesh does: merge the cut edges into straight
    segments and bridge each pair of parallel, equal segments offset across the plane (the two
    faces of a wall) with a quad facing +normal."""
    remaining = set(cut_edges)
    chains = []
    while remaining:
        edge = remaining.pop()
        chain = [edge.verts[0], edge.verts[1]]
        grown = True
        while grown:
            grown = False
            direction = (chain[-1].co - chain[0].co).normalized()
            for end in (0, -1):
                v = chain[end]
                for other in [e for e in remaining if v in e.verts]:
                    w = other.other_vert(v)
                    if direction.cross((w.co - v.co).normalized()).length < 1e-3:
                        remaining.discard(other)
                        if end == 0:
                            chain.insert(0, w)
                        else:
                            chain.append(w)
                        grown = True
                        break
                if grown:
                    break
        chains.append((chain[0], chain[-1]))
    uv_layer = bm.loops.layers.uv.active
    used = set()
    for i, (a, b) in enumerate(chains):
        if i in used:
            continue
        length = (b.co - a.co).length
        if length < 1e-4:
            continue
        direction = (b.co - a.co) / length
        mid = (a.co + b.co) * 0.5
        best = None
        for j, (c, e) in enumerate(chains):
            if j == i or j in used:
                continue
            other = e.co - c.co
            offset = (c.co + e.co) * 0.5 - mid
            if (
                other.length > 1e-4
                and direction.cross(other.normalized()).length < 1e-3
                and abs(other.length - length) < 1e-3
                and abs(offset.dot(direction)) < 1e-3
                and abs(offset.dot(normal.normalized())) < 1e-4
                and offset.length > 1e-4
                and (best is None or offset.length < best[1])
            ):
                best = (j, offset.length)
        if best is None:
            continue
        j, width = best
        used.update((i, j))
        c, e = chains[j]
        offset = (c.co + e.co) * 0.5 - mid
        qa, qb = (c, e) if (c.co - (a.co + offset)).length < (e.co - (a.co + offset)).length else (e, c)
        try:
            face = bm.faces.new((a, b, qb, qa))
        except ValueError:
            continue
        if face.normal.dot(normal) < 0.0:
            face.normal_flip()
        neighbour = next((f for f in a.link_faces if f is not face), None)
        if neighbour is not None:
            face.material_index = neighbour.material_index
        if uv_layer is not None and neighbour is not None:
            uv_at = {}
            for f in a.link_faces[:] + b.link_faces[:]:
                if f is face:
                    continue
                for loop in f.loops:
                    uv_at.setdefault(loop.vert, loop[uv_layer].uv.copy())
            for loop in face.loops:
                if loop.vert in (a, b):
                    loop[uv_layer].uv = uv_at.get(loop.vert, loop[uv_layer].uv)
                else:
                    near = a if loop.vert is qa else b
                    base = uv_at.get(near, loop[uv_layer].uv)
                    loop[uv_layer].uv = (base[0], base[1] + width * 0.25)


engine_materials = {}


def engine_material(texture):
    """A material sampling an engine-exported texture with the engine's nearest texels."""
    if texture in engine_materials:
        return engine_materials[texture]
    mat = bpy.data.materials.new(texture or "untextured")
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.9
    if texture:
        node = mat.node_tree.nodes.new("ShaderNodeTexImage")
        node.image = bpy.data.images.load(os.path.join(os.path.dirname(os.path.abspath(KIT_JSON)), texture))
        node.interpolation = "Closest"
        mat.node_tree.links.new(node.outputs["Color"], bsdf.inputs["Base Color"])
    engine_materials[texture] = mat
    return mat


def engine_mesh(asset_id, mesh_file):
    """An asset exactly as the engine built it (building_kit::export_meshes): damage cuts, snow
    and soot included. Engine space → Blender (x, -z, y); glTF V runs down, Blender's up."""
    data = json.load(open(os.path.join(os.path.dirname(os.path.abspath(KIT_JSON)), mesh_file)))
    p = data["positions"]
    verts = [(p[i], -p[i + 2], p[i + 1]) for i in range(0, len(p), 3)]
    idx = data["indices"]
    faces = [(idx[i], idx[i + 1], idx[i + 2]) for i in range(0, len(idx), 3)]
    mesh = bpy.data.meshes.new(asset_id)
    mesh.from_pydata(verts, [], faces)
    uv = mesh.uv_layers.new(name="UVMap")
    uvs = data["uvs"]
    for loop in mesh.loops:
        k = loop.vertex_index
        uv.data[loop.index].uv = (uvs[2 * k], 1.0 - uvs[2 * k + 1])
    mesh.materials.append(engine_material(data.get("texture")))
    mesh.update()
    return mesh


def kit_mesh(asset_id):
    """The asset's mesh as the engine builds it: its exported engine mesh when the grammar wrote
    one (every variant has one), else the filtered parts, placed, joined."""
    if asset_id in kit_meshes:
        return kit_meshes[asset_id]
    asset = kit[asset_id]
    if asset.get("mesh"):
        kit_meshes[asset_id] = engine_mesh(asset_id, asset["mesh"])
        return kit_meshes[asset_id]
    bm_all = bmesh.new()
    materials = []
    for part in asset["parts"]:
        src = source_object(part["path"])
        include, exclude = set(part["include"]), set(part["exclude"])
        bm = bmesh.new()
        bm.from_mesh(src.data)
        slots = [base_name(s.material.name) if s.material else "" for s in src.material_slots]
        drop = [
            f for f in bm.faces
            if (include and slots[f.material_index] not in include) or slots[f.material_index] in exclude
        ]
        bmesh.ops.delete(bm, geom=drop, context="FACES")
        # Half-space cuts n · p <= d in the source's engine space (building_kit::clip_mesh); the
        # imported mesh is C · p, so the plane is (C n) · q <= d in Blender space.
        for nx, ny, nz, dist in part.get("clip", []):
            normal = (C.to_3x3() @ Vector((nx, ny, nz)))
            result = bmesh.ops.bisect_plane(
                bm,
                geom=bm.verts[:] + bm.edges[:] + bm.faces[:],
                plane_co=normal * (dist / normal.length_squared),
                plane_no=normal,
                clear_outer=True,
            )
            cut = [g for g in result["geom_cut"] if isinstance(g, bmesh.types.BMEdge) and g.is_valid]
            cap_cut(bm, cut, normal)
        # Engine-space placement of the part, conjugated into Blender space.
        bm.transform(C @ engine_matrix(part["matrix"]) @ CI)
        # Remap material indices into the joined mesh.
        for f in bm.faces:
            mat = src.material_slots[f.material_index].material
            if mat not in materials:
                materials.append(mat)
            f.material_index = materials.index(mat)
        tmp = bpy.data.meshes.new(f"tmp_{asset_id}")
        bm.to_mesh(tmp)
        bm.free()
        bm_all.from_mesh(tmp)
        bpy.data.meshes.remove(tmp)
    mesh = bpy.data.meshes.new(asset_id)
    bm_all.to_mesh(mesh)
    bm_all.free()
    for mat in materials:
        mesh.materials.append(mat)
    kit_meshes[asset_id] = mesh
    return mesh


# ---- The building ----
building = bpy.data.collections.new("building")
scene.collection.children.link(building)
objects = []
for module in preview["modules"]:
    if module["failed"]:
        continue
    obj = bpy.data.objects.new(module["id"], kit_mesh(module["asset"]))
    obj.matrix_world = C @ engine_matrix(module["matrix"]) @ CI
    building.objects.link(obj)
    objects.append((module, obj))

# ---- Ground: trodden snow round the footprint (left open so the cellar shows in the cutaway) ----
w, d = spec["width_m"], spec["depth_m"]
snow = bpy.data.materials.new("snow")
snow.use_nodes = True
bsdf = snow.node_tree.nodes["Principled BSDF"]
bsdf.inputs["Base Color"].default_value = (0.82, 0.84, 0.86, 1.0)
bsdf.inputs["Roughness"].default_value = 0.9
margin = 30.0
for (x0, x1, z0, z1) in [
    (-w / 2 - margin, w / 2 + margin, d / 2, d / 2 + margin),
    (-w / 2 - margin, w / 2 + margin, -d / 2 - margin, -d / 2),
    (-w / 2 - margin, -w / 2, -d / 2, d / 2),
    (w / 2, w / 2 + margin, -d / 2, d / 2),
]:
    mesh = bpy.data.meshes.new("ground")
    verts = [(x0, -z0, 0.0), (x1, -z0, 0.0), (x1, -z1, 0.0), (x0, -z1, 0.0)]
    mesh.from_pydata(verts, [], [(0, 1, 2, 3)])
    mesh.materials.append(snow)
    ground = bpy.data.objects.new("ground", mesh)
    scene.collection.objects.link(ground)

# ---- Light: low winter sun and an overcast sky ----
world = bpy.data.worlds.new("sky")
world.use_nodes = True
world.node_tree.nodes["Background"].inputs[0].default_value = (0.62, 0.66, 0.72, 1.0)
world.node_tree.nodes["Background"].inputs[1].default_value = 0.9
scene.world = world
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 3.2
sun.data.angle = math.radians(1.0)
sun.rotation_euler = (math.radians(62), 0.0, math.radians(-35))
scene.collection.objects.link(sun)

scene.render.engine = "BLENDER_EEVEE"
scene.render.resolution_x = WIDTH
scene.render.resolution_y = HEIGHT
scene.render.film_transparent = False
scene.view_settings.view_transform = "Standard"

# Frame the standing building's bounding box (its stacks included) and the ground under it.
lo = Vector((-w / 2, -d / 2, 0.0))
hi = Vector((w / 2, d / 2, 0.0))
for _, obj in objects:
    for corner in obj.bound_box:
        p = obj.matrix_world @ Vector(corner)
        lo = Vector((min(lo.x, p.x), min(lo.y, p.y), min(lo.z, p.z)))
        hi = Vector((max(hi.x, p.x), max(hi.y, p.y), max(hi.z, p.z)))
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
cam.data.lens = 32
cam.data.clip_end = 2000
scene.collection.objects.link(cam)
scene.camera = cam


def aim(direction, target, margin=0.04):
    """Look at `target` along `direction` from the nearest distance at which every corner of every
    shown module's bounds (cellars from 1 m below grade) projects inside the frame, `margin` from
    its edges."""
    from bpy_extras.object_utils import world_to_camera_view

    direction = Vector(direction).normalized()
    seen = set()
    corners = []
    for _, obj in objects:
        if obj.hide_render:
            continue
        for corner in obj.bound_box:
            p = obj.matrix_world @ Vector(corner)
            key = (round(p.x, 1), round(p.y, 1), round(max(p.z, -1.0), 1))
            if key not in seen:
                seen.add(key)
                corners.append(Vector(key))

    def place(at, dist):
        cam.location = at + direction * dist
        cam.rotation_euler = (at - cam.location).to_track_quat("-Z", "Y").to_euler()
        bpy.context.view_layer.update()
        return [world_to_camera_view(scene, cam, c) for c in corners]

    def fits(at, dist):
        return all(
            depth > 0.0 and margin <= u <= 1.0 - margin and margin <= v <= 1.0 - margin
            for u, v, depth in place(at, dist)
        )

    def fit(at):
        near, far = 1.0, 1000.0
        for _ in range(32):
            mid = 0.5 * (near + far)
            if fits(at, mid):
                far = mid
            else:
                near = mid
        return far

    # Fit, then re-centre the look-at point on the projected extent and fit again.
    at = target.copy()
    for _ in range(3):
        dist = fit(at)
        projected = place(at, dist)
        us = [u for u, _, _ in projected]
        vs = [v for _, v, _ in projected]
        du = 0.5 * (min(us) + max(us)) - 0.5
        dv = 0.5 * (min(vs) + max(vs)) - 0.5
        frame = cam.data.view_frame(scene=scene)
        width = abs(frame[0].x - frame[2].x) / abs(frame[0].z) * dist
        height = abs(frame[0].y - frame[2].y) / abs(frame[0].z) * dist
        right = cam.matrix_world.to_3x3() @ Vector((1.0, 0.0, 0.0))
        up = cam.matrix_world.to_3x3() @ Vector((0.0, 1.0, 0.0))
        at = at + right * du * width + up * dv * height
    place(at, fit(at))


target = (lo + hi) * 0.5
target.z = (max(lo.z, -1.0) + hi.z) * 0.5
stem = os.path.splitext(OUT)[0]
paths = []

# Exterior three-quarter view from the street, front-left, level with the middle of the building:
# the upper windows of a gutted shell show sky as they do in the eye-level photographs.
aim((-0.75, -1.0, 0.0), target)
scene.render.filepath = stem + "__exterior.png"
bpy.ops.render.render(write_still=True)
paths.append(scene.render.filepath)

# Cutaway: the street facade (everything within 1 m of it), the roof and the street halves of
# the brick gables (which would otherwise hang over the cut end walls) removed.
for module, obj in objects:
    z = module["matrix"][14]
    street_gable = module["id"].startswith("module.gable.") and module["id"].endswith(".f")
    if z > d / 2 - 1.0 or module["kind"] == "Roof" or street_gable:
        obj.hide_render = True
aim((-0.35, -1.0, 0.42), target)
scene.render.filepath = stem + "__cutaway.png"
bpy.ops.render.render(write_still=True)
paths.append(scene.render.filepath)

# Side by side into OUT.
import numpy as np  # noqa: E402  (Blender's bundled numpy)

images = [bpy.data.images.load(p) for p in paths]
total_w = sum(i.size[0] for i in images)
total_h = max(i.size[1] for i in images)
canvas = np.zeros((total_h, total_w, 4), dtype=np.float32)
x = 0
for img in images:
    iw, ih = img.size
    px = np.empty(iw * ih * 4, dtype=np.float32)
    img.pixels.foreach_get(px)
    canvas[total_h - ih:, x:x + iw, :] = px.reshape(ih, iw, 4)
    x += iw
out = bpy.data.images.new("preview", total_w, total_h, alpha=False)
out.pixels.foreach_set(canvas.ravel())
out.filepath_raw = os.path.abspath(OUT)
out.file_format = "PNG"
out.save()
for p in paths:
    os.remove(p)
print(f"[preview] {OUT}: {len(objects)} modules shown, "
      f"{sum(1 for m in preview['modules'] if m['failed'])} failed, {len(kit_meshes)} kit assets")
