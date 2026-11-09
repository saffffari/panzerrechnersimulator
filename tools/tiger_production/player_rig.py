"""The player's body in the rig file: the game's crew figure (Panzertruppe, AdrielCZ, CC-BY-4.0,
39 bones) standing on PLAYER.Stand and working with its hands on HAND.L and HAND.R.

    import player_rig; player_rig.build(path_to_german_panzertruppe_rigged.glb)

The figure faces the stand's +Y. Everything is constraints (no scripts run at playback):
- Bones of the arms and legs are first pointed at their children (the importer leaves every bone
  standing upright); this changes no skinning.
- Shoulders. PLAYER.Shoulders starts at the standing shoulder height and is pulled toward the
  midpoint of the two hands only as far as needed to bring it within REACH_M of them, and never
  more than TRAVEL_M from standing. With the hands near, the body stands straight.
- Hips hang under the shoulders: they follow them down in full (a crouch, to a floor of
  CROUCH_M below standing) and 35 % of the way sideways and forward. The lower spine points at
  the shoulders, so what the hips cannot follow becomes a lean.
- Feet are pinned flat to the ground under the stand; knees bend forward (pole in front).
- Each wrist is pulled to its hand target by two-bone IK with the elbow kept down and back (pole
  behind the body), and takes 70 % of the target's turn.
- The head looks 60 % of the way toward the work.
Named approximations: no finger poses, no stepping or weight shift between the feet, no
anticipation or follow-through. It is a body that reaches and crouches believably for blocking,
and the base rig for hand-keyed or captured animation; it is not finished character animation.
"""
import math

import bpy
from mathutils import Matrix, Vector

HEIGHT_M = 1.72
REACH_M = 0.50        # shoulders come no nearer to the hands' midpoint than they must to be this close
TRAVEL_M = 0.80       # and never farther than this from where they are standing upright
CROUCH_M = 0.45       # how far the hips may sink
COL = 'CHARACTERS'
CHAINS = [('L_Arm_N1', 'L_Elbow_N1'), ('L_Elbow_N1', 'L_Wrist_N1'), ('L_Wrist_N1', 'L_Hand_N1'), ('R_Arm_N1', 'R_Elbow_N1'), ('R_Elbow_N1', 'R_Wrist_N1'),
          ('R_Wrist_N1', 'R_Hand_N1'), ('L_Leg_N1', 'L_Knee_N1'), ('L_Knee_N1', 'L_Ankle_N1'), ('L_Ankle_N1', 'L_Foot_N1'), ('R_Leg_N1', 'R_Knee_N1'),
          ('R_Knee_N1', 'R_Ankle_N1'), ('R_Ankle_N1', 'R_Foot_N1'), ('L_Shoulder_N1', 'L_Arm_N1'), ('R_Shoulder_N1', 'R_Arm_N1'),
          ('Spine_1_N1', 'Spine_2_N1'), ('Spine_2_N1', 'Chest_N1'), ('Chest_N1', 'Neck_N1'), ('Neck_N1', 'Head_N1')]


def _empty(name, parent, loc=(0, 0, 0), size=0.03, kind='PLAIN_AXES'):
    o = bpy.data.objects.get(name) or bpy.data.objects.new(name, None)
    col = bpy.data.collections[COL]
    if o.name not in col.objects:
        for c in list(o.users_collection):
            c.objects.unlink(o)
        col.objects.link(o)
    o.empty_display_type, o.empty_display_size = kind, size
    for c in list(o.constraints):
        o.constraints.remove(c)
    o.parent = parent
    o.matrix_parent_inverse = Matrix.Identity(4)
    o.location = loc
    o.rotation_mode = 'XYZ'
    o.rotation_euler = (0, 0, 0)
    return o


def _clear(pb):
    for c in list(pb.constraints):
        pb.constraints.remove(c)


def point_bones(arm):
    """Tails to children along the limbs and spine (edit mode)."""
    w = bpy.context.window_manager.windows[0]
    a = [x for x in w.screen.areas if x.type == 'VIEW_3D'][0]
    reg = [x for x in a.regions if x.type == 'WINDOW'][0]
    for o in bpy.context.selected_objects:
        o.select_set(False)
    arm.hide_set(False)
    arm.select_set(True)
    bpy.context.view_layer.objects.active = arm
    with bpy.context.temp_override(window=w, area=a, region=reg, active_object=arm, object=arm):
        bpy.ops.object.mode_set(mode='EDIT')
        eb = arm.data.edit_bones
        for parent, child in CHAINS:
            if (eb[child].head - eb[parent].head).length > 1.0e-4:
                eb[parent].tail = eb[child].head.copy()
        bpy.ops.object.mode_set(mode='OBJECT')
    arm.select_set(False)


def _pick_pole(arm, constraint, joint_bone, pole_obj):
    """The pole angle that brings the bent joint nearest its pole."""
    best = None
    for ang in (0.0, 90.0, 180.0, -90.0):
        constraint.pole_angle = math.radians(ang)
        bpy.context.view_layer.update()
        p = (arm.matrix_world @ arm.pose.bones[joint_bone].matrix).translation
        d = (p - pole_obj.matrix_world.translation).length
        if best is None or d < best[0]:
            best = (d, ang)
    constraint.pole_angle = math.radians(best[1])
    return best[1]


def build(glb_path=''):
    D = bpy.data.objects
    sc = bpy.context.scene
    col = bpy.data.collections.get(COL) or bpy.data.collections.new(COL)
    if col.name not in sc.collection.children:
        sc.collection.children.link(col)
    arm = D.get('PLAYER.Rig')
    if arm is None:
        before = set(D.keys())
        bpy.ops.import_scene.gltf(filepath=glb_path)
        new = [o for o in D if o.name not in before]
        for o in new:
            for c in list(o.users_collection):
                c.objects.unlink(o)
            col.objects.link(o)
        arm = next(o for o in new if o.type == 'ARMATURE')
        for o in new:
            if o.type == 'MESH' and o.parent is None:      # a stray sphere in the source file
                bpy.data.objects.remove(o, do_unlink=True)
        arm.name = 'PLAYER.Rig'
        for o in arm.children:
            o.name = 'PLAYER.' + o.name.replace('PanzerTruppe_', '').replace('_N1', '')
    stand = D['PLAYER.Stand']
    for pb in arm.pose.bones:
        _clear(pb)
        pb.location = (0, 0, 0)
        pb.rotation_quaternion = (1, 0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
    point_bones(arm)
    # the canonical frame: as imported the figure faces -Y with its left arm along +X
    arm.parent = None
    arm.matrix_world = Matrix.Rotation(math.radians(90.0), 4, 'X') @ Matrix.Scale(0.009, 4)
    bpy.context.view_layer.update()
    top = max((o.matrix_world @ v.co).z for o in arm.children if o.type == 'MESH' for v in o.data.vertices)
    k = HEIGHT_M / top
    canon = Matrix.Scale(k, 4) @ arm.matrix_world
    turn = Matrix.Rotation(math.pi, 4, 'Z')              # canonical -Y forward -> the stand's +Y forward
    arm.parent = stand
    arm.matrix_parent_inverse = Matrix.Identity(4)
    arm.matrix_basis = turn @ canon
    bpy.context.view_layer.update()
    bones = arm.data.bones
    inv = stand.matrix_world.inverted()
    head = lambda n: inv @ (arm.matrix_world @ bones[n].head_local)                   # in the stand's frame: +Y forward, +Z up
    orient = lambda n: (inv @ arm.matrix_world @ bones[n].matrix_local).to_3x3().normalized()
    unit = 0.009 * k
    report = {'height_scale': round(k, 4)}

    # ---- shoulders, hips, spine
    mid = _empty('PLAYER.HandsMid', None, size=0.04, kind='SPHERE')
    c1 = mid.constraints.new('COPY_LOCATION')
    c1.target = D['HAND.L']
    c2 = mid.constraints.new('COPY_LOCATION')
    c2.target = D['HAND.R']
    c2.influence = 0.5
    sh_rest = (head('L_Arm_N1') + head('R_Arm_N1')) * 0.5
    rest_e = _empty('PLAYER.ShoulderRest', stand, loc=tuple(sh_rest), size=0.04, kind='CUBE')
    sh = _empty('PLAYER.Shoulders', None, size=0.06, kind='SPHERE')
    a = sh.constraints.new('COPY_LOCATION')
    a.target = rest_e
    b = sh.constraints.new('LIMIT_DISTANCE')
    b.name = 'come within reach of the work'
    b.target = mid
    b.distance = REACH_M
    b.limit_mode = 'LIMITDIST_INSIDE'
    c = sh.constraints.new('LIMIT_DISTANCE')
    c.name = 'no farther than the body can go'
    c.target = rest_e
    c.distance = TRAVEL_M
    c.limit_mode = 'LIMITDIST_INSIDE'
    hips_rest = head('ROOT_N1')
    hips = _empty('PLAYER.HipsUnderShoulders', sh, loc=tuple(hips_rest - sh_rest), size=0.04, kind='SPHERE')
    rb = arm.pose.bones['ROOT_N1']
    s1 = rb.constraints.new('COPY_LOCATION')
    s1.name = 'hips drift toward the work'
    s1.target = hips
    s1.use_z = False
    s1.target_space = s1.owner_space = 'WORLD'
    s1.influence = 0.35
    s2 = rb.constraints.new('COPY_LOCATION')
    s2.name = 'hips sink with the shoulders'
    s2.target = hips
    s2.use_x = s2.use_y = False
    s2.target_space = s2.owner_space = 'WORLD'
    lim = rb.constraints.new('LIMIT_LOCATION')
    lim.name = 'crouch floor'
    lim.owner_space = 'POSE'
    lim.use_min_y = lim.use_max_y = True                 # the armature's Y is up
    lim.min_y = (hips_rest.z - CROUCH_M) / unit
    lim.max_y = hips_rest.z / unit
    sp = arm.pose.bones['Spine_1_N1']
    dt = sp.constraints.new('DAMPED_TRACK')
    dt.name = 'spine to the shoulders'
    dt.target = sh
    dt.track_axis = 'TRACK_Y'

    # ---- head looks toward the work
    hb = arm.pose.bones['Head_N1']
    fwd = orient('Head_N1').inverted() @ Vector((0.0, 1.0, 0.0))                       # the stand's forward in the head bone's own axes
    axis = max((('TRACK_X', fwd.x), ('TRACK_NEGATIVE_X', -fwd.x), ('TRACK_Z', fwd.z), ('TRACK_NEGATIVE_Z', -fwd.z), ('TRACK_Y', fwd.y), ('TRACK_NEGATIVE_Y', -fwd.y)), key=lambda t: t[1])[0]
    look = hb.constraints.new('DAMPED_TRACK')
    look.name = 'look at the work'
    look.target = mid
    look.track_axis = axis
    look.influence = 0.6
    report['head_forward_axis'] = axis

    # ---- legs: feet flat and planted, knees forward
    for side in ('L', 'R'):
        ankle = head(side + '_Ankle_N1')
        knee = head(side + '_Knee_N1')
        foot = _empty('PLAYER.Foot.' + side, stand, loc=tuple(ankle), size=0.05, kind='CUBE')
        flat = _empty('PLAYER.Foot.%s.flat' % side, foot)
        flat.rotation_mode = 'QUATERNION'
        flat.rotation_quaternion = orient(side + '_Ankle_N1').to_quaternion()
        pole = _empty('PLAYER.Knee.' + side, stand, loc=(knee.x * 1.3, knee.y + 0.7, knee.z + 0.05), size=0.04, kind='SPHERE')
        shin = arm.pose.bones[side + '_Knee_N1']
        fk = shin.constraints.new('IK')
        fk.name = 'planted'
        fk.target = foot
        fk.chain_count = 2
        fk.use_tail = True
        fk.use_stretch = False
        fk.pole_target = pole
        ab = arm.pose.bones[side + '_Ankle_N1']
        fr = ab.constraints.new('COPY_ROTATION')
        fr.name = 'foot flat'
        fr.target = flat
        fr.target_space = fr.owner_space = 'WORLD'
    # choose the knee pole angles with the hips sunk: muting nothing, just pushing the hips down by hand
    for cst in (s1, s2):
        cst.mute = True
    rb.location = (0.0, -0.30 / unit, 0.0)
    bpy.context.view_layer.update()
    report['knee_pole_deg'] = [_pick_pole(arm, arm.pose.bones[s + '_Knee_N1'].constraints['planted'], s + '_Knee_N1', D['PLAYER.Knee.' + s]) for s in ('L', 'R')]
    rb.location = (0, 0, 0)
    for cst in (s1, s2):
        cst.mute = False

    # ---- arms: wrists to the hand targets, elbows down and back
    palm = {'L': Matrix(((0, -1, 0), (-1, 0, 0), (0, 0, 1))), 'R': Matrix(((0, 1, 0), (1, 0, 0), (0, 0, 1)))}
    # (columns X, Y, Z in the stand's frame for a T-pose palm: fingers out along the arm, palm down)
    palm = {'L': Matrix(((0, -1, 0), (1, 0, 0), (0, 0, 1))).transposed(), 'R': Matrix(((0, 1, 0), (-1, 0, 0), (0, 0, 1))).transposed()}
    for side, sx in (('L', -1.0), ('R', 1.0)):
        hand = D['HAND.' + side]
        fingers = (head(side + '_Hand_N1') - head(side + '_Wrist_N1')).normalized()
        y = fingers
        z = Vector((0.0, 0.0, 1.0))                                                    # palm down in the T pose: -Z out of the palm
        x = y.cross(z).normalized()
        frame = Matrix((x, y, z)).transposed()                                        # the hand-target convention in the stand's frame
        off = _empty('PLAYER.%s.wrist_turn' % side, hand)
        off.rotation_mode = 'QUATERNION'
        off.rotation_quaternion = (frame.inverted() @ orient(side + '_Wrist_N1')).to_quaternion()
        shoulder = head(side + '_Arm_N1')
        pole = _empty('PLAYER.Elbow.' + side, stand, loc=(shoulder.x * 2.2, shoulder.y - 0.55, shoulder.z - 0.45), size=0.04, kind='SPHERE')
        rig_lib_parent = D['PLAYER.Shoulders']
        pole.parent = rig_lib_parent                                                   # the poles ride with the shoulders
        pole.matrix_parent_inverse = Matrix.Identity(4)
        pole.location = (shoulder.x * 2.2 - sh_rest.x, -0.55, -0.45)
        fore = arm.pose.bones[side + '_Elbow_N1']
        ik = fore.constraints.new('IK')
        ik.name = 'reach'
        ik.target = hand
        ik.chain_count = 2
        ik.use_tail = True
        ik.use_stretch = False
        ik.pole_target = pole
        wb = arm.pose.bones[side + '_Wrist_N1']
        cr = wb.constraints.new('COPY_ROTATION')
        cr.name = 'palm'
        cr.target = off
        cr.target_space = cr.owner_space = 'WORLD'
        cr.influence = 0.7
    # choose the elbow pole angles with the hands in front of the chest
    keep = {s: D['HAND.' + s].matrix_world.copy() for s in ('L', 'R')}
    for side, sx in (('L', -1.0), ('R', 1.0)):
        D['HAND.' + side].matrix_world = stand.matrix_world @ Matrix.Translation((sx * 0.18, 0.32, sh_rest.z - 0.2))
    bpy.context.view_layer.update()
    report['elbow_pole_deg'] = [_pick_pole(arm, arm.pose.bones[s + '_Elbow_N1'].constraints['reach'], s + '_Elbow_N1', D['PLAYER.Elbow.' + s]) for s in ('L', 'R')]
    for side in ('L', 'R'):
        D['HAND.' + side].matrix_world = keep[side]
    bpy.context.view_layer.update()

    for n in ('PLAYER.Body.preview', 'HAND.L.preview', 'HAND.R.preview'):
        if n in D:
            D[n].hide_viewport = True
    for m in bpy.data.materials:
        if m.name.startswith('Panzertruppe'):
            m.diffuse_color = (0.035, 0.035, 0.04, 1.0)     # the black Panzer uniform, for the solid view
    report.update({'shoulders_m': round(sh_rest.z, 3), 'hips_m': round(hips_rest.z, 3), 'upper_arm_m': round((head('L_Elbow_N1') - head('L_Arm_N1')).length, 3),
                   'forearm_m': round((head('L_Wrist_N1') - head('L_Elbow_N1')).length, 3), 'thigh_m': round((head('L_Knee_N1') - head('L_Leg_N1')).length, 3),
                   'shin_m': round((head('L_Ankle_N1') - head('L_Knee_N1')).length, 3)})
    return report
