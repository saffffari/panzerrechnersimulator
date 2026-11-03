"""Shared Blender kit for the Berlin 1945 landmark models (run inside `blender --background`).

Every model in tools/berlin1945/ is built from boxes, cylinders, beams and extruded outlines into
one bmesh per material, then exported as one GLB with constant PBR materials (the engine keeps
each primitive's factors, src/gltf_material_channels.rs) and a collision sidecar
`<name>.colliders.json` read by src/authored_colliders.rs.

Frames. Blender model frame: +X east, +Y north, +Z up, metres, origin at ground level at the
footprint's reference point. The default glTF Y-up export gives the engine frame
`engine = (blender_x, blender_z, -blender_y)` (north = engine -Z, as everywhere in the project).
The colliders are written in the engine frame.

Collision. Only parts registered with `solid=True` (or `Model.solid`) become boxes. Each box is
the modelled part itself, or the axis-aligned bounds of a curved/sloped part in the model frame;
`Model.report` records the worst excess of a bounded part over its own volume and the parts left
without collision on purpose (statues, the bare dome ribs, cornice projections).

Determinism. No randomness: damage is chosen by the project's integer hash of a string key.
"""
import json
import math
import os
import sys
import zlib

import bmesh
import bpy
from mathutils import Matrix, Vector

# Linear base colour, roughness, metallic. Weathered and soot-darkened (the first in-engine still of
# 2026-11-02 read far too pale against BU 8573). Stone and brick values are mid-grey to dark: Silesian
# and Elbe sandstone weather to grey-ochre, and in 1945 the Reichstag and the Gate were soot-black
# in streaks (R1, R2 and B3 in docs/BERLIN_REICHSTAG_1945.md).
MATERIALS = {
    'sandstone':       ((0.2, 0.185, 0.16, 1.0), 0.88, 0.0),
    'sandstone_light': ((0.25, 0.235, 0.2, 1.0), 0.86, 0.0),
    'sandstone_dark':  ((0.1, 0.095, 0.085, 1.0), 0.90, 0.0),
    'soot':            ((0.045, 0.040, 0.036, 1.0), 0.95, 0.0),
    'brick_infill':    ((0.15, 0.095, 0.07, 1.0), 0.92, 0.0),
    'brick':           ((0.14, 0.085, 0.065, 1.0), 0.92, 0.0),
    'plaster':         ((0.23, 0.215, 0.19, 1.0), 0.90, 0.0),
    'plaster_dark':    ((0.12, 0.11, 0.1, 1.0), 0.92, 0.0),
    'render_grey':     ((0.18, 0.175, 0.165, 1.0), 0.90, 0.0),
    'granite':         ((0.24, 0.23, 0.22, 1.0), 0.80, 0.0),
    'steel':           ((0.055, 0.052, 0.050, 1.0), 0.62, 0.85),
    'steel_rust':      ((0.13, 0.065, 0.035, 1.0), 0.85, 0.30),
    'copper_patina':   ((0.11, 0.20, 0.16, 1.0), 0.70, 0.35),
    'bronze':          ((0.09, 0.065, 0.045, 1.0), 0.55, 0.80),
    'slate':           ((0.07, 0.075, 0.08, 1.0), 0.75, 0.0),
    'zinc_roof':       ((0.16, 0.17, 0.17, 1.0), 0.55, 0.60),
    'timber_charred':  ((0.03, 0.025, 0.022, 1.0), 0.95, 0.0),
    'glass':           ((0.05, 0.06, 0.06, 1.0), 0.08, 0.0),
    'rubble':          ((0.30, 0.27, 0.23, 1.0), 0.95, 0.0),
    'bark':            ((0.085, 0.072, 0.060, 1.0), 0.92, 0.0),
    'sandstone_red':   ((0.2, 0.1, 0.075, 1.0), 0.88, 0.0),
    'water':           ((0.035, 0.045, 0.040, 1.0), 0.16, 0.0),
    'cloth_red':       ((0.35, 0.02, 0.02, 1.0), 0.90, 0.0),
}


def h01(key) -> float:
    """The project's counter hash on a string key (crc32 seed), [0, 1)."""
    n = zlib.crc32(str(key).encode()) & 0xFFFFFFFF
    n = (n * 2654435761) & 0xFFFFFFFF
    n ^= n >> 15
    n = (n * 2246822519) & 0xFFFFFFFF
    n ^= n >> 13
    return ((n >> 8) & 0xFFFF) / 65536.0


def clear_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


class Model:
    def __init__(self, name, source):
        self.name = name
        self.source = source
        self.bms = {}
        self.boxes = []           # colliders, model frame: (x0, x1, y0, y1, z0, z1)
        self.bounded = []         # (part, excess m) for curved/sloped parts boxed by their bounds
        self.no_collision = set()
        self.tris = 0

    # ── raw geometry ───────────────────────────────────────────────────────────────────────────
    def bm(self, mat):
        if mat not in MATERIALS:
            raise KeyError(mat)
        if mat not in self.bms:
            self.bms[mat] = bmesh.new()
        return self.bms[mat]

    def _quad_box(self, bm, corners):
        """corners: 8 Vectors in the order of (ix, iy, iz) bits."""
        v = [bm.verts.new(c) for c in corners]
        for a, b, c, d in ((0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)):
            bm.faces.new((v[a], v[b], v[c], v[d]))

    def solid(self, x0, x1, y0, y1, z0, z1, part=None):
        x0, x1 = sorted((x0, x1)); y0, y1 = sorted((y0, y1)); z0, z1 = sorted((z0, z1))
        if x1-x0 > 1e-3 and y1-y0 > 1e-3 and z1-z0 > 1e-3:
            self.boxes.append((x0, x1, y0, y1, z0, z1))

    def box(self, x0, x1, y0, y1, z0, z1, mat, solid=False, part=None):
        x0, x1 = sorted((x0, x1)); y0, y1 = sorted((y0, y1)); z0, z1 = sorted((z0, z1))
        if x1-x0 < 1e-3 or y1-y0 < 1e-3 or z1-z0 < 1e-3:
            return
        c = [Vector((x, y, z)) for x in (x0, x1) for y in (y0, y1) for z in (z0, z1)]
        # order bits: x is the slowest above; rebuild as (ix, iy, iz) with ix the lowest bit
        corners = [Vector((x1 if i & 1 else x0, y1 if i & 2 else y0, z1 if i & 4 else z0)) for i in range(8)]
        self._quad_box(self.bm(mat), corners)
        if solid:
            self.solid(x0, x1, y0, y1, z0, z1, part)
        elif part:
            self.no_collision.add(part)

    def beam(self, p0, p1, w, h, mat, up=(0, 0, 1)):
        """A square-section member between two points (decoration: no collision)."""
        p0, p1 = Vector(p0), Vector(p1)
        d = p1-p0
        if d.length < 1e-4:
            return
        f = d.normalized()
        u = Vector(up)
        if abs(f.dot(u)) > 0.95:
            u = Vector((1, 0, 0))
        s = f.cross(u).normalized()
        u = s.cross(f).normalized()
        corners = []
        for i in range(8):
            base = p1 if i & 4 else p0
            corners.append(base+s*((w/2) if i & 1 else (-w/2))+u*((h/2) if i & 2 else (-h/2)))
        bm = self.bm(mat)
        v = [bm.verts.new(c) for c in corners]
        for a, b, c, dd in ((0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)):
            bm.faces.new((v[a], v[b], v[c], v[dd]))

    def cyl(self, cx, cy, z0, z1, r0, r1, mat, seg=12, solid=False, part='column'):
        bm = self.bm(mat)
        bot = [bm.verts.new((cx+r0*math.cos(2*math.pi*k/seg), cy+r0*math.sin(2*math.pi*k/seg), z0)) for k in range(seg)]
        top = [bm.verts.new((cx+r1*math.cos(2*math.pi*k/seg), cy+r1*math.sin(2*math.pi*k/seg), z1)) for k in range(seg)]
        bm.faces.new(list(reversed(bot)))
        bm.faces.new(top)
        for k in range(seg):
            bm.faces.new((bot[k], bot[(k+1) % seg], top[(k+1) % seg], top[k]))
        if solid:
            r = max(r0, r1)
            self.solid(cx-r, cx+r, cy-r, cy+r, z0, z1)
            # a circle in its square: the corner excess is r*(sqrt2-1)
            self.bounded.append((part, round(r*(math.sqrt(2)-1), 3)))

    def prism(self, pts, z0, z1, mat, solid=False, part=None, excess=None):
        """Extrude a simple polygon (list of (x, y), any winding) from z0 to z1."""
        bm = self.bm(mat)
        if len(pts) < 3:
            return
        area = sum(pts[i][0]*pts[(i+1) % len(pts)][1]-pts[(i+1) % len(pts)][0]*pts[i][1] for i in range(len(pts)))
        if area < 0:
            pts = list(reversed(pts))
        bot = [bm.verts.new((x, y, z0)) for x, y in pts]
        top = [bm.verts.new((x, y, z1)) for x, y in pts]
        bm.faces.new(list(reversed(bot)))
        bm.faces.new(top)
        n = len(pts)
        for k in range(n):
            bm.faces.new((bot[k], bot[(k+1) % n], top[(k+1) % n], top[k]))
        if solid:
            xs = [p[0] for p in pts]; ys = [p[1] for p in pts]
            self.solid(min(xs), max(xs), min(ys), max(ys), z0, z1)
            if excess is not None:
                self.bounded.append((part or 'prism', excess))

    def vprism(self, pts, axis, a0, a1, mat, solid=False, part=None, excess=None):
        """Extrude a polygon drawn in a vertical plane. axis='x': pts are (y, z), extruded x a0..a1;
        axis='y': pts are (x, z), extruded y a0..a1."""
        bm = self.bm(mat)
        area = sum(pts[i][0]*pts[(i+1) % len(pts)][1]-pts[(i+1) % len(pts)][0]*pts[i][1] for i in range(len(pts)))
        if area < 0:
            pts = list(reversed(pts))
        if axis == 'x':
            A = [bm.verts.new((a0, u, v)) for u, v in pts]
            B = [bm.verts.new((a1, u, v)) for u, v in pts]
        else:
            A = [bm.verts.new((u, a0, v)) for u, v in pts]
            B = [bm.verts.new((u, a1, v)) for u, v in pts]
        bm.faces.new(list(reversed(A)))
        bm.faces.new(B)
        n = len(pts)
        for k in range(n):
            bm.faces.new((A[k], A[(k+1) % n], B[(k+1) % n], B[k]))
        if solid:
            us = [p[0] for p in pts]; vs = [p[1] for p in pts]
            if axis == 'x':
                self.solid(a0, a1, min(us), max(us), min(vs), max(vs))
            else:
                self.solid(min(us), max(us), a0, a1, min(vs), max(vs))
            if excess is not None:
                self.bounded.append((part or 'vprism', excess))

    def gable(self, x0, x1, y0, y1, z0, h, ridge, mat, solid=False, part='pediment'):
        """A triangular gable block: ridge along 'x' or 'y', rising h above z0."""
        if ridge == 'x':
            ym = (y0+y1)/2
            self.vprism([(y0, z0), (y1, z0), (ym, z0+h)], 'x', x0, x1, mat, solid, part,
                        excess=round(h/2, 3) if solid else None)
        else:
            xm = (x0+x1)/2
            self.vprism([(x0, z0), (x1, z0), (xm, z0+h)], 'y', y0, y1, mat, solid, part,
                        excess=round(h/2, 3) if solid else None)

    def arch_fill(self, axis, plane0, plane1, u0, u1, spring, mat, solid=False):
        """The masonry over a round arch: the rectangle [u0,u1] x [spring, spring+r] minus the
        half disc. axis is the wall's normal axis ('x' or 'y'); plane0..plane1 its thickness."""
        r = (u1-u0)/2
        uc = (u0+u1)/2
        pts = [(u0, spring), (u0, spring+r), (u1, spring+r), (u1, spring)]
        for k in range(1, 8):
            t = math.pi*k/8
            pts.append((uc+r*math.cos(t), spring+r*math.sin(t)))
        self.vprism(pts, axis, plane0, plane1, mat)
        if solid:
            # the two haunches: boxed by the outer quarter of the rectangle on each side
            if axis == 'x':
                self.solid(plane0, plane1, u0, u0+r*0.3, spring, spring+r)
                self.solid(plane0, plane1, u1-r*0.3, u1, spring, spring+r)
                self.solid(plane0, plane1, u0, u1, spring+r*0.85, spring+r)
            else:
                self.solid(u0, u0+r*0.3, plane0, plane1, spring, spring+r)
                self.solid(u1-r*0.3, u1, plane0, plane1, spring, spring+r)
                self.solid(u0, u1, plane0, plane1, spring+r*0.85, spring+r)

    def text(self, string, size, mat, place, depth=0.06):
        """Raised letters. `place` maps the text's local (x right, y up, z out) to the model."""
        cu = bpy.data.curves.new('txt', 'FONT')
        cu.body = string
        cu.size = size
        cu.extrude = depth/2
        cu.align_x = 'CENTER'
        ob = bpy.data.objects.new('txt', cu)
        bpy.context.scene.collection.objects.link(ob)
        dg = bpy.context.evaluated_depsgraph_get()
        me = bpy.data.meshes.new_from_object(ob.evaluated_get(dg))
        tmp = bmesh.new()
        tmp.from_mesh(me)
        tmp.transform(place)
        bm = self.bm(mat)
        vmap = {}
        for v in tmp.verts:
            vmap[v.index] = bm.verts.new(v.co)
        for f in tmp.faces:
            try:
                bm.faces.new([vmap[v.index] for v in f.verts])
            except ValueError:
                pass
        tmp.free()
        bpy.data.objects.remove(ob)
        bpy.data.curves.remove(cu)

    # ── walls with openings ────────────────────────────────────────────────────────────────────
    def wall(self, axis, plane0, plane1, u0, u1, z0, z1, openings, mat, solid=True, merge_rows=True):
        """A wall whose normal is `axis` ('x' or 'y'), thickness plane0..plane1, spanning u0..u1
        along the wall and z0..z1 up, with rectangular openings [(a0, a1, b0, b1), ...]. The wall
        is the grid of rectangles no opening covers: its two faces are drawn per merged row, its
        edges (the openings' reveals, the wall's ends and top) only where a cell meets an opening
        or the outside; each merged row piece is one collision box."""
        us = sorted({u0, u1, *[min(max(o[0], u0), u1) for o in openings], *[min(max(o[1], u0), u1) for o in openings]})
        zs = sorted({z0, z1, *[min(max(o[2], z0), z1) for o in openings], *[min(max(o[3], z0), z1) for o in openings]})
        us = [u for k, u in enumerate(us) if k == 0 or u-us[k-1] > 1e-4]
        zs = [z for k, z in enumerate(zs) if k == 0 or z-zs[k-1] > 1e-4]
        def covered(u, z):
            return any(o[0] <= u <= o[1] and o[2] <= z <= o[3] for o in openings)
        ni, nj = len(us)-1, len(zs)-1
        if ni < 1 or nj < 1:
            return
        filled = [[not covered((us[i]+us[i+1])/2, (zs[j]+zs[j+1])/2) for i in range(ni)] for j in range(nj)]
        p0, p1 = min(plane0, plane1), max(plane0, plane1)
        bm = self.bm(mat)
        def V(p, u, z):
            return (p, u, z) if axis == 'x' else (u, p, z)
        def quad(a, b, c, d):
            vs = [bm.verts.new(V(*q)) for q in (a, b, c, d)]
            bm.faces.new(vs)
        for j in range(nj):
            za, zb = zs[j], zs[j+1]
            run = None
            for i in range(ni+1):
                f = i < ni and filled[j][i]
                if f and run is None:
                    run = i
                if (not f or not merge_rows or i == ni) and run is not None:
                    end = i if not f or i == ni else i+1
                    if not merge_rows and f:
                        end = i+1
                    ua, ub = us[run], us[end]
                    quad((p0, ua, za), (p0, ub, za), (p0, ub, zb), (p0, ua, zb))
                    quad((p1, ua, za), (p1, ua, zb), (p1, ub, zb), (p1, ub, za))
                    if solid:
                        if axis == 'x':
                            self.solid(p0, p1, ua, ub, za, zb)
                        else:
                            self.solid(ua, ub, p0, p1, za, zb)
                    run = None if (not f or not merge_rows) else run
                    if f and not merge_rows:
                        run = None
            # the cells' exposed edges
            for i in range(ni):
                if not filled[j][i]:
                    continue
                ua, ub = us[i], us[i+1]
                if i == 0 or not filled[j][i-1]:
                    quad((p0, ua, za), (p0, ua, zb), (p1, ua, zb), (p1, ua, za))
                if i == ni-1 or not filled[j][i+1]:
                    quad((p0, ub, za), (p1, ub, za), (p1, ub, zb), (p0, ub, zb))
                if j == 0 or not filled[j-1][i]:
                    quad((p0, ua, za), (p1, ua, za), (p1, ub, za), (p0, ub, za))
                if j == nj-1 or not filled[j+1][i]:
                    quad((p0, ua, zb), (p0, ub, zb), (p1, ub, zb), (p1, ua, zb))

    def _wall_piece(self, axis, p0, p1, u0, u1, z0, z1, mat, solid):
        if axis == 'x':
            self.box(p0, p1, u0, u1, z0, z1, mat, solid)
        else:
            self.box(u0, u1, p0, p1, z0, z1, mat, solid)

    def plate(self, axis, plane, out, u0, u1, z0, z1, mat):
        """A single facing quad just proud of a wall face (soot, a stain): two triangles."""
        p = plane+out*0.02
        bm = self.bm(mat)
        if axis == 'x':
            q = [(p, u0, z0), (p, u1, z0), (p, u1, z1), (p, u0, z1)]
        else:
            q = [(u0, p, z0), (u1, p, z0), (u1, p, z1), (u0, p, z1)]
        if (axis == 'x') == (out < 0):
            q.reverse()
        bm.faces.new([bm.verts.new(v) for v in q])

    def wbox(self, axis, p0, p1, u0, u1, z0, z1, mat, solid=False):
        """A box given in a wall's (normal, along, up) coordinates."""
        self._wall_piece(axis, p0, p1, u0, u1, z0, z1, mat, solid)

    # ── output ─────────────────────────────────────────────────────────────────────────────────
    def build_object(self):
        me = bpy.data.meshes.new(self.name)
        ob = bpy.data.objects.new(self.name, me)
        bpy.context.scene.collection.objects.link(ob)
        merged = bmesh.new()
        mats = []
        for mat, bm in self.bms.items():
            bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-4)
            bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
            m = bpy.data.materials.new(mat)
            m.use_nodes = True
            bsdf = m.node_tree.nodes['Principled BSDF']
            base, rough, metal = MATERIALS[mat]
            bsdf.inputs['Base Color'].default_value = base
            bsdf.inputs['Roughness'].default_value = rough
            bsdf.inputs['Metallic'].default_value = metal
            m.diffuse_color = base
            m.roughness = rough
            m.metallic = metal
            ob.data.materials.append(m)
            idx = len(mats)
            mats.append(mat)
            tmp = bm.copy()
            for f in tmp.faces:
                f.material_index = idx
            me2 = bpy.data.meshes.new('tmp')
            tmp.to_mesh(me2)
            merged.from_mesh(me2)
            bpy.data.meshes.remove(me2)
            tmp.free()
        bmesh.ops.triangulate(merged, faces=merged.faces)
        self.tris = len(merged.faces)
        merged.to_mesh(me)
        merged.free()
        for p in me.polygons:
            p.use_smooth = False
        return ob

    def export(self, out_dir, preview=None):
        os.makedirs(out_dir, exist_ok=True)
        ob = self.build_object()
        bpy.ops.object.select_all(action='DESELECT')
        ob.select_set(True)
        bpy.context.view_layer.objects.active = ob
        glb = os.path.join(out_dir, f'{self.name}.glb')
        bpy.ops.export_scene.gltf(filepath=glb, export_format='GLB', use_selection=True, export_yup=True,
                                  export_apply=True, export_normals=True, export_texcoords=False)
        # Colliders in the engine frame: (bx, bz, -by).
        boxes = [[round(x0, 3), round(z0, 3), round(-y1, 3), round(x1, 3), round(z1, 3), round(-y0, 3)]
                 for x0, x1, y0, y1, z0, z1 in self.boxes]
        worst = max([e for _, e in self.bounded], default=0.0)
        side = dict(schema='panzer.authored-colliders', version=1, model=f'{self.name}.glb', source=self.source,
                    frame='engine (x east, y up, z south), metres, the GLB origin', boxes=boxes,
                    bounded_parts=sorted({p for p, _ in self.bounded}), worst_bounded_excess_m=worst,
                    no_collision=sorted(self.no_collision),
                    note=('Boxes are the modelled solid parts; curved or sloped parts are boxed by their '
                          'model-frame bounds (worst excess above). Parts listed in no_collision are '
                          'decoration with no box (src/authored_colliders.rs).'))
        sidecar = os.path.join(out_dir, f'{self.name}.colliders.json')
        if boxes:
            with open(sidecar, 'w') as f:
                json.dump(side, f, separators=(',', ':'))
        elif os.path.exists(sidecar):
            os.remove(sidecar)
        dims = ob.dimensions
        report = dict(model=self.name, glb=glb, triangles=self.tris, colliders=len(boxes),
                      size_m=[round(dims.x, 2), round(dims.y, 2), round(dims.z, 2)], worst_bounded_excess_m=worst)
        print('[berlin1945]', json.dumps(report))
        if preview:
            render_previews(ob, preview)
        return report


def render_previews(ob, spec):
    """Workbench stills of the model for the author's own comparison (not shipped)."""
    out, cams = spec
    scene = bpy.context.scene
    scene.render.engine = 'BLENDER_WORKBENCH'
    scene.display.shading.light = 'STUDIO'
    scene.display.shading.color_type = 'MATERIAL'
    scene.display.shading.show_shadows = True
    scene.display.shading.show_cavity = True
    scene.render.resolution_x, scene.render.resolution_y = 1280, 900
    world = bpy.data.worlds.new('w')
    scene.world = world
    cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam'))
    scene.collection.objects.link(cam)
    scene.camera = cam
    for tag, eye, target, lens in cams:
        cam.location = Vector(eye)
        d = Vector(target)-Vector(eye)
        cam.rotation_euler = d.to_track_quat('-Z', 'Y').to_euler()
        cam.data.lens = lens
        cam.data.clip_end = 5000
        scene.render.filepath = f'{out}_{tag}.png'
        bpy.ops.render.render(write_still=True)


def args():
    argv = sys.argv
    return argv[argv.index('--')+1:] if '--' in argv else []
