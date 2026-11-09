"""The Tiger's repairs and hand jobs as animated step sequences (see repair_anim.py for how they
are stored and exported). One function per sequence; `build_all()` rebuilds every one in the open
rig file. Positions are world metres in the kit pose; the player faces forward with yaw 180.

Each sequence ends with the tank back in its kit pose unless its title says it leaves something
open. Counts marked "not verified" are placeholders for numbers no source was found for.
"""
import math

import bpy
from mathutils import Matrix, Vector

import repair_anim as ra

X, Y, Z = Vector((1, 0, 0)), Vector((0, 1, 0)), Vector((0, 0, 1))
POCKET = Matrix.Translation((-0.2, 0.05, 0.95))     # where small things go, in the player's frame
FWD = 180.0


def T(x, y, z):
    return Matrix.Translation((x, y, z))


def fasteners(set_name):
    return sorted(o.name for o in bpy.data.collections['FASTENERS'].objects if o.get('fastener_set') == set_name)


def tool_for(fastener):
    """The tool a fastener wants: a spanner sized to a hex or square head, a hand for a wing nut,
    a screwdriver stand-in for a round head."""
    o = bpy.data.objects[fastener]
    if o.get('fastener_tool') == 'hand' or o['fastener_kind'] == 'wingnut':
        return None
    if o.get('fastener_shape') in ('hex', 'square'):
        return ra.ensure_spanner(o['fastener_size_m']).name
    return ra.ensure_spanner(max(o['fastener_size_m'] * 0.6, 0.008)).name  # stands in for a screwdriver; named in the step


def undo(r, fastener, sid, keep='pocket', stroke_deg=60.0, seconds=0.8):
    """Two steps: one stroke of the tool (a cycle the player repeats), then the fastener out and
    put away. Returns the tool's name (None for a hand job)."""
    o = bpy.data.objects[fastener]
    turns, lift = ra.turns_for(fastener)
    if o['fastener_kind'] == 'wingnut':
        turns, lift = 5.0, 0.012          # not verified: no thread on the kit part
    tool = tool_for(fastener)
    strokes = max(1, round(turns * 360.0 / stroke_deg))
    h = float(o.get('fastener_height_m', 0.01))
    with r.step(sid + '_turn', kind='cycle', seconds=seconds, tool=tool or 'hand', repeats=strokes, sets=o.get('fastener_set', ''),
                player='turn the %s anticlockwise, one stroke' % o['fastener_kind'], input='stroke'):
        if tool:
            r.follow(tool, fastener, offset=T(0, 0, h * 0.5), a=0.0, b=0.85)
            r.hand('R', tool, a=0.0, b=0.85)
        else:
            r.hand('R', fastener, grip=T(0, 0, h) @ Matrix.Rotation(math.pi, 4, 'X'), a=0.0, b=0.85)
        r.unscrew(fastener, stroke_deg / 360.0, lift * stroke_deg / 360.0 / max(turns, 1e-6), a=0.1, b=0.8)
        r.event('ratchet', at=0.8)
    with r.step(sid + '_out', kind='oneshot', seconds=0.9, tool=tool or 'hand', sets=o.get('fastener_set', ''), player='spin it out and pocket it'):
        r.unscrew(fastener, turns - stroke_deg / 360.0, lift * (1.0 - stroke_deg / 360.0 / max(turns, 1e-6)), a=0.0, b=0.35)
        if tool:
            r.carry(tool, a=0.1, b=1.0)
        r.hand('L', fastener, grip=T(0, 0, h) @ Matrix.Rotation(math.pi, 4, 'X'), a=0.3, b=1.0)
        if keep == 'pocket':
            r.follow(fastener, 'PLAYER.Stand', offset=POCKET, a=0.75, b=1.0, reach=0.3, hold=True)
        r.event('fastener_out', at=0.35)
    return tool


def do_up(r, fastener, sid, stroke_deg=60.0, seconds=0.8):
    """The reverse: start the fastener in its hole by hand, then one stroke of the tool (a cycle)."""
    o = bpy.data.objects[fastener]
    turns, lift = ra.turns_for(fastener)
    if o['fastener_kind'] == 'wingnut':
        turns, lift = 5.0, 0.012
    tool = tool_for(fastener)
    strokes = max(1, round(turns * 360.0 / stroke_deg))
    h = float(o.get('fastener_height_m', 0.01))
    seat = o.matrix_world.copy()
    with r.step(sid + '_start', kind='oneshot', seconds=1.0, tool='hand', sets=o.get('fastener_set', ''), player='start it in its thread by hand'):
        r.hand('L', fastener, grip=T(0, 0, h) @ Matrix.Rotation(math.pi, 4, 'X'), a=0.0, b=0.9)
        up = seat.copy()
        up.translation = seat.translation + (seat.to_3x3() @ Z).normalized() * lift
        r.place(fastener, ra._unscaled(up), a=0.1, b=0.6, arc=0.05)
        r.unscrew(fastener, -stroke_deg / 360.0, -lift * stroke_deg / 360.0 / max(turns, 1e-6), a=0.62, b=0.95)
        if tool:
            r.carry(tool, a=0.0, b=1.0)
    with r.step(sid + '_tighten', kind='cycle', seconds=seconds, tool=tool or 'hand', repeats=strokes, sets=o.get('fastener_set', ''),
                player='turn it clockwise, one stroke', input='stroke'):
        if tool:
            r.follow(tool, fastener, offset=T(0, 0, h * 0.5), a=0.0, b=0.85)
            r.hand('R', tool, a=0.0, b=0.85)
        else:
            r.hand('R', fastener, grip=T(0, 0, h) @ Matrix.Rotation(math.pi, 4, 'X'), a=0.0, b=0.85)
        r.unscrew(fastener, -stroke_deg / 360.0, -lift * stroke_deg / 360.0 / max(turns, 1e-6), a=0.1, b=0.8)
        r.event('ratchet', at=0.8)
    with r.step(sid + '_tight', kind='oneshot', seconds=0.4, tool=tool or 'hand', sets=o.get('fastener_set', ''), player='nip it up'):
        # the strokes the cycle stands for: the fastener ends on its seat
        left = turns - 2.0 * stroke_deg / 360.0
        r.unscrew(fastener, -left, -lift * left / max(turns, 1e-6), a=0.0, b=0.2)
        if tool:
            r.follow(tool, fastener, offset=T(0, 0, h * 0.5), a=0.0, b=0.6, reach=0.0)
            r.hand('R', tool, a=0.0, b=0.6, reach=0.0)
            r.carry(tool, a=0.75, b=1.0)
        r.event('fastener_tight', at=0.6)
    return tool


def two_hands(r, ctrl, left, right, a=0.0, b=1.0, palm_l=(-1, 0, 0), palm_r=(1, 0, 0), fingers=(0, -1, 0), reach=0.3):
    """Both hands on a part: `left` and `right` are points in the control's own space."""
    r.hand('L', ctrl, grip=ra.hand_pose(left, palm_l, fingers), a=a, b=b, reach=reach)
    r.hand('R', ctrl, grip=ra.hand_pose(right, palm_r, fingers), a=a, b=b, reach=reach)


# ------------------------------------------------------------------------------- rear plate
def starter_hand_crank():
    """E9. Hand start: cover off the starter port, the shaft out of its clips and into the port,
    wind the inertia starter, trip it, put everything back. The cover and shaft readings of the
    kit are not verified; the crank handle is a stand-in (the kit has none); the number of turns
    and the trip are not verified."""
    D = bpy.data.objects
    r = ra.Repair('starter_hand_crank', row='E9', title='Hand start at the rear plate',
                  note='cover 28A-B69 and shaft 28A-Z16 read from the kit, not verified; crank handle is a stand-in; turns and trip not verified')
    bolt = fasteners('starter_port_cover')[0]
    sock = D['CTRL.Starter_Crank_Turn']
    seat = sock.matrix_world.translation.copy()
    n = -(sock.matrix_world.to_3x3() @ X).normalized()          # out of the hull
    handle = ra.ensure_crank_handle().name
    r.stand((-0.25, 4.55, 0.0), FWD)
    r.start_at(handle, ra.pose((-0.6, 4.35, 0.02)))
    r.bring(tool_for(bolt))
    tool = undo(r, bolt, 'cover_bolt')
    flat = T(0.45, 4.2, 0.04) @ n.rotation_difference(Z).to_matrix().to_4x4()
    with r.step('cover_off', seconds=1.6, player='lift the cover off the port and set it down'):
        r.place(tool, ra.pose((-0.75, 4.35, 0.02)), a=0.0, b=0.22, arc=0.2)
        two_hands(r, 'CTRL.Starter_Port_Cover', (0.13, 0.03, 0.0), (-0.13, 0.03, 0.0), a=0.42, b=0.95, reach=0.25)
        r.place('CTRL.Starter_Port_Cover', flat, a=0.45, b=0.95, arc=0.25)
        r.event('cover_off', at=0.3)
    with r.step('shaft_out_of_clips', seconds=1.4, player='lift the starting shaft out of its clips'):
        two_hands(r, 'CTRL.Starter_Crank', (0.16, 0.0, -0.02), (-0.16, 0.0, -0.02), a=0.1, b=1.0, palm_l=(0, 0, 1), palm_r=(0, 0, 1))
        out = D['CTRL.Starter_Crank'].matrix_world.copy()
        out.translation = out.translation + n * 0.16 + Z * 0.05
        r.place('CTRL.Starter_Crank', out, a=0.4, b=1.0)
    with r.step('shaft_into_port', seconds=1.8, player='turn the shaft square end first and seat it in the port', requires='Starter_Port_Cover off'):
        two_hands(r, 'CTRL.Starter_Crank', (0.16, 0.0, -0.02), (-0.16, 0.0, -0.02), a=0.0, b=1.0, palm_l=(0, 0, 1), palm_r=(0, 0, 1))
        half = 0.3585                                           # shaft middle to its square end
        near = ra.pose_x(seat + n * (half + 0.12), -n)
        r.place('CTRL.Starter_Crank', near, a=0.0, b=0.7, arc=0.1)
        r.place('CTRL.Starter_Crank', ra.pose_x(seat + n * half, -n), a=0.75, b=1.0)
        r.event('shaft_seated', at=1.0)
    with r.step('fit_handle', seconds=1.2, tool=handle, player='slip the crank handle on the shaft end'):
        r.hand('R', handle, a=0.3, b=1.0)
        r.follow(handle, 'CTRL.Starter_Crank', offset=T(-half, 0, 0) @ Matrix.Rotation(math.radians(-90), 4, 'Y'), a=0.75, b=1.0, reach=0.4)
    level = ra.hand_pose((0, 0, 0), palm_to=(0, 0, -1), fingers=(1, 0, 0))
    with r.step('wind', kind='cycle', seconds=1.0, tool=handle, repeats=60, input='turn', requires='shaft seated',
                player='wind the inertia starter, one turn (count not verified)'):
        r.turn('CTRL.Starter_Crank_Turn', 360.0)
        r.follow('CTRL.Starter_Crank', 'CTRL.Starter_Crank_Turn', a=0.0, b=1.0, reach=0.0)
        r.follow(handle, 'CTRL.Starter_Crank', offset=T(-half, 0, 0) @ Matrix.Rotation(math.radians(-90), 4, 'Y'), a=0.0, b=1.0, reach=0.0)
        r.hand('R', handle, a=0.0, b=1.0, orient=level)
        r.event('starter_whine', at=0.5)
    with r.step('trip', seconds=1.0, player='trip the starter onto the engine (control not in the kit; not verified)'):
        r.follow(handle, 'CTRL.Starter_Crank', offset=T(-half, 0, 0) @ Matrix.Rotation(math.radians(-90), 4, 'Y'), a=0.0, b=1.0, reach=0.0)
        r.hand('R', handle, a=0.0, b=1.0, orient=level)
        r.event('starter_engage', at=0.3)
        r.event('engine_fires', at=0.9)
    with r.step('shaft_back', seconds=2.4, player='draw the shaft, clip it back on the plate'):
        r.hand('R', handle, a=0.0, b=0.3)
        r.place(handle, ra.pose((-0.6, 4.35, 0.02)), a=0.05, b=0.3, arc=0.15)
        two_hands(r, 'CTRL.Starter_Crank', (0.16, 0.0, -0.02), (-0.16, 0.0, -0.02), a=0.35, b=1.0, palm_l=(0, 0, 1), palm_r=(0, 0, 1))
        home = D['FRAME.Starter_Crank'].matrix_world.copy()
        r.place('CTRL.Starter_Crank', ra.pose_x(seat + n * (half + 0.15), -n), a=0.4, b=0.55)
        r.place('CTRL.Starter_Crank', home, a=0.55, b=1.0, arc=0.1)
    with r.step('cover_on', seconds=1.6, player='put the cover back on the port'):
        two_hands(r, 'CTRL.Starter_Port_Cover', (0.13, 0.03, 0.0), (-0.13, 0.03, 0.0), a=0.1, b=1.0)
        r.place('CTRL.Starter_Port_Cover', D['FRAME.Starter_Port_Cover'].matrix_world.copy(), a=0.3, b=1.0, arc=0.25)
    with r.step('pick_up_spanner', seconds=0.9, tool=tool, player='pick the spanner up'):
        r.hand('R', tool, a=0.3, b=1.0)
        r.follow(tool, 'PLAYER.Stand', offset=T(0.28, 0.3, 1.0) @ Matrix.Rotation(math.radians(-90), 4, 'Y'), a=0.6, b=1.0, reach=0.3)
    do_up(r, bolt, 'cover_bolt_in')
    return r.finish()


def jack_off_the_plate():
    """F3. Free the crank strap and the two holder clamps, lift the jack off the rear plate and
    set it on the ground, wind it one turn, put it back. The clamps do not open in the rig yet:
    their bolts come out and the jack lifts through them. Rack travel per turn is not rigged."""
    D = bpy.data.objects
    r = ra.Repair('jack_off_the_plate', row='F3', title='Jack off the rear plate and back',
                  note='holder clamps not split: bolts out, the jack lifts through them; rack does not move with the crank yet')
    bolts = fasteners('jack_holder.000') + fasteners('jack_holder.001')
    tool = tool_for(bolts[0])
    r.stand((-1.05, 4.75, 0.0), FWD)
    r.bring(tool)
    with r.step('strap_open', seconds=1.2, player='lift the catch loop, swing the strap\'s front jaw off the grip'):
        r.hand('L', 'CTRL.Jack_Crank_Latch', grip=ra.hand_pose((-0.02, 0.0, -0.13), (0, 0, 1), (1, 0, 0)), a=0.0, b=1.0)
        r.joint('CTRL.Jack_Crank_Latch', 110.0, a=0.25, b=1.0)
        r.event('latch_open', at=0.3)
    for i, b in enumerate(bolts):
        undo(r, b, 'holder_bolt_%d' % (i + 1))
    ground = T(-1.1, 4.55, 0.16) @ Matrix.Rotation(math.radians(-82), 4, 'X')
    with r.step('lift_off', seconds=2.2, tool='', requires='Jack_Crank_Latch open, holder bolts out', player='two hands under the body, lift the jack off and lay it down'):
        r.place(tool, ra.pose((-1.75, 4.6, 0.02)), a=0.0, b=0.15, arc=0.2)
        two_hands(r, 'CTRL.Jack', (0.3, 0.05, -0.05), (-0.3, 0.05, -0.05), a=0.25, b=1.0, palm_l=(0, 0, 1), palm_r=(0, 0, 1))
        out = D['FRAME.Jack'].matrix_world.copy()
        out.translation = out.translation + Vector((0.0, 0.45, 0.05))
        r.place('CTRL.Jack', out, a=0.3, b=0.6)
        r.place('CTRL.Jack', ground, a=0.6, b=1.0, arc=0.1)
        r.event('jack_free', at=0.35)
    level = ra.hand_pose((0, 0, 0), palm_to=(0, 0, -1), fingers=(0, -1, 0))
    with r.step('wind', kind='cycle', seconds=1.2, repeats=40, input='turn', requires='jack off the plate', player='wind the jack, one turn of the crank (turns per lift not rigged)'):
        r.turn('CTRL.Jack_Crank', 360.0)
        grip = next(o for o in D if '28A-O34_ARM' in o.name)
        r.hand('R', grip.name, grip=T(0, 0, 0.09), a=0.0, b=1.0, orient=level)
        r.hand('L', 'CTRL.Jack', grip=ra.hand_pose((-0.25, 0.0, 0.1), (0, 0, -1), (0, -1, 0)), a=0.0, b=1.0)
        r.event('jack_ratchet', at=0.5)
    with r.step('lift_back', seconds=2.2, player='lift the jack back onto its holders'):
        two_hands(r, 'CTRL.Jack', (0.3, 0.05, -0.05), (-0.3, 0.05, -0.05), a=0.0, b=1.0, palm_l=(0, 0, 1), palm_r=(0, 0, 1))
        r.place('CTRL.Jack', out, a=0.1, b=0.6, arc=0.1)
        r.place('CTRL.Jack', D['FRAME.Jack'].matrix_world.copy(), a=0.6, b=1.0)
    with r.step('pick_up_spanner', seconds=0.9, tool=tool, player='pick the spanner up'):
        r.hand('R', tool, a=0.3, b=1.0)
        r.follow(tool, 'PLAYER.Stand', offset=T(0.28, 0.3, 1.0) @ Matrix.Rotation(math.radians(-90), 4, 'Y'), a=0.6, b=1.0, reach=0.3, hold=True)
    for i, b in enumerate(bolts):
        do_up(r, b, 'holder_bolt_%d_in' % (i + 1))
    with r.step('strap_shut', seconds=1.0, player='swing the strap shut over the grip'):
        r.hand('L', 'CTRL.Jack_Crank_Latch', grip=ra.hand_pose((-0.02, 0.0, -0.13), (0, 0, 1), (1, 0, 0)), a=0.0, b=1.0)
        r.joint('CTRL.Jack_Crank_Latch', 0.0, a=0.1, b=0.9)
    return r.finish()


# ------------------------------------------------------------------------------- engine deck
DECK_Z = 1.80


def engine_hatch_open():
    """B7. Draw the two side bolts, fold the bar up, swing the engine hatch up toward the turret
    and shut it again. With the turret at 12 o'clock the lid stops on the turret bin at 35
    degrees; the sequence goes that far."""
    D = bpy.data.objects
    r = ra.Repair('engine_hatch_open', row='B7', title='Engine hatch open and shut',
                  note='opens to 35 degrees, the limit with the turret at 12 o\'clock; 100 degrees with the turret traversed away')
    open_to = float(D['CTRL.Deck_Hatch'].get('max_turret_forward', 35.0))
    r.stand((0.0, 3.75, DECK_Z), FWD)
    with r.step('bolt_left', seconds=0.8, player='draw the left bolt inward'):
        r.hand('L', 'CTRL.Deck_Hatch_Lock_L', grip=ra.hand_pose((0.0, 0.0, 0.03), (0, 0, -1), (0, -1, 0)), a=0.0, b=1.0)
        r.joint('CTRL.Deck_Hatch_Lock_L', 0.04, a=0.3, b=0.9)
        r.event('bolt_drawn', at=0.9)
    with r.step('bolt_right', seconds=0.8, player='draw the right bolt inward'):
        r.hand('R', 'CTRL.Deck_Hatch_Lock_R', grip=ra.hand_pose((0.0, 0.0, 0.03), (0, 0, -1), (0, -1, 0)), a=0.0, b=1.0)
        r.joint('CTRL.Deck_Hatch_Lock_R', 0.04, a=0.3, b=0.9)
        r.event('bolt_drawn', at=0.9)
    bar = ra.hand_pose((0.0, -0.17, 0.02), (0, 0, 1), (1, 0, 0))
    with r.step('bar_up', seconds=0.8, player='fold the lifting bar up'):
        r.hand('R', 'CTRL.Deck_Hatch_Bar', grip=bar, a=0.0, b=1.0)
        r.joint('CTRL.Deck_Hatch_Bar', 40.0, a=0.3, b=1.0)
    with r.step('lift', seconds=2.0, requires='both bolts drawn', player='lift the rear edge and swing the lid up'):
        r.hand('R', 'CTRL.Deck_Hatch_Bar', grip=bar, a=0.0, b=1.0)
        r.hand('L', 'CTRL.Deck_Hatch', grip=ra.hand_pose((0.25, 1.1, 0.03), (0, 0, 1), (0, -1, 0)), a=0.1, b=1.0)
        r.joint('CTRL.Deck_Hatch', open_to, a=0.15, b=1.0)
        r.event('hatch_open', at=1.0)
    with r.step('hold_open', kind='hold', seconds=1.0, player='the lid is up: the engine bay is open to work in'):
        r.hand('L', 'CTRL.Deck_Hatch', grip=ra.hand_pose((0.25, 1.1, 0.03), (0, 0, 1), (0, -1, 0)), a=0.0, b=1.0)
    with r.step('lower', seconds=1.8, player='lower the lid'):
        r.hand('R', 'CTRL.Deck_Hatch_Bar', grip=bar, a=0.0, b=1.0)
        r.hand('L', 'CTRL.Deck_Hatch', grip=ra.hand_pose((0.25, 1.1, 0.03), (0, 0, 1), (0, -1, 0)), a=0.0, b=0.9)
        r.joint('CTRL.Deck_Hatch', 0.0, a=0.0, b=0.9)
        r.event('hatch_shut', at=0.9)
    with r.step('bar_down', seconds=0.6, player='fold the bar down'):
        r.hand('R', 'CTRL.Deck_Hatch_Bar', grip=bar, a=0.0, b=1.0)
        r.joint('CTRL.Deck_Hatch_Bar', 0.0, a=0.0, b=0.8)
    with r.step('bolts_home', seconds=1.0, player='push both bolts home'):
        r.hand('L', 'CTRL.Deck_Hatch_Lock_L', grip=ra.hand_pose((0.0, 0.0, 0.03), (0, 0, -1), (0, -1, 0)), a=0.0, b=1.0)
        r.hand('R', 'CTRL.Deck_Hatch_Lock_R', grip=ra.hand_pose((0.0, 0.0, 0.03), (0, 0, -1), (0, -1, 0)), a=0.0, b=1.0)
        r.joint('CTRL.Deck_Hatch_Lock_L', 0.0, a=0.3, b=0.9)
        r.joint('CTRL.Deck_Hatch_Lock_R', 0.0, a=0.3, b=0.9)
    return r.finish()


def _fan_grille(side):
    """B7. Undo the two wing nuts, fold the bar up, lift the lattice grille off the fan bay and
    lay it on the deck, lift the plate under it out; then everything back."""
    D = bpy.data.objects
    name = 'Fan_Grille_' + side
    cx = D['CTRL.' + name].matrix_world.translation.x
    r = ra.Repair('fan_grille_off_' + side, row='B7', title='Fan grille %s off and back' % side,
                  note='which edge the wing nuts clamp is a reading of the kit; the folding bar is used as the lifting handle')
    nuts = fasteners('fan_grille_' + side)
    r.stand((cx * 0.45, 3.3, DECK_Z), -90.0 if cx > 0 else 90.0)      # on the engine hatch, facing the grille
    for i, nut in enumerate(nuts):
        undo(r, nut, 'wing_nut_%d' % (i + 1), keep='pocket', stroke_deg=180.0, seconds=0.7)
    bar = 'CTRL.Fan_Grille_Bar_' + side
    grip = ra.hand_pose((0.0, -0.17, 0.02), (0, 0, 1), (1, 0, 0))
    with r.step('bar_up', seconds=0.8, player='fold the bar up'):
        r.hand('R', bar, grip=grip, a=0.0, b=1.0)
        r.joint(bar, 60.0, a=0.3, b=1.0)
    aside = T(cx * 0.35, 2.75, DECK_Z + 0.12)
    with r.step('grille_off', seconds=2.2, requires='both wing nuts off', player='lift the lattice grille and lay it on the engine hatch'):
        r.hand('R', bar, grip=grip, a=0.0, b=1.0)
        r.hand('L', 'CTRL.' + name, grip=ra.hand_pose((0.0, 0.2, 0.05), (0, 0, 1), (0, -1, 0)), a=0.1, b=1.0)
        up = D['FRAME.' + name].matrix_world.copy()
        up.translation = up.translation + Z * 0.25
        r.place('CTRL.' + name, up, a=0.2, b=0.5)
        r.place('CTRL.' + name, aside, a=0.5, b=1.0, arc=0.1)
        r.event('grille_off', at=0.3)
    cover = 'Fan_Cover_' + side
    aside2 = T(cx * 0.35, 2.75, DECK_Z + 0.22)
    with r.step('plate_out', seconds=2.0, requires='grille off', player='lift the plate out from over the fan'):
        two_hands(r, 'CTRL.' + cover, (0.35, 0.0, 0.02), (-0.35, 0.0, 0.02), a=0.1, b=1.0, palm_l=(0, 0, 1), palm_r=(0, 0, 1))
        up2 = D['FRAME.' + cover].matrix_world.copy()
        up2.translation = up2.translation + Z * 0.3
        r.place('CTRL.' + cover, up2, a=0.2, b=0.5)
        r.place('CTRL.' + cover, aside2, a=0.5, b=1.0, arc=0.1)
    with r.step('fan_bay_open', kind='hold', seconds=1.0, player='the fan bay is open'):
        pass
    with r.step('plate_in', seconds=2.0, player='lower the plate back over the fan'):
        two_hands(r, 'CTRL.' + cover, (0.35, 0.0, 0.02), (-0.35, 0.0, 0.02), a=0.0, b=1.0, palm_l=(0, 0, 1), palm_r=(0, 0, 1))
        r.place('CTRL.' + cover, up2, a=0.1, b=0.6, arc=0.1)
        r.place('CTRL.' + cover, D['FRAME.' + cover].matrix_world.copy(), a=0.6, b=1.0)
    with r.step('grille_on', seconds=2.2, player='lift the grille back on'):
        r.hand('R', bar, grip=grip, a=0.0, b=1.0)
        r.hand('L', 'CTRL.' + name, grip=ra.hand_pose((0.0, 0.2, 0.05), (0, 0, 1), (0, -1, 0)), a=0.0, b=1.0)
        r.place('CTRL.' + name, up, a=0.1, b=0.6, arc=0.1)
        r.place('CTRL.' + name, D['FRAME.' + name].matrix_world.copy(), a=0.6, b=1.0)
    with r.step('bar_down', seconds=0.6, player='fold the bar down'):
        r.hand('R', bar, grip=grip, a=0.0, b=1.0)
        r.joint(bar, 0.0, a=0.0, b=0.8)
    for i, nut in enumerate(nuts):
        do_up(r, nut, 'wing_nut_%d_on' % (i + 1), stroke_deg=180.0, seconds=0.7)
    return r.finish()


def fan_grille_off_L():
    return _fan_grille('L')


def fan_grille_off_R():
    return _fan_grille('R')


def _deck_cap(tag):
    """B7. Unscrew a filler cap by hand, lift it off, set it down, and screw it back. Turns are
    not verified; which filler each cap covers is not identified."""
    D = bpy.data.objects
    ctrl = 'CTRL.Deck_Cap_' + tag
    c = D[ctrl].matrix_world.translation
    r = ra.Repair('deck_cap_' + tag, row='B7', title='Filler cap %s off and on' % tag, note='turns not verified; filler not identified')
    r.stand((c.x * 0.5, c.y + 0.1, DECK_Z), -90.0 if c.x > 0 else 90.0)
    palm = ra.hand_pose((0.0, 0.0, 0.03), (0, 0, -1), (0, -1, 0))
    with r.step('unscrew', kind='cycle', seconds=0.8, repeats=6, input='stroke', player='half a turn anticlockwise (count not verified)'):
        r.hand('R', ctrl, grip=palm, a=0.0, b=0.85)
        r.spin_lift(ctrl, 0.5, 0.002, a=0.1, b=0.8)
    aside = T(c.x * 0.8, c.y - 0.3, DECK_Z + 0.05)
    with r.step('cap_off', seconds=1.4, player='lift the cap off and set it down'):
        r.spin_lift(ctrl, 2.5, 0.010, a=0.0, b=0.3)
        r.hand('R', ctrl, grip=palm, a=0.0, b=1.0)
        r.place(ctrl, aside, a=0.35, b=1.0, arc=0.15)
        r.event('filler_open', at=0.35)
    with r.step('filler_open', kind='hold', seconds=1.0, player='the filler is open'):
        pass
    with r.step('cap_on', seconds=1.4, player='set the cap back on its thread'):
        r.hand('R', ctrl, grip=palm, a=0.0, b=1.0)
        up = D['FRAME.Deck_Cap_' + tag].matrix_world.copy()
        up.translation = up.translation + Z * 0.012
        r.place(ctrl, up, a=0.1, b=0.9, arc=0.15)
    with r.step('screw_down', kind='cycle', seconds=0.8, repeats=6, input='stroke', player='half a turn clockwise'):
        r.hand('R', ctrl, grip=palm, a=0.0, b=0.85)
        r.place(ctrl, D['FRAME.Deck_Cap_' + tag].matrix_world.copy(), a=0.1, b=0.8)
    return r.finish()


def deck_cap_FL():
    return _deck_cap('FL')


ALL = [starter_hand_crank, jack_off_the_plate, engine_hatch_open, fan_grille_off_L, fan_grille_off_R, deck_cap_FL]


def build_all(wipe=True, first=0, count=None):
    """Rebuild the sequences in order (`first`, `count` to do them in batches: a full rebuild takes
    minutes). Wipe only with the first batch."""
    if wipe and first == 0:
        ra.wipe()
    out = []
    for f in ALL[first:(first + count) if count else None]:
        bpy.context.scene.frame_set(1)
        out.append(f())
    return out
