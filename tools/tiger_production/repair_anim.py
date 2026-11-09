"""Repairs as animated sequences of player interactions (owner, 2026-11-08: "with each repair i
want you to animate the repair within blender as a sequence of interactions performed by the
player that we can bring into the game. tools and player manipulation should be included").

How it is laid out
- One Action per repair, `REPAIR.<id>`, with one slot per object that moves in it: rig controls,
  fastener objects, tools, the player's two hand targets and standing point.
- A repair is a run of steps. Each step is a frame range in the action, marked by a pose marker
  `step:<name>`; the step's meaning for the game (what the player does, with which tool, whether
  the motion is one shot or a cycle the player repeats, what must be true before it) is JSON on
  the action (`repair_json`).
- Every repair sits on the scene's one timeline at its own block of frames, as NLA strips. Each
  action is keyed with the rest pose on the frame before its first and the frame after its last,
  and its strips hold, so the tank is at rest between repairs whatever a repair leaves open, and
  scrubbing the timeline shows any of them. Scene markers name the repairs and their steps.
- `export()` samples every step at the scene frame rate and writes a table the game reads: joint
  values for hinges and slides, position and rotation in the tank's frame for everything else.

What is animated and what is not
- Parts, fasteners and tools move exactly as the rig's controls allow. Hands are targets (where a
  palm is and how it is turned), baked to follow whatever they hold; the game's arms reach for
  them. There is no body animation: `PLAYER.Stand` is where the feet are and which way they face.
- A step of kind 'cycle' holds one stroke (one swing of a wrench, one turn of a crank) and a
  count; the game repeats it at the player's pace. On the timeline it plays once, and the next
  step starts from the finished state.
- Named approximations: hand poses are blocking, not studied grips; tools the kit does not have
  (spanners) are plain stand-ins sized to the head they fit; turns to undo a fastener come from a
  guessed thread (see fasteners.py), one diameter of engagement and the DIN coarse pitch.

Use (inside the open rig file):
    import repair_anim as ra
    r = ra.Repair('starter_hand_crank', row='E9', title='Hand start at the rear plate')
    r.stand((0.3, 4.6, 0.0), yaw_deg=180)
    with r.step('undo_cover_bolt', kind='cycle', seconds=1.0, tool='TOOL.Spanner_16', repeats=40): ...
    r.finish()
"""
import json
import math

import bpy
from mathutils import Matrix, Quaternion, Vector

FPS = 30
GAP = 30          # frames of rest between repairs on the timeline
PLAYER_COL = 'REPAIR_PLAYER'
TOOL_COL = 'REPAIR_TOOLS'
PITCH_MM = {'M4': 0.7, 'M5': 0.8, 'M6': 1.0, 'M8': 1.25, 'M10': 1.5, 'M12': 1.75, 'M14': 2.0, 'M16': 2.0, 'M18': 2.5,
            'M20': 2.5, 'M22': 2.5, 'M24': 3.0, 'M27': 3.0, 'M30': 3.5}


# ------------------------------------------------------------------------------- scene set-up
def _col(name):
    c = bpy.data.collections.get(name)
    if c is None:
        c = bpy.data.collections.new(name)
        bpy.context.scene.collection.children.link(c)
    return c


def _box(name, size, offset=(0.0, 0.0, 0.0)):
    import bmesh
    me = bpy.data.meshes.get(name) or bpy.data.meshes.new(name)
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation(offset) @ Matrix.Diagonal((*size, 1.0)))
    bm.to_mesh(me)
    bm.free()
    return me


def ensure_player():
    """PLAYER.Stand (feet, local +Y the way the player faces), HAND.L and HAND.R (palm targets:
    local +Y along the fingers, -Z out of the palm), each with a plain preview mesh."""
    col = _col(PLAYER_COL)
    made = {}
    for name, kind, size in (('PLAYER.Stand', 'ARROWS', 0.3), ('HAND.L', 'ARROWS', 0.06), ('HAND.R', 'ARROWS', 0.06)):
        o = bpy.data.objects.get(name)
        if o is None:
            o = bpy.data.objects.new(name, None)
            col.objects.link(o)
        o.empty_display_type, o.empty_display_size = kind, size
        o.rotation_mode = 'QUATERNION'
        made[name] = o
    for name, parent, size, offset, colour in (('PLAYER.Body.preview', 'PLAYER.Stand', (0.46, 0.26, 1.75), (0.0, 0.0, 0.875), (0.3, 0.5, 0.9, 1.0)),
                                               ('HAND.L.preview', 'HAND.L', (0.09, 0.19, 0.035), (0.0, 0.03, 0.0175), (0.9, 0.5, 0.2, 1.0)),
                                               ('HAND.R.preview', 'HAND.R', (0.09, 0.19, 0.035), (0.0, 0.03, 0.0175), (0.9, 0.3, 0.2, 1.0))):
        o = bpy.data.objects.get(name)
        if o is None:
            o = bpy.data.objects.new(name, _box(name, size, offset))
            col.objects.link(o)
            o.parent = bpy.data.objects[parent]
        o.color = colour
        o.hide_render = True
        o['preview_only'] = True
        o.display_type = 'SOLID'
        mat = bpy.data.materials.get('PREVIEW.' + name.split('.')[0])
        if mat is None:
            mat = bpy.data.materials.new('PREVIEW.' + name.split('.')[0])
        mat.diffuse_color = (*colour[:3], 0.3 if name.startswith('PLAYER.Body') else 1.0)
        if not o.data.materials:
            o.data.materials.append(mat)
        o.show_transparent = True
    if made['PLAYER.Stand'].get('placed') is None:
        made['PLAYER.Stand'].location = (0.0, 6.0, 0.0)
        made['PLAYER.Stand']['placed'] = True
        for side, x in (('L', -0.25), ('R', 0.25)):
            made['HAND.' + side].location = (x, 6.15, 0.95)
    return made


def ensure_spanner(across_flats_m):
    """A plain open-ended spanner for a head of the given size: origin at the jaw's middle, local
    +Z along the fastener, +X down the shaft to the hand. Stand-in for a tool the kit lacks."""
    import bmesh
    mm = round(across_flats_m * 1000.0)
    name = 'TOOL.Spanner_%d' % mm
    o = bpy.data.objects.get(name)
    if o is not None:
        return o
    length = max(0.16, across_flats_m * 11.0)
    t = max(0.006, across_flats_m * 0.35)
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    for size, offset in (((length - across_flats_m, across_flats_m * 0.9, t), (across_flats_m * 0.5 + (length - across_flats_m) * 0.5 + across_flats_m * 0.5, 0.0, 0.0)),
                         ((across_flats_m * 1.3, across_flats_m * 0.55, t), (across_flats_m * 0.1, across_flats_m * 0.8, 0.0)),
                         ((across_flats_m * 1.3, across_flats_m * 0.55, t), (across_flats_m * 0.1, -across_flats_m * 0.8, 0.0)),
                         ((across_flats_m * 0.5, across_flats_m * 2.15, t), (across_flats_m * 0.9, 0.0, 0.0))):
        bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation(offset) @ Matrix.Diagonal((*size, 1.0)))
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new(name, me)
    _col(TOOL_COL).objects.link(o)
    o.rotation_mode = 'QUATERNION'
    o['tool_kind'] = 'spanner'
    o['tool_size_m'] = across_flats_m
    o['tool_stand_in'] = True
    o['grip_local'] = (length * 0.8, 0.0, 0.0)
    o.color = (0.25, 0.25, 0.28, 1.0)
    o.location = (0.0, 6.4, 0.9)
    return o


def ensure_crank_handle(throw_m=0.2, bore_m=0.05):
    """A plain cranked handle that slips on the end of a shaft: origin on the shaft's axis at its
    end, local +Z along the shaft away from the work, the throw along +X, the grip beyond it
    along +Z. Stand-in: the kit has the starting shaft but no handle for it."""
    import bmesh
    name = 'TOOL.Crank_Handle'
    o = bpy.data.objects.get(name)
    if o is not None:
        return o
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    for size, offset in (((bore_m * 1.3, bore_m * 1.3, 0.07), (0.0, 0.0, -0.02)), ((throw_m, 0.03, 0.02), (throw_m * 0.5, 0.0, 0.02)),
                         ((0.032, 0.032, 0.14), (throw_m, 0.0, 0.1))):
        bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation(offset) @ Matrix.Diagonal((*size, 1.0)))
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new(name, me)
    _col(TOOL_COL).objects.link(o)
    o.rotation_mode = 'QUATERNION'
    o['tool_kind'] = 'crank_handle'
    o['tool_stand_in'] = True
    o['throw_m'] = throw_m
    o['grip_local'] = (throw_m, 0.0, 0.1)
    o.color = (0.25, 0.25, 0.28, 1.0)
    o.location = (0.4, 6.4, 0.9)
    return o


def _stand_in(name, boxes, kind, grip, park, extra=None):
    import bmesh
    o = bpy.data.objects.get(name)
    if o is not None:
        return o
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    for size, offset in boxes:
        bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation(offset) @ Matrix.Diagonal((*size, 1.0)))
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new(name, me)
    _col(TOOL_COL).objects.link(o)
    o.rotation_mode = 'QUATERNION'
    o['tool_kind'], o['tool_stand_in'], o['grip_local'] = kind, True, tuple(grip)
    for k, v in (extra or {}).items():
        o[k] = v
    o.color = (0.25, 0.25, 0.28, 1.0)
    o.location = park
    return o


def ensure_drift():
    """A plain pin drift: origin at the working tip, local +Z up the shank to the struck end, 0.25 m.
    Stand-in: the kit has none."""
    return _stand_in('TOOL.Drift', [((0.022, 0.022, 0.25), (0.0, 0.0, 0.125))], 'drift', (0.0, 0.0, 0.12), (0.8, 6.4, 0.9), {'length_m': 0.25})


def ensure_track_wrench():
    """A plain T-handled socket wrench for the track adjuster: origin at the working end, local +Z
    up the shaft (0.5 m), the handle across X at the top. Stand-in: the kit has none."""
    return _stand_in('TOOL.Track_Wrench', [((0.03, 0.03, 0.5), (0.0, 0.0, 0.25)), ((0.44, 0.03, 0.03), (0.0, 0.0, 0.5))], 'track_wrench',
                     (0.18, 0.0, 0.5), (1.2, 6.4, 0.9), {'grip2_local': (-0.18, 0.0, 0.5)})


def kit_tools():
    """The kit's own hand tools, with where a hand takes each and where it works (own space)."""
    out = {}
    for key, kind, grip, tip, grip2 in (('35O21_HAMMER', 'hammer', (0.0, 0.017, -0.60), (0.0, 0.196, -0.035), (0.0, 0.017, -0.30)),
                                        ('32O06_TOOL', 'crowbar', (0.0, -0.45, 0.0), (0.0, -1.76, 0.0), (0.0, -0.95, 0.0)),
                                        ('32O12_AXE', 'axe', (0.0, -0.02, -0.62), (0.0, -0.24, -0.03), (0.0, -0.02, -0.32)),
                                        ('34O26_SHOVEL', 'shovel', (0.0, -0.12, 0.01), (0.0, -0.95, 0.01), (0.0, -0.5, 0.01)),
                                        ('32O20_PLIERS', 'wire_cutters', (0.05, 0.0, -0.4), (0.0, 0.0, 0.09), (-0.05, 0.0, -0.4))):
        o = next(x for x in bpy.data.objects if x.type == 'MESH' and key in x.name)
        o['tool_kind'], o['grip_local'], o['tip_local'], o['grip2_local'] = kind, grip, tip, grip2
        out[kind] = o.name
    return out


def register_tool(obj_name, kind, grip_local, tip_local=(0.0, 0.0, 0.0)):
    """A kit object used as a tool (hammer, crowbar): where a hand takes it and where it works,
    in its own space."""
    o = bpy.data.objects[obj_name]
    o['tool_kind'] = kind
    o['grip_local'] = tuple(grip_local)
    o['tip_local'] = tuple(tip_local)
    return o


# ------------------------------------------------------------------------------------ helpers
def smooth(t):
    t = min(max(t, 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)


def pose(loc, z_axis=(0, 0, 1), x_hint=(1, 0, 0)):
    """World matrix at `loc` with local +Z along z_axis and +X as near x_hint as it can be."""
    z = Vector(z_axis).normalized()
    x = Vector(x_hint)
    x = x - z * x.dot(z)
    if x.length < 1.0e-6:
        x = z.orthogonal()
    x.normalize()
    y = z.cross(x)
    m = Matrix((x, y, z)).transposed().to_4x4()
    m.translation = Vector(loc)
    return m


def pose_x(loc, x_axis, z_hint=(0, 0, 1)):
    """World matrix at `loc` with local +X along x_axis and +Z as near z_hint as it can be."""
    x = Vector(x_axis).normalized()
    z = Vector(z_hint)
    z = z - x * z.dot(x)
    if z.length < 1.0e-6:
        z = x.orthogonal()
    z.normalize()
    y = z.cross(x)
    m = Matrix((x, y, z)).transposed().to_4x4()
    m.translation = Vector(loc)
    return m


def hand_pose(point, palm_to, fingers=(0, 0, 1)):
    """A palm at `point` pressing toward `palm_to` (a direction), fingers along `fingers`."""
    return pose(point, z_axis=-Vector(palm_to), x_hint=Vector(fingers).cross(-Vector(palm_to)))


def lerp_pose(a, b, t, arc=0.0):
    la, qa, _ = a.decompose()
    lb, qb, _ = b.decompose()
    m = qa.slerp(qb, t).to_matrix().to_4x4()
    m.translation = la.lerp(lb, t) + Vector((0.0, 0.0, arc * 4.0 * t * (1.0 - t)))
    return m


def rest_basis(o):
    """An object's pose when no repair is running (its own channels, in its parent's space). Kept
    on the object the first time a repair uses it, while nothing animates it yet: Blender gives
    an animated channel its default, not its old value, wherever no strip covers it, so the rest
    pose has to be keyed in and has to be known."""
    if 'rest_basis' not in o:
        bpy.context.view_layer.update()
        o['rest_basis'] = [c for row in o.matrix_basis for c in row]
        o['rest_world'] = [c for row in o.matrix_world for c in row]
    v = list(o['rest_basis'])
    return Matrix([v[0:4], v[4:8], v[8:12], v[12:16]])


def _rest_value(o, path, index):
    m = rest_basis(o)
    if path == 'location':
        return m.translation[index]
    if path == 'rotation_quaternion':
        return m.to_quaternion()[index]
    if path == 'rotation_euler':
        return m.to_euler(o.rotation_mode)[index]
    if path == 'scale':
        return m.to_scale()[index]
    return 0.0


def reset_object(o):
    """Put an object back on its rest pose (after its last strip is removed)."""
    if 'rest_basis' in o:
        scale = o.scale.copy()
        o.matrix_basis = rest_basis(o)
        o.scale = scale


def _registry():
    return json.loads(bpy.context.scene.get('repairs_json', '{}'))


def _set_registry(reg):
    bpy.context.scene['repairs_json'] = json.dumps(reg)


# -------------------------------------------------------------------------------------- Repair
class _Step:
    def __init__(self, repair, meta):
        self.r, self.meta = repair, meta

    def __enter__(self):
        return self.r

    def __exit__(self, *exc):
        self.r.cursor = self.meta['f1'] + 1
        return False


class Repair:
    def __init__(self, rid, row, title, note=''):
        self.id, self.row, self.title, self.note = rid, row, title, note
        self.steps, self.cursor = [], 1
        self.scalar = {}      # (object, path, index) -> [(frame, value, interp)]
        self.paths = []       # world-pose records, resolved at finish in this order
        self.order = []       # objects in order of first use
        self.events = []
        self.cur = None
        self.rest = {}        # objects whose transform this repair touches -> their rest basis
        ensure_player()
        bpy.context.scene.render.fps = FPS

    # ---- steps
    def step(self, sid, kind='oneshot', seconds=1.0, tool='', player='', repeats=1, requires='', sets='', input=''):
        n = max(2, round(seconds * FPS))
        meta = {'id': sid, 'kind': kind, 'f0': self.cursor, 'f1': self.cursor + n - 1, 'seconds': n / FPS, 'tool': tool,
                'player': player, 'repeats': repeats, 'requires': requires, 'sets': sets, 'input': input}
        self.steps.append(meta)
        self.cur = meta
        return _Step(self, meta)

    def _f(self, frac):
        m = self.cur
        return m['f0'] + (m['f1'] - m['f0']) * frac

    def _use(self, name):
        if name not in bpy.data.objects:
            raise KeyError('repair %s: no object %s' % (self.id, name))
        if name not in self.order:
            self.order.append(name)
        o = bpy.data.objects[name]
        if name.startswith(('HAND.', 'TOOL.', 'PLAYER.')) and o.rotation_mode != 'QUATERNION':
            o.rotation_mode = 'QUATERNION'
        rest_basis(o)
        return o

    def _key(self, name, path, index, frame, value, interp='BEZIER'):
        self._use(name)
        self.scalar.setdefault((name, path, index), []).append((float(frame), float(value), interp))

    def event(self, name, at=0.5):
        self.events.append((round(self._f(at)), name))

    # ---- rig joints
    def joint(self, ctrl, value, a=0.0, b=1.0, start=None):
        """Hinge (degrees) or slide (metres) to `value`, from where it is (or from `start`)."""
        e = self._use(ctrl)
        hinge = e['rig_kind'] == 'hinge'
        path, conv = ('rotation_euler', math.radians) if hinge else ('location', float)
        keys = self.scalar.get((ctrl, path, 0))
        if start is None:
            start = (math.degrees(keys[-1][1]) if hinge else keys[-1][1]) if keys else 0.0
        self._key(ctrl, path, 0, self._f(a), conv(start))
        self._key(ctrl, path, 0, self._f(b), conv(value))

    def turn(self, ctrl, degrees, a=0.0, b=1.0):
        """Turn a continuous hinge by `degrees` from where it is, at an even rate."""
        self._use(ctrl)
        keys = self.scalar.get((ctrl, 'rotation_euler', 0))
        start = keys[-1][1] if keys else 0.0
        self._key(ctrl, 'rotation_euler', 0, self._f(a), start, 'LINEAR')
        self._key(ctrl, 'rotation_euler', 0, self._f(b), start + math.radians(degrees), 'LINEAR')

    def unscrew(self, fastener, turns, lift_m, a=0.0, b=1.0):
        """Turn a fastener about its own axis (anticlockwise seen from the head) and draw it out."""
        o = bpy.data.objects[fastener]
        self._zyx(o)
        self._use(fastener)
        rest = rest_basis(o)
        keys = self.scalar.get((fastener, 'rotation_euler', 2))
        z0 = keys[-1][1] if keys else rest.to_euler('ZYX').z
        self._key(fastener, 'rotation_euler', 2, self._f(a), z0, 'LINEAR')
        self._key(fastener, 'rotation_euler', 2, self._f(b), z0 + turns * 2.0 * math.pi, 'LINEAR')
        axis = (rest.to_3x3() @ Vector((0.0, 0.0, 1.0))).normalized()
        for i in range(3):
            keys = self.scalar.get((fastener, 'location', i))
            p0 = keys[-1][1] if keys else rest.translation[i]
            self._key(fastener, 'location', i, self._f(a), p0, 'LINEAR')
            self._key(fastener, 'location', i, self._f(b), p0 + axis[i] * lift_m, 'LINEAR')

    def spin_lift(self, ctrl, turns, lift_m, a=0.0, b=1.0):
        """Unscrew a stowed part that sits flat (a filler cap): turn its control about its own
        upright axis and raise it."""
        self._use(ctrl)
        for path, index, delta in (('rotation_euler', 2, turns * 2.0 * math.pi), ('location', 2, lift_m)):
            keys = self.scalar.get((ctrl, path, index))
            v0 = keys[-1][1] if keys else 0.0
            self._key(ctrl, path, index, self._f(a), v0, 'LINEAR')
            self._key(ctrl, path, index, self._f(b), v0 + delta, 'LINEAR')

    def _zyx(self, o):
        """Fasteners turn in ZYX order, where the Z channel is a spin about the fastener's own axis."""
        if o.rotation_mode != 'ZYX':
            if o.animation_data and any(len(t.strips) for t in o.animation_data.nla_tracks):
                raise RuntimeError(o.name + ' is already animated in another rotation mode')
            bpy.context.view_layer.update()
            basis = o.matrix_basis.copy()
            scale = o.scale.copy()
            o.rotation_mode = 'ZYX'
            o.matrix_basis = basis
            o.scale = scale
            bpy.context.view_layer.update()

    # ---- world-pose records (resolved in finish)
    def place(self, name, to, a=0.0, b=1.0, arc=0.0):
        """Move an object (a stowed part's control, a tool, a fastener) from where it is to the
        world pose `to` (a Matrix, or a location to keep its present turn), over an arc."""
        self._use(name)
        self.paths.append({'obj': name, 'f0': round(self._f(a)), 'f1': round(self._f(b)), 'kind': 'place', 'to': to, 'arc': arc})

    def start_at(self, name, world):
        """Where a tool or loose part lies when the repair begins."""
        self._use(name)
        self.paths.append({'obj': name, 'f0': 1, 'f1': 1, 'kind': 'place', 'to': world, 'arc': 0.0})

    def follow(self, name, target, offset=None, a=0.0, b=1.0, reach=0.25, orient=None, hold=False):
        """Keep `name` fixed to `target` (another object) through the range, at `offset` in the
        target's space (default: as they stand at the range's first frame). `reach` is the share
        of a second spent getting there from wherever it was. With `hold` it stays so after the
        range too, until the object's next move or the repair's end (a bolt in a pocket)."""
        self._use(name)
        if target in bpy.data.objects and not target.startswith(('TG_', 'FRAME.')):
            self._use(target)
        self.paths.append({'obj': name, 'f0': round(self._f(a)), 'f1': round(self._f(b)), 'kind': 'follow', 'target': target,
                           'offset': offset, 'reach': max(1, round(reach * FPS)), 'orient': orient, 'hold': hold})

    def hand(self, side, target, grip=None, a=0.0, b=1.0, reach=0.3, orient=None, hold=False):
        """Put a hand on `target` for the range. `grip` is the palm's pose in the target's space;
        default: the target's `grip_local` point (a tool), else its origin. `orient` (a world
        Matrix) keeps the palm turned one way while the target turns in it (a crank's grip)."""
        t = bpy.data.objects[target]
        if grip is None:
            g = Vector(t.get('grip_local', (0.0, 0.0, 0.0)))
            if side == 'L' and 'grip2_local' in t:
                g = Vector(t['grip2_local'])
            grip = Matrix.Translation(g)
        self.follow('HAND.' + side, target, offset=grip, a=a, b=b, reach=reach, orient=orient, hold=hold)

    def stand(self, loc, yaw_deg=0.0, a=0.0, b=0.0):
        """Where the player's feet are (z on the ground or the deck) and which way they face
        (yaw 0 looks along +Y, to the tank's rear; 180 looks forward). Inside a step it walks
        there over the range; before the first step it is the starting place."""
        m = Matrix.Translation(Vector(loc)) @ Matrix.Rotation(math.radians(yaw_deg), 4, 'Z')
        self._use('PLAYER.Stand')
        if self.cur is None:
            self.paths.append({'obj': 'PLAYER.Stand', 'f0': 1, 'f1': 1, 'kind': 'place', 'to': m, 'arc': 0.0})
        else:
            self.paths.append({'obj': 'PLAYER.Stand', 'f0': round(self._f(a)), 'f1': round(self._f(max(b, a + 0.01))), 'kind': 'place', 'to': m, 'arc': 0.0})

    def bring(self, tool, side='R'):
        """The player arrives with this tool in hand."""
        x = 0.28 if side == 'R' else -0.28
        self._use(tool)
        off = Matrix.Translation((x, 0.3, 1.0)) @ Matrix.Rotation(math.radians(-90), 4, 'Y')
        self.paths.append({'obj': tool, 'f0': 1, 'f1': 1, 'kind': 'follow', 'target': 'PLAYER.Stand', 'offset': off, 'reach': 0, 'orient': None, 'hold': True})
        t = bpy.data.objects[tool]
        self.paths.append({'obj': 'HAND.' + side, 'f0': 1, 'f1': 1, 'kind': 'follow', 'target': tool,
                           'offset': Matrix.Translation(Vector(t.get('grip_local', (0.0, 0.0, 0.0)))), 'reach': 0, 'orient': None, 'hold': True})
        self._use('HAND.' + side)

    def carry(self, tool, a=0.0, b=1.0, side='R', hold=True):
        """A tool held at the player's side through the range, and after it until its next use."""
        x = 0.28 if side == 'R' else -0.28
        self.follow(tool, 'PLAYER.Stand', offset=Matrix.Translation((x, 0.3, 1.0)) @ Matrix.Rotation(math.radians(-90), 4, 'Y'), a=a, b=b, hold=hold)
        self.hand(side, tool, a=a, b=b, hold=hold)

    # ---- finishing
    def finish(self):
        sc = bpy.context.scene
        n = self.steps[-1]['f1']
        reg = _registry()
        _remove(self.id)
        reg = _registry()
        start = reg[self.id]['start'] if self.id in reg else (max([v['end'] for v in reg.values()], default=0) + GAP + 1)
        if self.id in reg and reg[self.id]['end'] - reg[self.id]['start'] + 1 < n:
            start = max(v['end'] for v in reg.values()) + GAP + 1  # it grew: move to the end
        act = bpy.data.actions.new('REPAIR.' + self.id)
        act.use_fake_user = True
        layer = act.layers.new('Layer')
        strip = layer.strips.new(type='KEYFRAME')
        hands = [x for x in self.order if x.startswith('HAND.')]
        tools = [x for x in self.order if x.startswith('TOOL.') or bpy.data.objects[x].get('tool_kind')]
        order = [x for x in self.order if x not in hands and x not in tools]
        # a part is resolved after every animated part it hangs under (a bolt after the cover it sits in)
        def depth(x):
            d, p = 0, bpy.data.objects[x].parent
            while p is not None:
                d += 1 if p.name in order else 0
                p = p.parent
            return d
        parts_sorted = sorted([x for x in order if x != 'PLAYER.Stand'], key=lambda x: (depth(x), order.index(x)))
        order = [x for x in order if x == 'PLAYER.Stand'] + parts_sorted + tools + hands
        # holds run on to the object's next record, or to the end
        by_obj = {}
        for p in self.paths:
            by_obj.setdefault(p['obj'], []).append(p)
        for recs in by_obj.values():
            recs.sort(key=lambda p: (p['f0'], p['f1']))
            for i, p in enumerate(recs):
                if p.get('hold'):
                    nxt = recs[i + 1] if i + 1 < len(recs) else None
                    p['f1'] = max(p['f1'], (nxt['f0'] - nxt.get('reach', 0) - 1) if nxt else n)
        # a hand follows the tool it carries only while that tool is not put to work or set down
        # a hand with nothing to hold hangs at the player's side
        for side, x in (('L', -0.25), ('R', 0.25)):
            name = 'HAND.' + side
            taken = sorted((max(1, p['f0'] - p.get('reach', 0)), p['f1']) for p in self.paths if p['obj'] == name)
            free, at = [], 1
            for lo, hi in taken:
                if lo > at:
                    free.append((at, lo - 1))
                at = max(at, hi + 1)
            if at <= n:
                free.append((at, n))
            for lo, hi in free:
                if hi - lo < 3 and lo > 1:
                    continue
                self.paths.append({'obj': name, 'f0': lo if lo == 1 else min(hi, lo + 8), 'f1': hi, 'kind': 'follow', 'target': 'PLAYER.Stand',
                                   'offset': Matrix.Translation((x, 0.15, 0.95)), 'reach': 0 if lo == 1 else 8})
            if name not in order:
                order.append(name)
        if 'PLAYER.Stand' not in order:
            order.insert(0, 'PLAYER.Stand')
        bags = {}
        for name in order:
            o = bpy.data.objects[name]
            slot = act.slots.new(id_type='OBJECT', name=name)
            bag = strip.channelbag(slot, ensure=True)
            bags[name] = bag
            ad = o.animation_data or o.animation_data_create()
            track = ad.nla_tracks.get('repairs') or ad.nla_tracks.new()
            track.name = 'repairs'
            st = track.strips.new(self.id, int(start) - 1, act)
            st.action_slot = slot
            st.use_sync_length = False
            st.action_frame_start, st.action_frame_end = 0, n + 1
            st.frame_start, st.frame_end = start - 1, start + n
            st.extrapolation = 'HOLD'
            st.blend_type = 'REPLACE'
        # scalar keys
        for (name, path, index), keys in self.scalar.items():
            fc = bags[name].fcurves.new(path, index=index)
            # hold the first value from the repair's first frame
            if keys[0][0] > 1:
                keys = [(1.0, keys[0][1], 'LINEAR')] + keys
            # hold the last value to the end, unless the object is moved by a world-pose record later
            moved_later = any(p['obj'] == name and p['f1'] > keys[-1][0] for p in self.paths)
            if keys[-1][0] < n and not moved_later:
                keys = keys + [(float(n), keys[-1][1], 'LINEAR')]
            for f, v, interp in keys:
                k = fc.keyframe_points.insert(f, v, options={'FAST'})
                k.interpolation = interp
            fc.update()
        # world-pose records, object by object in dependency order
        for name in order:
            recs = [p for p in self.paths if p['obj'] == name]
            if recs:
                self._bake(name, recs, bags[name], start, n)
        # the rest pose on the frame before and the frame after, on every channel; an object whose
        # rest is not the identity gets all its location and rotation channels, so that no channel
        # is ever left to Blender's default
        for name in order:
            o = bpy.data.objects[name]
            bag = bags[name]
            if not name.startswith('CTRL.'):
                rot = ('rotation_quaternion', 4) if o.rotation_mode == 'QUATERNION' else ('rotation_euler', 3)
                for path, count in (('location', 3), rot):
                    for i in range(count):
                        if bag.fcurves.find(path, index=i) is None:
                            bag.fcurves.new(path, index=i)
            for fc in bag.fcurves:
                v = _rest_value(o, fc.data_path, fc.array_index)
                if len(fc.keyframe_points) == 0:
                    fc.keyframe_points.insert(1, v, options={'FAST'}).interpolation = 'LINEAR'
                for f in (0, n + 1):
                    fc.keyframe_points.insert(f, v, options={'FAST'}).interpolation = 'LINEAR'
                fc.update()
        for m in self.steps:
            pm = act.pose_markers.new('step:' + m['id'])
            pm.frame = m['f0']
        for f, name in self.events:
            pm = act.pose_markers.new('ev:' + name)
            pm.frame = f
        meta = {'id': self.id, 'row': self.row, 'title': self.title, 'note': self.note, 'frames': n, 'fps': FPS, 'steps': self.steps,
                'events': [{'frame': f, 'name': nm} for f, nm in self.events], 'objects': order}
        act['repair_json'] = json.dumps(meta)
        reg[self.id] = {'start': int(start), 'end': int(start + n - 1), 'row': self.row, 'title': self.title}
        _set_registry(reg)
        _markers()
        sc.frame_set(1)
        return {'id': self.id, 'start': int(start), 'frames': n, 'seconds': round(n / FPS, 1), 'steps': len(self.steps), 'objects': len(order)}

    def _bake(self, name, recs, bag, start, n):
        """Resolve an object's world-pose records frame by frame against the evaluated scene."""
        sc = bpy.context.scene
        o = bpy.data.objects[name]
        euler = o.rotation_mode != 'QUATERNION' and not name.startswith(('HAND.', 'TOOL.', 'PLAYER.')) and not o.get('tool_kind')
        if not euler and o.rotation_mode != 'QUATERNION':
            bpy.context.view_layer.update()
            basis = o.matrix_basis.copy()
            o.rotation_mode = 'QUATERNION'
            o.matrix_basis = basis
        rot_path, rot_n = ('rotation_euler', 3) if euler else ('rotation_quaternion', 4)
        curves = {}
        for path, count in (('location', 3), (rot_path, rot_n)):
            for i in range(count):
                fc = bag.fcurves.find(path, index=i)
                if fc is not None and path == 'location' and not euler:
                    pass
                curves[(path, i)] = fc if fc is not None else bag.fcurves.new(path, index=i)
        recs = sorted(recs, key=lambda p: p['f0'])
        rest = rest_basis(o)
        last_q = None
        prev_e = None

        def write(local_f, basis):
            nonlocal last_q, prev_e
            loc, q, _ = basis.decompose()
            for i in range(3):
                curves[('location', i)].keyframe_points.insert(local_f, loc[i], options={'FAST'}).interpolation = 'LINEAR'
            if euler:
                e = q.to_euler(o.rotation_mode, prev_e) if prev_e is not None else q.to_euler(o.rotation_mode)
                prev_e = e
                for i in range(3):
                    curves[(rot_path, i)].keyframe_points.insert(local_f, e[i], options={'FAST'}).interpolation = 'LINEAR'
            else:
                if last_q is not None and last_q.dot(q) < 0.0:
                    q = -q
                last_q = q
                for i in range(4):
                    curves[(rot_path, i)].keyframe_points.insert(local_f, q[i], options={'FAST'}).interpolation = 'LINEAR'

        def to_basis(world):
            parent = (o.parent.matrix_world @ o.matrix_parent_inverse) if o.parent else Matrix.Identity(4)
            return parent.inverted() @ world

        # a channel with no keys yet starts on the rest pose (an empty curve would read as zero)
        for (path, i), fc in curves.items():
            if len(fc.keyframe_points) == 0:
                v = _rest_value(o, path, i)
                for f in (0, 1):
                    fc.keyframe_points.insert(f, v, options={'FAST'}).interpolation = 'LINEAR'
                fc.update()
        for rec in recs:
            f0, f1 = rec['f0'], rec['f1']
            if euler:
                sc.frame_set(int(start + max(1, f0 - rec.get('reach', 0)) - 1))
                prev_e = o.rotation_euler.copy()  # carry on from the turns already made
            if rec['kind'] == 'place':
                sc.frame_set(int(start + f0 - 1))
                a_world = o.matrix_world.copy()
                to = rec['to']
                if isinstance(to, Matrix):
                    b_world = to
                else:
                    b_world = a_world.copy()
                    b_world.translation = Vector(to)
                if f1 <= f0:
                    write(f0, to_basis(b_world))
                else:
                    for f in range(f0, f1 + 1):
                        sc.frame_set(int(start + f - 1))
                        write(f, to_basis(lerp_pose(a_world, b_world, smooth((f - f0) / (f1 - f0)), rec['arc'])))
            else:
                target = bpy.data.objects[rec['target']]
                offset = rec['offset']
                r0 = max(1, f0 - rec['reach'])
                sc.frame_set(int(start + r0 - 1))
                from_world = o.matrix_world.copy()
                if offset is None:
                    sc.frame_set(int(start + f0 - 1))
                    offset = _unscaled(target.matrix_world).inverted() @ _unscaled(o.matrix_world)
                for f in range(r0, f1 + 1):
                    sc.frame_set(int(start + f - 1))
                    want = _unscaled(target.matrix_world) @ offset
                    if rec.get('orient') is not None:
                        fixed = rec['orient'].to_3x3().to_4x4()
                        fixed.translation = want.translation
                        want = fixed
                    if f < f0:
                        want = lerp_pose(from_world, want, smooth((f - r0) / max(1, f0 - r0)))
                    write(f, to_basis(want))
            for fc in curves.values():
                fc.update()
        # hold the last resolved pose to the end, unless scalar keys carry on after it
        later = any(k.co[0] > recs[-1]['f1'] for fc in curves.values() for k in fc.keyframe_points)
        if recs[-1]['f1'] < n and not later:
            sc.frame_set(int(start + recs[-1]['f1'] - 1))
            write(n, o.matrix_basis.copy())
            for fc in curves.values():
                fc.update()


def _unscaled(m):
    loc, q, _ = m.decompose()
    out = q.to_matrix().to_4x4()
    out.translation = loc
    return out


def _remove(rid):
    """Take a repair off the timeline (its action and strips); its block of frames stays reserved."""
    act = bpy.data.actions.get('REPAIR.' + rid)
    for o in bpy.data.objects:
        ad = o.animation_data
        if ad is None:
            continue
        for tr in ad.nla_tracks:
            for st in list(tr.strips):
                if st.name == rid or (act is not None and st.action == act):
                    tr.strips.remove(st)
    if act is not None:
        bpy.data.actions.remove(act)
    for o in bpy.data.objects:
        ad = o.animation_data
        if ad is not None and 'rest_basis' in o and not any(len(t.strips) for t in ad.nla_tracks):
            reset_object(o)


def _markers():
    sc = bpy.context.scene
    for m in list(sc.timeline_markers):
        if m.name.startswith(('R:', 's:')):
            sc.timeline_markers.remove(m)
    reg = _registry()
    end = 1
    for rid, v in sorted(reg.items(), key=lambda kv: kv[1]['start']):
        sc.timeline_markers.new('R:' + rid, frame=v['start'])
        act = bpy.data.actions.get('REPAIR.' + rid)
        if act is not None:
            for st in json.loads(act['repair_json'])['steps']:
                if st['f0'] > 1:
                    sc.timeline_markers.new('s:' + st['id'], frame=v['start'] + st['f0'] - 1)
        end = max(end, v['end'])
    sc.frame_start, sc.frame_end = 1, end + GAP


def wipe():
    """Take every repair off the timeline and forget their frame blocks (before a full rebuild)."""
    for rid in list(_registry()):
        _remove(rid)
    for a in [a for a in bpy.data.actions if a.name.startswith('REPAIR.')]:
        _remove(a.name[7:])
    _set_registry({})
    _markers()
    # back to the kit pose by world matrix, parents first, and forget the stored rest poses: a
    # part may have been given a new parent control since they were taken
    def depth(o):
        d = 0
        while o.parent is not None:
            d, o = d + 1, o.parent
        return d
    kept = sorted([o for o in bpy.data.objects if 'rest_world' in o], key=depth)
    for o in kept:
        v = list(o['rest_world'])
        bpy.context.view_layer.update()
        o.matrix_world = Matrix([v[0:4], v[4:8], v[8:12], v[12:16]])
    bpy.context.view_layer.update()
    for o in bpy.data.objects:
        for k in ('rest_basis', 'rest_world'):
            if k in o:
                del o[k]


def focus(rid):
    """Set the preview range to one repair and go to its first frame."""
    sc = bpy.context.scene
    v = _registry()[rid]
    sc.use_preview_range = True
    sc.frame_preview_start, sc.frame_preview_end = v['start'], v['end']
    sc.frame_set(v['start'])
    return v


def listing():
    reg = _registry()
    return [(rid, v['row'], v['start'], v['end'], round((v['end'] - v['start'] + 1) / FPS, 1)) for rid, v in sorted(reg.items(), key=lambda kv: kv[1]['start'])]


# -------------------------------------------------------------------------------------- export
def export(path, only=None):
    """Sample every repair for the game. Per step and per object: `joint` (degrees or metres per
    frame) for hinges and slides, else `pos` and `quat` (w, x, y, z) per frame in the tank's frame
    (the file's world: metres, +Z up, -Y forward, +X left)."""
    sc = bpy.context.scene
    reg = _registry()
    out = {'format': 'tiger_repairs.v1', 'fps': FPS, 'frame': 'blender world of tiger_gen2_live.blend: metres, +Z up, -Y forward, +X left',
           'source': bpy.data.filepath, 'repairs': []}
    for rid, v in sorted(reg.items(), key=lambda kv: kv[1]['start']):
        if only and rid not in only:
            continue
        act = bpy.data.actions.get('REPAIR.' + rid)
        if act is None:
            continue
        meta = json.loads(act['repair_json'])
        names = [x for x in meta['objects'] if not bpy.data.objects[x].get('preview_only')]
        samples = {x: [] for x in names}
        for f in range(v['start'], v['end'] + 1):
            sc.frame_set(f)
            for x in names:
                o = bpy.data.objects[x]
                kind = o.get('rig_kind')
                if kind == 'hinge':
                    samples[x].append(round(math.degrees(o.rotation_euler.x), 4))
                elif kind == 'slide':
                    samples[x].append(round(o.location.x, 5))
                else:
                    loc, q, _ = o.matrix_world.decompose()
                    samples[x].append([round(c, 5) for c in (*loc, q.w, q.x, q.y, q.z)])
        rep = {'id': rid, 'row': meta['row'], 'title': meta['title'], 'note': meta['note'], 'frames': meta['frames'], 'steps': [], 'objects': {}}
        for x in names:
            o = bpy.data.objects[x]
            info = {'kind': o.get('rig_kind') or ('fastener' if 'fastener_kind' in o else 'tool' if o.get('tool_kind') else 'hand' if x.startswith('HAND.') else 'stand' if x == 'PLAYER.Stand' else 'part')}
            for k in ('fastener_kind', 'fastener_set', 'tool_kind', 'tool_size_m', 'tool_stand_in', 'continuous'):
                if k in o:
                    info[k] = o[k]
            rep['objects'][x] = info
        for st in meta['steps']:
            tracks = {}
            for x in names:
                seg = samples[x][st['f0'] - 1:st['f1']]
                if any(s != seg[0] for s in seg) or x.startswith(('HAND.', 'PLAYER.')) or x == st.get('tool'):
                    key = 'joint' if bpy.data.objects[x].get('rig_kind') in ('hinge', 'slide') else 'xform'
                    tracks[x] = {key: seg}
                else:
                    tracks[x] = {'hold': seg[0]}
            rep['steps'].append({**st, 'events': [e for e in meta['events'] if st['f0'] <= e['frame'] <= st['f1']], 'tracks': tracks})
        out['repairs'].append(rep)
    sc.frame_set(1)
    with open(path, 'w') as fh:
        json.dump(out, fh, separators=(',', ':'))
    return {'path': path, 'repairs': len(out['repairs']), 'steps': sum(len(r['steps']) for r in out['repairs'])}


def turns_for(fastener_name):
    """(turns to free, lift in metres) for a fastener object: one diameter of engagement at the
    DIN coarse pitch of its guessed thread; six turns and 9 mm when no thread is guessed."""
    o = bpy.data.objects[fastener_name]
    g = o.get('fastener_thread_guess', '')
    if g in PITCH_MM:
        d = float(g[1:])
        return d / PITCH_MM[g], d / 1000.0
    return 6.0, 0.009


# ------------------------------------------------------------------------------------ viewing
def _view3d():
    for w in bpy.context.window_manager.windows:
        for a in w.screen.areas:
            if a.type == 'VIEW_3D':
                return w, a, [x for x in a.regions if x.type == 'WINDOW'][0]
    raise RuntimeError('no 3D view')


def render_frames(rid, out_dir, view, frames=None, size=(1280, 800), every=1):
    """Viewport stills of a repair (all its frames, or the listed local frames) from one fixed
    view: view = (direction to the camera, look-at point, distance). The caller checks first that
    the game is not running. Returns the file paths. The 3D view is put back afterwards."""
    import os
    sc = bpy.context.scene
    w, a, reg3d = _view3d()
    s = a.spaces.active
    r = s.region_3d
    v = _registry()[rid]
    keep = (r.view_location.copy(), r.view_rotation.copy(), r.view_distance, r.view_perspective, s.overlay.show_overlays, s.shading.show_xray,
            sc.render.resolution_x, sc.render.resolution_y, sc.render.resolution_percentage, sc.render.filepath, sc.render.image_settings.file_format, sc.frame_current)
    direction, look, dist = view
    r.view_perspective = 'PERSP'
    r.view_rotation = Vector(direction).normalized().to_track_quat('Z', 'Y')
    r.view_location, r.view_distance = Vector(look), dist
    s.overlay.show_overlays, s.shading.show_xray = False, False
    sc.render.resolution_x, sc.render.resolution_y, sc.render.resolution_percentage = size[0], size[1], 100
    sc.render.image_settings.file_format = 'JPEG'
    os.makedirs(out_dir, exist_ok=True)
    local = frames if frames is not None else list(range(1, v['end'] - v['start'] + 2, every))
    paths = []
    selected = [o for o in bpy.context.selected_objects]
    for o in selected:
        o.select_set(False)
    try:
        for i, f in enumerate(local):
            sc.frame_set(int(v['start'] + f - 1))
            sc.render.filepath = os.path.join(out_dir, '%s_%04d.jpg' % (rid, i))
            with bpy.context.temp_override(window=w, area=a, region=reg3d):
                bpy.ops.render.opengl(write_still=True, view_context=True)
            paths.append(sc.render.filepath)
    finally:
        (r.view_location, r.view_rotation, r.view_distance, r.view_perspective, s.overlay.show_overlays, s.shading.show_xray,
         sc.render.resolution_x, sc.render.resolution_y, sc.render.resolution_percentage, sc.render.filepath, sc.render.image_settings.file_format) = keep[:11]
        sc.frame_set(keep[11])
        for o in selected:
            o.select_set(True)
    return paths


def step_mid_frames(rid, per_step=1):
    """Local frames at the middle (and end) of each step, for a contact sheet."""
    meta = json.loads(bpy.data.actions['REPAIR.' + rid]['repair_json'])
    out = []
    for st in meta['steps']:
        out.append(round((st['f0'] + st['f1']) / 2))
        if per_step > 1:
            out.append(st['f1'])
    return out


def check(rid):
    """Numbers to trust a repair by: how far each object travels (mm), whether everything is back
    on its rest pose after the block, and the pose error at the last frame of the last step."""
    sc = bpy.context.scene
    v = _registry()[rid]
    meta = json.loads(bpy.data.actions['REPAIR.' + rid]['repair_json'])
    names = [x for x in meta['objects']]
    parts = [x for x in names if not x.startswith(('HAND.', 'PLAYER.', 'TOOL.'))]
    sc.frame_set(max(1, v['start'] - 5))
    rest = {x: bpy.data.objects[x].matrix_world.copy() for x in names}
    far = {x: 0.0 for x in names}
    spun = {x: 0.0 for x in names}
    for f in range(v['start'], v['end'] + 1, 3):
        sc.frame_set(f)
        for x in names:
            far[x] = max(far[x], (bpy.data.objects[x].matrix_world.translation - rest[x].translation).length)
            spun[x] = max(spun[x], bpy.data.objects[x].matrix_world.to_quaternion().rotation_difference(rest[x].to_quaternion()).angle)
    sc.frame_set(v['end'])
    last = {x: (bpy.data.objects[x].matrix_world.translation - rest[x].translation).length for x in names}
    last_rot = {x: bpy.data.objects[x].matrix_world.to_quaternion().rotation_difference(rest[x].to_quaternion()).angle for x in names}
    sc.frame_set(v['end'] + 5)
    after = max((bpy.data.objects[x].matrix_world.translation - rest[x].translation).length for x in names)
    sc.frame_set(1)
    return {'id': rid, 'seconds': round(meta['frames'] / FPS, 1), 'steps': len(meta['steps']),
            'travel_mm_and_turn_deg': {x.replace('TG_RM5010-', '')[:28]: (round(far[x] * 1000), round(math.degrees(spun[x]))) for x in parts},
            'not_home_at_last_frame': {x.replace('TG_RM5010-', '')[:28]: (round(last[x] * 1000, 1), round(math.degrees(last_rot[x]) % 360, 1)) for x in parts
                                       if last[x] > 0.0005 or 0.5 < math.degrees(last_rot[x]) % 360 < 359.5},
            'rest_after_block_mm': round(after * 1000, 3)}


def export_batch(path, first=0, budget_s=35.0):
    """`export` in pieces for a long timeline: writes the repairs from index `first` (in timeline
    order) until the time budget runs out, merging into the file at `path`. Returns the next index;
    call again until it equals the count. Start a fresh file with first=0."""
    import os
    import time
    ids = [r[0] for r in listing()]
    began = time.time()
    data = None
    if first > 0 and os.path.exists(path):
        data = json.load(open(path))
    i = first
    tmp = path + '.part'
    while i < len(ids) and time.time() - began < budget_s:
        export(tmp, only=[ids[i]])
        piece = json.load(open(tmp))
        if data is None:
            data = piece
        else:
            data['repairs'] = [r for r in data['repairs'] if r['id'] != ids[i]] + piece['repairs']
        i += 1
    if data is not None:
        order = {rid: k for k, rid in enumerate(ids)}
        data['repairs'].sort(key=lambda r: order.get(r['id'], 1e9))
        with open(path, 'w') as fh:
            json.dump(data, fh, separators=(',', ':'))
    if os.path.exists(tmp):
        os.remove(tmp)
    return {'next': i, 'count': len(ids), 'repairs_in_file': len(data['repairs']) if data else 0}
