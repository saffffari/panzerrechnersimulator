"""Rigging helpers for the gen2 Tiger (imported inside Blender: sys.path + import rig_lib).

Conventions
- A mechanism is a controller empty in the RIG_CONTROLLERS collection named CTRL.<Name>. Its rest
  frame lives in delta_rotation_euler (built so local X is the hinge or slide axis); its own
  rotation_euler.x (hinge, radians) or location.x (slide, metres) is the joint value, 0 = closed/rest.
- Limits and provenance are custom properties: rig_kind, min, max, unit, closed, source, and the
  gameplay notes interaction, damage, repair.
- Limits are found by sweeping the joint and testing the moving parts against the static geometry
  near them (BVH overlap); contacts already present at rest (the hinge knuckle in its bracket) are
  ignored.
"""
import bpy, re, math
import numpy as np
from mathutils import Vector, Matrix
from mathutils.bvhtree import BVHTree

SUFFIX = re.compile(r'(_îAÔO.*?)?(\.\d+)?$')


def clean(name):
    return SUFFIX.sub('', name).replace('TG_RM5010-', '')


def meshes():
    return [o for o in bpy.data.objects if o.type == 'MESH']


def V(o):
    M = o.matrix_world
    co = np.empty(len(o.data.vertices) * 3)
    o.data.vertices.foreach_get('co', co)
    co = co.reshape(-1, 3)
    R = np.array(M)
    return co @ R[:3, :3].T + R[:3, 3]


def centre(o):
    return Vector(V(o).mean(0))


def find(kit, near=None, all=False):
    """Kit part(s) by kit name (e.g. '34B26_DOOR'), nearest to a point if given."""
    c = [o for o in meshes() if clean(o.name) == kit]
    if all:
        return c
    if not c:
        return None
    if near is None:
        return c[0]
    p = Vector(near)
    return min(c, key=lambda o: (centre(o) - p).length)


def near_parts(point, radius, exclude=()):
    p = np.array(point)
    out = []
    for o in meshes():
        if o in exclude:
            continue
        mw = o.matrix_world
        bb = np.array([tuple(mw @ Vector(c)) for c in o.bound_box])
        lo, hi = bb.min(0), bb.max(0)
        d = np.maximum(0, np.maximum(lo - p, p - hi))
        if np.linalg.norm(d) <= radius:
            out.append(o)
    return out


def ctrl_col():
    c = bpy.data.collections.get('RIG_CONTROLLERS')
    if not c:
        c = bpy.data.collections.new('RIG_CONTROLLERS')
        bpy.context.scene.collection.children.link(c)
    return c


def frame(axis):
    ax = Vector(axis).normalized()
    up = Vector((0, 0, 1)) if abs(ax.z) < 0.9 else Vector((0, 1, 0))
    y = up.cross(ax).normalized()
    z = ax.cross(y)
    return Matrix((ax, y, z)).transposed()


def reparent(o, parent):
    bpy.context.view_layer.update()
    mw = o.matrix_world.copy()
    o.parent = parent
    o.matrix_parent_inverse = parent.matrix_world.inverted() if parent else Matrix()
    o.matrix_world = mw


def make_joint(name, kind, pivot, axis, parts, parent=None, source='', size=0.12):
    """kind 'hinge' (rotation about local X) or 'slide' (translation along local X).

    Two empties: FRAME.<name> holds the joint's fixed orientation (local X = axis) at the pivot;
    CTRL.<name> under it is the grabbable control. It has identity rest, every channel locked but
    rotation X (hinge) or location X (slide), and a Limit constraint once limits are set, so dragging
    it in the viewport moves the part exactly as the mechanism does."""
    base = name.split('.', 1)[1] if '.' in name else name
    fr = bpy.data.objects.get('FRAME.' + base) or bpy.data.objects.new('FRAME.' + base, None)
    e = bpy.data.objects.get('CTRL.' + base) or bpy.data.objects.new('CTRL.' + base, None)
    for o in (fr, e):
        if o.name not in ctrl_col().objects:
            ctrl_col().objects.link(o)
    fr.empty_display_type = 'PLAIN_AXES'; fr.empty_display_size = size * 0.4; fr.hide_select = True
    bpy.context.view_layer.update()
    fr.parent = parent
    fr.matrix_parent_inverse = parent.matrix_world.inverted() if parent else Matrix()
    fr.matrix_world = Matrix.Translation(Vector(pivot)) @ frame(axis).to_4x4()
    bpy.context.view_layer.update()
    e.parent = fr; e.matrix_parent_inverse = Matrix()
    e.rotation_mode = 'XYZ'; e.location = (0, 0, 0); e.rotation_euler = (0, 0, 0)
    e.delta_rotation_euler = (0, 0, 0); e.scale = (1, 1, 1)
    e.empty_display_type = 'CIRCLE' if kind == 'hinge' else 'SINGLE_ARROW'
    e.empty_display_size = size
    e.lock_location = (True, True, True) if kind == 'hinge' else (False, True, True)
    e.lock_rotation = (False, True, True) if kind == 'hinge' else (True, True, True)
    e.lock_scale = (True, True, True)
    bpy.context.view_layer.update()
    e['rig_kind'] = kind
    e['unit'] = 'deg' if kind == 'hinge' else 'm'
    e['closed'] = 0.0
    e['source'] = source
    for p in parts:
        if p is not None:
            reparent(p, e)
    return e


def set_joint(e, value):
    if e['rig_kind'] == 'hinge':
        e.rotation_euler.x = math.radians(value)
    else:
        e.location.x = value
    bpy.context.view_layer.update()


def joint_frame(e):
    return e.parent if e.parent and e.parent.name.startswith('FRAME.') else None


def detach_all(e):
    """Hand the joint's parts back to the frame's parent at their current pose and delete the joint."""
    fr = joint_frame(e)
    parts = [c for c in e.children]
    keep = fr.parent if fr else e.parent
    for p in parts:
        reparent(p, keep)
    for o in (e, fr):
        if o is not None:
            bpy.data.objects.remove(o, do_unlink=True)
    return parts, keep


def bvh(o):
    M = o.matrix_world
    verts = [M @ v.co for v in o.data.vertices]
    polys = [tuple(p.vertices) for p in o.data.polygons]
    return BVHTree.FromPolygons(verts, polys)


def moving_parts(e):
    return [o for o in e.children_recursive if o.type == 'MESH']


def sweep(e, lo, hi, step=1.0, radius=0.6, ignore=()):
    """Sweep the joint from 0 toward hi (and lo); return the last free value each way.
    Static parts touching the moving parts at rest are ignored (hinge contacts)."""
    mov = moving_parts(e)
    lim = e.constraints.get('rig_limit')
    was = lim.mute if lim else None
    if lim:
        lim.mute = True
    set_joint(e, 0.0)
    pivot = e.matrix_world.translation
    reach = max((Vector(c) - pivot).length for o in mov for c in [o.matrix_world @ Vector(b) for b in o.bound_box])
    stat = [o for o in near_parts(pivot, reach + radius, exclude=set(mov) | set(ignore))]
    stat_t = {o.name: bvh(o) for o in stat}
    mov_t = [bvh(o) for o in mov]
    rest = {n for n, t in stat_t.items() if any(t.overlap(m) for m in mov_t)}
    result = {}
    for direction, end in (('max', hi), ('min', lo)):
        last = 0.0
        if end == 0:
            result[direction] = 0.0
            continue
        n = int(abs(end) / step)
        hit_at, hit_by = None, None
        for i in range(1, n + 1):
            v = math.copysign(i * step, end)
            set_joint(e, v)
            mt = [bvh(o) for o in mov]
            hits = [nm for nm, t in stat_t.items() if nm not in rest and any(t.overlap(m) for m in mt)]
            if hits:
                hit_at, hit_by = v, hits
                break
            last = v
        result[direction] = last
        result[direction + '_hit'] = (hit_at, [clean(h) for h in (hit_by or [])][:4])
    if lim:
        lim.mute = was
    set_joint(e, 0.0)
    result['ignored_rest_contacts'] = [clean(n) for n in rest][:8]
    return result


def limits(e, lo, hi, source):
    """Record the range and enforce it on the control with a Limit constraint (local space)."""
    e['min'] = float(lo)
    e['max'] = float(hi)
    e['limit_source'] = source
    kind = e['rig_kind']
    cname = 'rig_limit'
    c = e.constraints.get(cname)
    if c is None:
        c = e.constraints.new('LIMIT_ROTATION' if kind == 'hinge' else 'LIMIT_LOCATION')
        c.name = cname
    c.owner_space = 'LOCAL'
    if kind == 'hinge':
        c.use_limit_x = True; c.min_x = math.radians(lo); c.max_x = math.radians(hi)
    else:
        c.use_min_x = True; c.use_max_x = True; c.min_x = lo; c.max_x = hi
    c.use_transform_limit = True


def gameplay(e, interaction='', damage='', repair=''):
    if interaction:
        e['interaction'] = interaction
    if damage:
        e['damage'] = damage
    if repair:
        e['repair'] = repair
