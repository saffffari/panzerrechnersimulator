"""The Tiger's repairs and hand jobs as animated step sequences (see repair_anim.py for how they
are stored and exported). One function per sequence; `build_all()` rebuilds every one in the open
rig file. Positions are world metres in the kit pose; the player faces forward with yaw 180.

Each sequence ends with the tank back in its kit pose unless its title says it leaves something
open. Counts marked "not verified" are placeholders for numbers no source was found for.
"""
import math

import bpy
import numpy as np
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


# ------------------------------------------------------------------------------- running gear
def hammer_pose(hammer, target, strike, handle):
    """World pose of the kit hammer with its face on `target`, striking along `strike`, the handle
    running off toward `handle`."""
    o = bpy.data.objects[hammer]
    y = Vector(strike).normalized()
    g = Vector(handle)
    g = (g - y * g.dot(y)).normalized()
    z = -g
    x = y.cross(z)
    m = Matrix((x, y, z)).transposed().to_4x4()
    m.translation = Vector(target) - m.to_3x3() @ Vector(o['tip_local'])
    return m


def swing_back(hammer, strike_pose, strike, degrees=50.0):
    """The same hammer swung back about the hand."""
    o = bpy.data.objects[hammer]
    grip = strike_pose @ Vector(o['grip_local'])
    tip = strike_pose @ Vector(o['tip_local'])
    axis = (tip - grip).cross(Vector(strike)).normalized()
    for sign in (1.0, -1.0):
        rot = Matrix.Translation(grip) @ Matrix.Rotation(math.radians(sign * degrees), 4, axis) @ Matrix.Translation(-grip)
        back = rot @ strike_pose
        if ((back @ Vector(o['tip_local'])) - tip).dot(Vector(strike)) < 0.0:
            return back
    return back


def _blows(r, sid, hammer, drift, pin_ctrl, outward, per_blow, blows, handle, inward=False):
    """Three steps on one pin: drift on, one blow (a cycle), and the rest of the travel."""
    D = bpy.data.objects
    e = D[pin_ctrl]
    lo, hi = float(e['min']), float(e['max'])
    half = (hi - 0.03) * 0.5
    drift_off = T(-half, 0, 0) @ Matrix.Rotation(math.radians(-90), 4, 'Y')       # tip on the pin's inner end, shank inboard
    if inward:
        drift_off = T(half, 0, 0) @ Matrix.Rotation(math.radians(90), 4, 'Y')     # on the outer end, shank outboard
    with r.step(sid + '_drift_on', seconds=0.8, tool=drift, player='set the drift on the pin end'):
        r.follow(drift, pin_ctrl, offset=drift_off, a=0.5, b=1.0, reach=0.35, hold=True)
        r.hand('L', drift, a=0.0, b=1.0, hold=True)
    D_dir = Vector(outward) * (-1.0 if inward else 1.0)
    bpy.context.view_layer.update()
    end = e.matrix_world.translation + Vector(outward) * ((half if inward else -half)) - D_dir * 0.25
    if inward:
        end = end + Vector(outward) * (hi - 0.0)    # the pin stands drawn when it is driven home
    strike = hammer_pose(hammer, end, D_dir, handle)
    back = swing_back(hammer, strike, D_dir)
    step = -per_blow if inward else per_blow
    start = hi if inward else lo
    with r.step(sid + '_blow', kind='cycle', seconds=0.7, tool=hammer, repeats=blows, input='strike', player='one blow of the hammer on the drift'):
        r.place(hammer, back, a=0.0, b=0.4)
        r.place(hammer, strike, a=0.42, b=0.58)
        r.place(hammer, back, a=0.62, b=1.0)
        r.hand('R', hammer, a=0.0, b=1.0, hold=True)
        r.joint(pin_ctrl, start + step, a=0.58, b=0.66, start=start)
        r.event('hammer_strike', at=0.58)
    with r.step(sid + ('_home' if inward else '_drawn'), seconds=1.0, player='the pin is home' if inward else 'the pin is out of both links'):
        r.joint(pin_ctrl, lo if inward else hi, a=0.0, b=0.6)
        r.event('pin_home' if inward else 'pin_out', at=0.6)


def _track_link_replace(side):
    """A1. Break the track at the rearmost link on the idler: drive both pins out from between the
    tracks, lift the broken link out, seat a spare, drive the pins home from outside. The spare
    lies ready on the ground (sequence spare_link_off fetches it); the drawn pins stay on their
    own line beside the track; blows per pin are not verified."""
    D = bpy.data.objects
    sign = 1.0 if side == 'L' else -1.0
    out = Vector((sign, 0, 0))
    tools = ra.kit_tools()
    hammer, drift = tools['hammer'], ra.ensure_drift().name
    r = ra.Repair('track_link_replace_' + side, row='A1', title='Track link replaced at the idler, %s side' % side,
                  note='spare link starts on the ground; drawn pins stay on their line; blow counts not verified; which way the real pins come out is not verified')
    link_ctrl, spare = 'CTRL.Track_Link_' + side, 'CTRL.Spare_Link_1'
    link_obj = next(c for c in D[link_ctrl].children if 'TRACK' in c.name)
    spare_obj = D[spare].children[0]
    seat = ra._unscaled(link_obj.matrix_world) @ ra._unscaled(spare_obj.matrix_world).inverted() @ ra._unscaled(D[spare].matrix_world)
    spare_ground = T(sign * 0.45, 4.7, 0.07) @ Matrix.Rotation(math.radians(90), 4, 'X')
    r.stand((sign * 0.55, 4.25, 0.0), -90.0 * sign)
    r.start_at(spare, spare_ground)
    r.bring(hammer, 'R')
    r.bring(drift, 'L')
    for tag in ('a', 'b'):
        _blows(r, 'pin_' + tag, hammer, drift, 'CTRL.Track_Pin_%s_%s' % (side, tag), out, 0.06, 13, handle=(-sign * 0.3, 1.0, 0.4))
    broken_ground = T(sign * 0.9, 4.75, 0.07) @ Matrix.Rotation(math.radians(90), 4, 'X')
    with r.step('link_out', seconds=2.0, requires='both pins drawn', player='put the tools down, lift the broken link out of the run'):
        r.place(hammer, ra.pose((sign * 0.2, 4.4, 0.03)), a=0.0, b=0.2, arc=0.2)
        r.place(drift, ra.pose((sign * 0.3, 4.5, 0.02), z_axis=(1, 0, 0)), a=0.0, b=0.2, arc=0.2)
        two_hands(r, link_ctrl, (0.25, 0.0, 0.02), (-0.25, 0.0, 0.02), a=0.35, b=1.0, palm_l=(0, 0, 1), palm_r=(0, 0, 1))
        up = D['FRAME.Track_Link_' + side].matrix_world.copy()
        up.translation = up.translation + Vector((0, 0.25, 0.05))
        r.place(link_ctrl, up, a=0.4, b=0.6)
        r.place(link_ctrl, broken_ground, a=0.6, b=1.0, arc=0.15)
        r.event('track_parted', at=0.45)
    with r.step('spare_in', seconds=2.2, player='lift the spare link into the gap'):
        two_hands(r, spare, (0.25, 0.0, 0.02), (-0.25, 0.0, 0.02), a=0.1, b=1.0, palm_l=(0, 0, 1), palm_r=(0, 0, 1))
        near = seat.copy()
        near.translation = seat.translation + Vector((0, 0.25, 0.05))
        r.place(spare, near, a=0.2, b=0.7, arc=0.2)
        r.place(spare, seat, a=0.7, b=1.0)
        r.event('link_seated', at=1.0)
    with r.step('round_to_the_outside', seconds=2.0, player='pick the tools up and step round to the outside of the track'):
        r.stand((sign * 2.3, 4.0, 0.0), 90.0 * sign, a=0.3, b=1.0)
        r.hand('R', hammer, a=0.05, b=0.3)
        r.follow(hammer, 'PLAYER.Stand', offset=T(0.28, 0.3, 1.0) @ Matrix.Rotation(math.radians(-90), 4, 'Y'), a=0.25, b=1.0, reach=0.2, hold=True)
        r.hand('L', drift, a=0.05, b=0.3)
        r.follow(drift, 'PLAYER.Stand', offset=T(-0.28, 0.3, 1.0) @ Matrix.Rotation(math.radians(-90), 4, 'Y'), a=0.25, b=1.0, reach=0.2, hold=True)
    for tag in ('b', 'a'):
        _blows(r, 'pin_%s_in' % tag, hammer, drift, 'CTRL.Track_Pin_%s_%s' % (side, tag), out, 0.06, 13, handle=(sign * 0.3, 1.0, 0.4), inward=True)
    return r.finish()


def track_link_replace_L():
    return _track_link_replace('L')


def track_link_replace_R():
    return _track_link_replace('R')


def spare_link_off():
    """A2. Lift a spare link off its brackets on the turret side, set it on the deck, put it back."""
    D = bpy.data.objects
    ctrl = 'CTRL.Spare_Link_1'
    c = D[ctrl].matrix_world.translation.copy()
    r = ra.Repair('spare_link_off', row='A2', title='Spare link off the turret side and back', note='the brackets do not open in the kit: the link lifts straight off them')
    r.stand((c.x - 0.75 if c.x > 0 else c.x + 0.75, c.y + 0.2, DECK_Z), -90.0 if c.x > 0 else 90.0)
    out = Vector((1.0 if c.x > 0 else -1.0, 0, 0))
    with r.step('lift_off', seconds=1.8, player='lift the link up and off its brackets'):
        two_hands(r, ctrl, (0.0, 0.2, 0.0), (0.0, -0.2, 0.0), a=0.0, b=1.0, palm_l=(0, 0, 1), palm_r=(0, 0, 1))
        up = D['FRAME.Spare_Link_1'].matrix_world.copy()
        up.translation = up.translation + Z * 0.08 + out * 0.12
        r.place(ctrl, up, a=0.3, b=1.0)
        r.event('link_free', at=0.5)
    down = T(c.x, c.y + 0.9, DECK_Z + 0.07) @ Matrix.Rotation(math.radians(90), 4, 'X')
    with r.step('set_down', seconds=1.6, player='set it down on the deck'):
        two_hands(r, ctrl, (0.0, 0.2, 0.0), (0.0, -0.2, 0.0), a=0.0, b=1.0, palm_l=(0, 0, 1), palm_r=(0, 0, 1))
        r.place(ctrl, down, a=0.0, b=1.0, arc=0.1)
    with r.step('in_hand', kind='hold', seconds=0.8, player='a spare link is ready'):
        pass
    with r.step('back_on', seconds=2.2, player='hang it back on its brackets'):
        two_hands(r, ctrl, (0.0, 0.2, 0.0), (0.0, -0.2, 0.0), a=0.0, b=1.0, palm_l=(0, 0, 1), palm_r=(0, 0, 1))
        r.place(ctrl, up, a=0.0, b=0.6, arc=0.15)
        r.place(ctrl, D['FRAME.Spare_Link_1'].matrix_world.copy(), a=0.6, b=1.0)
    return r.finish()


def _jack_under(r, sid, sign):
    """The jack stood under a station's arm (it is fetched by jack_off_the_plate first)."""
    D = bpy.data.objects
    axle = D['CTRL.Wheel_' + sid].matrix_world.translation
    pose_ = T(sign * 1.0, axle.y + 0.33, 0.37) @ Matrix.Rotation(math.radians(90), 4, 'Y')
    r.start_at('CTRL.Jack', pose_)
    return pose_


def _wind_jack(r, sid, degrees, turns, up=True):
    D = bpy.data.objects
    grip = next(o for o in D if '28A-O34_ARM' in o.name)
    level = ra.hand_pose((0, 0, 0), palm_to=(0, 0, -1), fingers=(0, -1, 0))
    arm = 'CTRL.Arm_' + sid
    keys = r.scalar.get((arm, 'rotation_euler', 0))
    now = math.degrees(keys[-1][1]) if keys else 0.0
    per = degrees / turns
    with r.step('jack_%s_turn' % ('up' if up else 'down'), kind='cycle', seconds=1.2, repeats=turns, input='turn', requires='jack under the arm',
                player='one turn of the jack crank (lift per turn not verified)'):
        r.turn('CTRL.Jack_Crank', 360.0 if up else -360.0)
        r.hand('R', grip.name, grip=T(0, 0, 0.09), a=0.0, b=1.0, orient=level)
        r.joint(arm, now + (per if up else -per), a=0.0, b=1.0, start=now)
        r.event('jack_ratchet', at=0.5)
    with r.step('jacked_%s' % ('up' if up else 'down'), seconds=0.6, player='the arm is %s' % ('clear of the track' if up else 'back on the track')):
        r.joint(arm, now + (degrees if up else -degrees), a=0.0, b=0.8)


def road_wheel_off(sid='L3'):
    """A3. Jack a station's arm up, undo the six nuts of its outer disc, draw the disc off and lay
    it down; then back on. The jack starts under the arm."""
    D = bpy.data.objects
    sign = 1.0 if sid[0] == 'L' else -1.0
    disc = 'CTRL.Wheel_%s_out' % sid
    c = D[disc].matrix_world.translation.copy()
    r = ra.Repair('road_wheel_off_' + sid, row='A3', title='Outer road wheel off station %s and back' % sid,
                  note='the jack starts under the arm; lift per turn of the jack is not verified')
    nuts = fasteners('wheel_%s_out' % sid)
    tool = tool_for(nuts[0])
    r.stand((sign * 2.45, c.y + 0.1, 0.0), 90.0 * sign)
    _jack_under(r, sid, sign)
    r.bring(tool)
    _wind_jack(r, sid, 6.0, 6, up=True)
    for i, nut in enumerate(nuts):
        undo(r, nut, 'nut_%d' % (i + 1))
    with r.step('disc_off', seconds=2.4, requires='six nuts off, arm jacked', player='put the spanner down, draw the disc off the axle and lean it down'):
        r.place(tool, ra.pose((sign * 2.3, c.y + 0.7, 0.02)), a=0.0, b=0.15, arc=0.2)
        two_hands(r, disc, (0.0, 0.3, 0.0), (0.0, -0.3, 0.0), a=0.25, b=1.0, palm_l=(0, -1, 0), palm_r=(0, 1, 0), fingers=(-sign, 0, 0))
        bpy.context.view_layer.update()
        off = D[disc].matrix_world.copy()
        off.translation = off.translation + Vector((sign * 0.4, 0, 0.05))
        r.place(disc, off, a=0.3, b=0.6)
        r.place(disc, T(c.x + sign * 0.55, c.y - 0.9, 0.42), a=0.6, b=1.0, arc=0.1)
        r.event('wheel_off', at=0.6)
    with r.step('axle_bare', kind='hold', seconds=0.8, player='the outer disc is off'):
        pass
    with r.step('disc_on', seconds=2.4, player='lift the disc back on the axle'):
        two_hands(r, disc, (0.0, 0.3, 0.0), (0.0, -0.3, 0.0), a=0.0, b=1.0, palm_l=(0, -1, 0), palm_r=(0, 1, 0), fingers=(-sign, 0, 0))
        r.place(disc, off, a=0.1, b=0.6, arc=0.1)
        r.follow(disc, 'FRAME.Wheel_%s_out' % sid, offset=Matrix.Identity(4), a=0.95, b=1.0, reach=0.35)   # its seat rides on the jacked arm
    with r.step('pick_up_spanner', seconds=0.9, tool=tool, player='pick the spanner up'):
        r.hand('R', tool, a=0.3, b=1.0)
        r.follow(tool, 'PLAYER.Stand', offset=T(0.28, 0.3, 1.0) @ Matrix.Rotation(math.radians(-90), 4, 'Y'), a=0.6, b=1.0, reach=0.3, hold=True)
    for i, nut in enumerate(nuts):
        do_up(r, nut, 'nut_%d_on' % (i + 1))
    _wind_jack(r, sid, 6.0, 6, up=False)
    return r.finish()


def road_wheel_off_L3():
    return road_wheel_off('L3')


def arm_chained_up(sid='L3'):
    """A4. A sagging station: jack its arm up and chain it clear of the track (concession). The
    chain is not modelled; the arm is held at 20 degrees."""
    sign = 1.0 if sid[0] == 'L' else -1.0
    c = bpy.data.objects['CTRL.Wheel_' + sid].matrix_world.translation.copy()
    r = ra.Repair('arm_chained_up_' + sid, row='A4', title='Swing arm %s jacked and chained up' % sid, note='chain not modelled; ends with the arm held up')
    r.stand((sign * 2.45, c.y + 0.3, 0.0), 90.0 * sign)
    _jack_under(r, sid, sign)
    _wind_jack(r, sid, 20.0, 16, up=True)
    with r.step('chain_on', seconds=1.5, player='pass the chain round the arm and make it fast (chain not modelled)'):
        r.hand('L', 'CTRL.Arm_' + sid, grip=ra.hand_pose((0.2 * sign, -0.3, 0.0), (0, 0, 1), (-sign, 0, 0)), a=0.0, b=1.0)
        r.event('chain_on', at=0.8)
    with r.step('held_up', kind='hold', seconds=1.0, player='the station is held clear: the tank runs on the next wheels'):
        pass
    return r.finish()


def arm_chained_up_L3():
    return arm_chained_up('L3')


def _sprocket_cap(side):
    """A5 (first part). Undo the eight screws of the sprocket's hub cap and draw the cap; back on.
    The tooth ring is not separate in the rig yet, so turning it is not shown."""
    D = bpy.data.objects
    sign = 1.0 if side == 'L' else -1.0
    ctrl = 'CTRL.Sprocket_Cap_' + side
    c = D[ctrl].matrix_world.translation.copy()
    r = ra.Repair('sprocket_cap_off_' + side, row='A5', title='Sprocket hub cap %s off and back' % side, note='tooth ring not separate yet; the screwdriver is a stand-in')
    screws = fasteners('sprocket_cap_' + side)
    tool = tool_for(screws[0])
    r.stand((sign * 2.4, c.y, 0.0), 90.0 * sign)
    r.bring(tool)
    for i, f in enumerate(screws):
        undo(r, f, 'screw_%d' % (i + 1), seconds=0.6)
    with r.step('cap_off', seconds=1.6, requires='eight screws out', player='draw the hub cap and set it down'):
        r.place(tool, ra.pose((sign * 2.3, c.y + 0.6, 0.02)), a=0.0, b=0.15, arc=0.2)
        r.hand('R', ctrl, grip=ra.hand_pose((sign * 0.1, 0, 0), (-sign, 0, 0), (0, 0, 1)), a=0.25, b=1.0)
        r.place(ctrl, T(c.x + sign * 0.45, c.y - 0.5, 0.1), a=0.3, b=1.0, arc=0.2)
        r.event('cap_off', at=0.4)
    with r.step('hub_open', kind='hold', seconds=0.8, player='the hub is open'):
        pass
    with r.step('cap_on', seconds=1.6, player='put the cap back'):
        r.hand('R', ctrl, grip=ra.hand_pose((sign * 0.1, 0, 0), (-sign, 0, 0), (0, 0, 1)), a=0.0, b=1.0)
        r.follow(ctrl, 'FRAME.Sprocket_Cap_' + side, offset=Matrix.Identity(4), a=0.9, b=1.0, reach=0.8)
    with r.step('pick_up_tool', seconds=0.9, tool=tool, player='pick the tool up'):
        r.hand('R', tool, a=0.3, b=1.0)
        r.follow(tool, 'PLAYER.Stand', offset=T(0.28, 0.3, 1.0) @ Matrix.Rotation(math.radians(-90), 4, 'Y'), a=0.6, b=1.0, reach=0.3, hold=True)
    for i, f in enumerate(screws):
        do_up(r, f, 'screw_%d_in' % (i + 1), seconds=0.6)
    return r.finish()


def sprocket_cap_off_L():
    return _sprocket_cap('L')


def _track_tension(side):
    """A6. Track wrench on the adjuster at the rear plate, turn: the idler moves rearward and the
    track tightens. The cap over the adjuster is still part of the plate; 4 mm a turn and the
    count are placeholders."""
    D = bpy.data.objects
    sign = 1.0 if side == 'L' else -1.0
    sock = 'CTRL.Track_Tensioner_' + side
    p = D[sock].matrix_world.translation.copy()
    wrench = ra.ensure_track_wrench().name
    r = ra.Repair('track_tension_' + side, row='A6', title='Track tensioned at the rear plate, %s side' % side,
                  note='adjuster cap not cut from the plate; the wrench is a stand-in; 4 mm a turn and ten turns are placeholders')
    r.stand((p.x, p.y + 0.85, 0.0), FWD)
    r.bring(wrench)
    on = Matrix.Rotation(math.radians(-90), 4, 'Y')          # the socket's X runs into the hull: wrench shaft back out along -X
    with r.step('wrench_on', seconds=1.0, tool=wrench, player='seat the track wrench on the adjuster'):
        r.follow(wrench, sock, offset=on, a=0.6, b=1.0, reach=0.4)
        r.hand('R', wrench, a=0.0, b=1.0)
        r.hand('L', wrench, a=0.5, b=1.0)
    level = ra.hand_pose((0, 0, 0), palm_to=(0, -1, 0), fingers=(0, 0, 1))
    with r.step('turn', kind='cycle', seconds=1.4, tool=wrench, repeats=10, input='turn', player='one turn of the adjuster'):
        r.turn(sock, 360.0)
        r.follow(wrench, sock, offset=on, a=0.0, b=1.0, reach=0.0)
        r.hand('R', wrench, a=0.0, b=1.0, orient=level)
        r.hand('L', wrench, a=0.0, b=1.0, orient=level)
        r.joint('CTRL.Idler_Tension_' + side, 0.004, a=0.0, b=1.0, start=0.0)
        r.event('adjuster_click', at=0.5)
    with r.step('tight', seconds=0.8, tool=wrench, player='the track is tight'):
        r.joint('CTRL.Idler_Tension_' + side, 0.04, a=0.0, b=0.6)
        r.follow(wrench, sock, offset=on, a=0.0, b=0.5, reach=0.0)
        r.carry(wrench, a=0.7, b=1.0)
        r.event('track_tight', at=0.6)
    return r.finish()


def track_tension_L():
    return _track_tension('L')


# ------------------------------------------------------------------------- tools and cables
def _tool_off(ctrl, rid):
    """F1. Open a tool's clamps, lift it off the roof and hold it; then back in its clamps."""
    D = bpy.data.objects
    ra.kit_tools()
    e = D['CTRL.' + ctrl]
    what = e.get('tool_kind', 'tool')
    mesh = next(c for c in e.children if c.type == 'MESH')
    clamps = [c.strip() for c in e['released_by'].split(',') if c.strip().startswith('Tool_')]
    c = e.matrix_world.translation.copy()
    r = ra.Repair(rid, row='F1', title='%s off its clamps and back' % what.capitalize(), note='clamp strap cut from its base by colour; the wire loop does not move')
    side = 1.0 if c.x > 0 else -1.0
    if abs(c.x) > 1.0:
        r.stand((c.x - side * 0.6, c.y, DECK_Z), -90.0 * side)
    else:
        r.stand((c.x, c.y + 0.75, DECK_Z), FWD)
    for i, cl in enumerate(clamps):
        open_to = min(100.0, float(D['CTRL.' + cl]['max']))
        with r.step('clamp_%d_open' % (i + 1), seconds=0.8, player='slip the wire loop and swing the strap up'):
            r.hand('L' if i % 2 else 'R', 'CTRL.' + cl, grip=ra.hand_pose((0.0, 0.1, 0.03), (0, 0, -1), (0, 1, 0)), a=0.0, b=1.0)
            r.joint('CTRL.' + cl, open_to, a=0.3, b=1.0)
            r.event('clamp_open', at=0.35)
    held = T(0.25, 0.35, 1.05) @ Matrix.Rotation(math.radians(-90), 4, 'Y')
    with r.step('lift', seconds=1.6, requires='clamps open', player='lift the %s off the roof' % what):
        r.hand('R', mesh.name, a=0.0, b=1.0)
        r.hand('L', mesh.name, a=0.0, b=1.0)
        up = D['FRAME.' + ctrl].matrix_world.copy()
        up.translation = up.translation + Z * 0.15
        r.place('CTRL.' + ctrl, up, a=0.25, b=0.6)
        r.follow('CTRL.' + ctrl, 'PLAYER.Stand', offset=T(0.0, 0.45, 1.05), a=0.95, b=1.0, reach=0.4, hold=True)
        r.event('tool_in_hand', at=1.0)
    with r.step('in_hand', kind='hold', seconds=0.8, player='the %s is in hand' % what):
        r.hand('R', mesh.name, a=0.0, b=1.0)
        r.hand('L', mesh.name, a=0.0, b=1.0)
    with r.step('back', seconds=1.8, player='lay it back in its clamps'):
        r.hand('R', mesh.name, a=0.0, b=1.0)
        r.hand('L', mesh.name, a=0.0, b=1.0)
        r.place('CTRL.' + ctrl, up, a=0.05, b=0.6, arc=0.05)
        r.place('CTRL.' + ctrl, D['FRAME.' + ctrl].matrix_world.copy(), a=0.6, b=1.0)
    for i, cl in enumerate(clamps):
        with r.step('clamp_%d_shut' % (i + 1), seconds=0.7, player='swing the strap down and catch the loop'):
            r.hand('L' if i % 2 else 'R', 'CTRL.' + cl, grip=ra.hand_pose((0.0, 0.1, 0.03), (0, 0, -1), (0, 1, 0)), a=0.0, b=1.0)
            r.joint('CTRL.' + cl, 0.0, a=0.1, b=0.8)
            r.event('clamp_shut', at=0.8)
    return r.finish()


def tool_off_hammer():
    return _tool_off('Tool_Hammer', 'tool_off_hammer')


def tool_off_crowbar():
    return _tool_off('Tool_Crowbar', 'tool_off_crowbar')


def tool_off_axe():
    return _tool_off('Tool_Axe', 'tool_off_axe')


def tool_off_shovel():
    return _tool_off('Tool_Shovel', 'tool_off_shovel')


def tool_off_wire_cutters():
    return _tool_off('Tool_Wire_Cutters', 'tool_off_wire_cutters')


def extinguisher_off():
    """E10 / F1. Pull the hand extinguisher off its bracket on the engine deck, hold it, put it back."""
    D = bpy.data.objects
    ctrl = 'CTRL.Extinguisher'
    c = D[ctrl].matrix_world.translation.copy()
    r = ra.Repair('extinguisher_off', row='E10', title='Hand extinguisher off its bracket and back', note='the bracket strap is not separate in the kit')
    r.stand((c.x * 0.45, c.y, DECK_Z), -90.0 if c.x > 0 else 90.0)
    with r.step('pull', seconds=1.4, player='pull the bottle off its bracket'):
        two_hands(r, ctrl, (0.0, 0.1, 0.0), (0.0, -0.1, 0.0), a=0.0, b=1.0, palm_l=(0, -1, 0), palm_r=(0, 1, 0), fingers=(0, 0, 1))
        r.follow(ctrl, 'PLAYER.Stand', offset=T(0.0, 0.4, 1.1), a=0.9, b=1.0, reach=0.7, hold=True)
        r.event('extinguisher_in_hand', at=1.0)
    with r.step('in_hand', kind='hold', seconds=0.8, player='the extinguisher is in hand'):
        two_hands(r, ctrl, (0.0, 0.1, 0.0), (0.0, -0.1, 0.0), a=0.0, b=1.0, palm_l=(0, -1, 0), palm_r=(0, 1, 0), fingers=(0, 0, 1))
    with r.step('back', seconds=1.4, player='push it back on its bracket'):
        two_hands(r, ctrl, (0.0, 0.1, 0.0), (0.0, -0.1, 0.0), a=0.0, b=1.0, palm_l=(0, -1, 0), palm_r=(0, 1, 0), fingers=(0, 0, 1))
        r.place(ctrl, D['FRAME.Extinguisher'].matrix_world.copy(), a=0.1, b=0.9, arc=0.05)
    return r.finish()


def _tow_cable_lift(side):
    """F2. Lift a tow cable's two eyes off their posts and put them back. The cable is rigid in
    the rig, so it only lifts; laying it out and hooking it up need the game's rope."""
    D = bpy.data.objects
    ctrl = 'CTRL.Tow_Cable_' + side
    e = D[ctrl]
    eyes = [D[e['eye_inner']], D[e['eye_outer']]]
    pts = []
    for o in eyes:
        w = np.array([o.matrix_world @ v.co for v in o.data.vertices])
        pts.append(Vector(w[w[:, 1].argmin()]))       # the eye's loop end, on its post
    c = (pts[0] + pts[1]) * 0.5
    r = ra.Repair('tow_cable_eyes_off_' + side, row='F2', title='Tow cable %s: eyes off their posts and back' % side,
                  note='rigid cable: it only lifts; hooking up needs the game\'s rope from the guide curve')
    r.stand((c.x - (0.6 if c.x > 0 else -0.6), c.y + 0.1, DECK_Z), -90.0 if c.x > 0 else 90.0)
    inv = ra._unscaled(e.matrix_world).inverted()
    grips = [ra.hand_pose(inv @ (p + Vector((0, 0.08, 0.03))), (0, 0, -1), (0, 1, 0)) for p in pts]
    with r.step('eyes_off', seconds=1.6, player='lift both eyes off their posts'):
        r.hand('L', ctrl, grip=grips[0], a=0.0, b=1.0)
        r.hand('R', ctrl, grip=grips[1], a=0.0, b=1.0)
        up = D['FRAME.Tow_Cable_' + side].matrix_world.copy()
        up.translation = up.translation + Z * 0.14
        r.place(ctrl, up, a=0.35, b=1.0)
        r.event('cable_free', at=0.9)
    with r.step('free', kind='hold', seconds=0.8, player='the cable is free of its posts'):
        r.hand('L', ctrl, grip=grips[0], a=0.0, b=1.0)
        r.hand('R', ctrl, grip=grips[1], a=0.0, b=1.0)
    with r.step('eyes_on', seconds=1.4, player='drop the eyes back over their posts'):
        r.hand('L', ctrl, grip=grips[0], a=0.0, b=1.0)
        r.hand('R', ctrl, grip=grips[1], a=0.0, b=1.0)
        r.place(ctrl, D['FRAME.Tow_Cable_' + side].matrix_world.copy(), a=0.1, b=0.9)
    return r.finish()


def tow_cable_eyes_off_L():
    return _tow_cable_lift('L')


# ------------------------------------------------------- turret, gun, hull fittings, engine bay
def _draw_out(rid, row, title, ctrl, stand, yaw, draw, note='', verb='draw it out', back='seat it again', seconds=1.6, grip_l=(0.0, 0.08, 0.0), grip_r=(0.0, -0.08, 0.0),
              state=None, hold_text='it is out', spare=False):
    """A part that comes out whole: both hands on it, drawn along `draw` (a world vector, metres),
    held, and seated again (the same part stands for a spare when `spare`). `state` lists
    (control, value) pairs that hold from the first frame, for what has to be open already."""
    D = bpy.data.objects
    r = ra.Repair(rid, row=row, title=title, note=note)
    r.stand(stand, yaw)
    home = D['FRAME.' + ctrl[5:]].matrix_world.copy()
    out = D[ctrl].matrix_world.copy()
    out.translation = out.translation + Vector(draw)
    with r.step('out', seconds=seconds, player=verb):
        for c, v in (state or []):
            r.joint(c, v, a=0.0, b=0.0, start=v)
        two_hands(r, ctrl, grip_l, grip_r, a=0.0, b=1.0, palm_l=(0, -1, 0), palm_r=(0, 1, 0), fingers=(0, 0, 1))
        r.place(ctrl, out, a=0.3, b=1.0)
        r.event('part_out', at=0.5)
    with r.step('held', kind='hold', seconds=0.8, player=hold_text):
        two_hands(r, ctrl, grip_l, grip_r, a=0.0, b=1.0, palm_l=(0, -1, 0), palm_r=(0, 1, 0), fingers=(0, 0, 1))
    with r.step('in', seconds=seconds, player=('seat the spare' if spare else back)):
        two_hands(r, ctrl, grip_l, grip_r, a=0.0, b=1.0, palm_l=(0, -1, 0), palm_r=(0, 1, 0), fingers=(0, 0, 1))
        r.follow(ctrl, 'FRAME.' + ctrl[5:], offset=Matrix.Identity(4), a=0.9, b=1.0, reach=seconds * 0.8)
        r.event('part_seated', at=1.0)
    return r.finish()


def _at(ctrl):
    return bpy.data.objects[ctrl].matrix_world.translation.copy()


def vision_block_swap_cupola():
    c = _at('CTRL.Vision_Block_Cupola_1')
    centre = Vector((0.87, 1.31, c.z))
    inward = (centre - c).normalized() * 0.16 + Vector((0, 0, -0.12))
    return _draw_out('vision_block_swap_cupola', 'B3', 'Cupola vision block pulled and a spare seated', 'CTRL.Vision_Block_Cupola_1', (0.87, 1.31, 1.05), 0.0, inward,
                     note='the block\'s clamp is not in the kit; the same block stands for the spare', verb='pull the cracked block inward and down', spare=True, hold_text='the slot is empty')


def driver_visor_block_swap():
    c = _at('CTRL.Driver_Visor_Glass')
    return _draw_out('driver_visor_block_swap', 'B2', 'Driver\'s visor block pulled and a spare seated', 'CTRL.Driver_Visor_Glass', (c.x, c.y + 0.7, 0.95), FWD, (0, 0.2, -0.1),
                     note='the visor shutter is not rigged; the same block stands for the spare', verb='pull the block inward', spare=True, hold_text='the slot is empty')


def hull_mg_out():
    c = _at('CTRL.Hull_MG')
    return _draw_out('hull_mg_out', 'B5', 'Bow MG drawn in and put back', 'CTRL.Hull_MG', (c.x, c.y + 1.2, 0.95), FWD, (0, 0.55, 0.0),
                     note='the ball mount does not turn in the rig', verb='draw the gun inward out of the ball', hold_text='the gun is out: change the barrel or swap the gun',
                     grip_l=(0.06, 0.2, 0.0), grip_r=(-0.06, 0.2, 0.0))


def headlamp_off():
    c = _at('CTRL.Headlamp')
    return _draw_out('headlamp_off', 'B6', 'Headlamp lifted off its foot and back', 'CTRL.Headlamp', (c.x, c.y - 0.9, 0.0), 0.0, (0, -0.1, 0.25),
                     verb='lift the lamp off its foot', hold_text='the lamp is off')


def mudguard_off_rear_L():
    c = _at('CTRL.Mudguard_Rear_L')
    return _draw_out('mudguard_off_rear_L', 'A8', 'Rear mudguard torn clear and hung back', 'CTRL.Mudguard_Rear_L', (c.x + 0.2, c.y + 1.0, 0.0), FWD, (0.15, 0.5, -0.2),
                     note='its fixings are not read from the kit; in the field it is levered off with the crowbar', verb='wrench the bent guard clear of the track', hold_text='the track runs free',
                     grip_l=(0.25, 0.0, 0.0), grip_r=(-0.25, 0.0, 0.0), seconds=2.0)


def ventilator_cowl_off():
    c = _at('CTRL.Ventilator_Cowl')
    return _draw_out('ventilator_cowl_off', 'C7', 'Ventilator cowl lifted off the turret roof and back', 'CTRL.Ventilator_Cowl', (c.x - 0.6, c.y + 0.3, c.z - 0.1), -90.0, (-0.3, 0.2, 0.25),
                     note='the fan under it is not in the rig', verb='lift the crushed cowl off', hold_text='the vent is clear')


def gunner_sight_swap():
    c = _at('CTRL.Gunner_Sight')
    return _draw_out('gunner_sight_swap', 'D6', 'Gunner\'s telescope drawn and the spare seated', 'CTRL.Gunner_Sight', (c.x + 0.1, c.y + 1.1, 1.05), FWD, (0, 0.6, -0.05),
                     note='its clamp is not in the kit; the same telescope stands for the spare', verb='draw the smashed telescope rearward out of its mount', spare=True, hold_text='the mount is empty',
                     grip_l=(0.05, 0.3, 0.0), grip_r=(-0.05, 0.3, 0.0), seconds=2.0)


def coax_mg_out():
    c = _at('CTRL.Coax_MG')
    return _draw_out('coax_mg_out', 'D7', 'Coaxial MG lifted out of its cradle and back', 'CTRL.Coax_MG', (c.x - 0.1, c.y + 1.0, 1.05), FWD, (0.0, 0.45, 0.12),
                     verb='lift the gun out of its cradle', hold_text='the gun is out: change the barrel or swap the gun', grip_l=(0.06, 0.3, 0.0), grip_r=(-0.06, 0.3, 0.0), seconds=2.0)


def aa_mg_off():
    c = _at('CTRL.AA_MG')
    return _draw_out('aa_mg_off', 'D8', 'Cupola MG lifted off its mount and back', 'CTRL.AA_MG', (0.87, 1.31, 1.75), FWD, (0.0, 0.3, 0.25),
                     note='the mount does not swing in the rig', verb='lift the gun off the cupola mount', hold_text='the spare gun is in hand', grip_l=(0.05, 0.2, 0.0), grip_r=(-0.05, 0.2, 0.0))


def round_from_rack():
    D = bpy.data.objects
    rounds = [o for o in D if o.name.startswith('CTRL.Round_')]
    e = max(rounds, key=lambda o: abs(o.matrix_world.translation.x) - 0.2 * abs(o.matrix_world.translation.y - 1.0))
    c = e.matrix_world.translation.copy()
    side = 1.0 if c.x > 0 else -1.0
    return _draw_out('round_from_rack', 'D9', 'A round drawn from a side rack and put back', e.name, (c.x - side * 0.75, c.y, 1.05), -90.0 * side, (-side * 0.45, 0.0, 0.05),
                     note='rack clamps are not in the kit; %s is the round shown' % e.name[5:], verb='draw the round out of its rack', hold_text='the round is in hand',
                     grip_l=(0.0, 0.25, 0.0), grip_r=(0.0, -0.25, 0.0), seconds=1.8)


def carburettor_off():
    c = _at('CTRL.Carburettor_1')
    return _draw_out('carburettor_off', 'E1', 'Carburettor lifted off the engine and back', 'CTRL.Carburettor_1', (0.0, 3.75, DECK_Z), FWD, (0.0, 0.25, 0.5),
                     note='the engine hatch is held open at 35 degrees (see engine_hatch_open); the carburettor\'s fixings and lines are not modelled',
                     verb='reach into the bay and lift the carburettor off', hold_text='the carburettor is out', state=[('CTRL.Deck_Hatch', 35.0), ('CTRL.Deck_Hatch_Lock_L', 0.04), ('CTRL.Deck_Hatch_Lock_R', 0.04)], seconds=2.2)


def air_filter_off():
    return _draw_out('air_filter_off', 'E5', 'Air filter bowl lifted out and back', 'CTRL.Air_Filter_A', (0.0, 3.75, DECK_Z), FWD, (0.0, 0.3, 0.55),
                     note='the engine hatch is held open at 35 degrees; the bowl\'s clamps are not modelled', verb='lift the filter bowl out to clean it', hold_text='the bowl is out',
                     state=[('CTRL.Deck_Hatch', 35.0), ('CTRL.Deck_Hatch_Lock_L', 0.04), ('CTRL.Deck_Hatch_Lock_R', 0.04)], seconds=2.2)


def aerial_swap():
    """E11. Unscrew the aerial rod from its base, lift it out, screw a spare in."""
    D = bpy.data.objects
    ctrl = 'CTRL.Aerial'
    c = D[ctrl].matrix_world.translation.copy()
    r = ra.Repair('aerial_swap', row='E11', title='Aerial rod unscrewed and a spare screwed in', note='turns not verified; the same rod stands for the spare')
    r.stand((c.x + (0.6 if c.x < 0 else -0.6), c.y, DECK_Z), 90.0 if c.x < 0 else -90.0)
    palm = ra.hand_pose((0.0, 0.0, 0.0), (1, 0, 0), (0, 0, 1))
    with r.step('unscrew', kind='cycle', seconds=0.7, repeats=8, input='stroke', player='half a turn anticlockwise (count not verified)'):
        r.hand('R', ctrl, grip=palm, a=0.0, b=0.85)
        r.spin_lift(ctrl, 0.5, 0.001, a=0.1, b=0.8)
    with r.step('rod_out', seconds=1.4, player='lift the broken rod out'):
        r.spin_lift(ctrl, 3.5, 0.007, a=0.0, b=0.3)
        r.hand('R', ctrl, grip=palm, a=0.0, b=1.0)
        out = D['FRAME.Aerial'].matrix_world.copy()
        out.translation = out.translation + Vector((0, 0, 0.25))
        r.place(ctrl, out, a=0.35, b=1.0)
        r.event('aerial_out', at=0.4)
    with r.step('spare_in', seconds=1.4, player='set the spare rod on the thread'):
        r.hand('R', ctrl, grip=palm, a=0.0, b=1.0)
        up = D['FRAME.Aerial'].matrix_world.copy()
        up.translation = up.translation + Vector((0, 0, 0.008))
        r.place(ctrl, up, a=0.1, b=0.9)
    with r.step('screw_in', kind='cycle', seconds=0.7, repeats=8, input='stroke', player='half a turn clockwise'):
        r.hand('R', ctrl, grip=palm, a=0.0, b=0.85)
        r.place(ctrl, D['FRAME.Aerial'].matrix_world.copy(), a=0.1, b=0.8)
        r.event('aerial_tight', at=0.8)
    return r.finish()


def bin_lid_open_L():
    """C9. Lift a turret bin lid and shut it."""
    D = bpy.data.objects
    ctrl = 'CTRL.Bin_Lid_L'
    c = D[ctrl].matrix_world.translation.copy()
    r = ra.Repair('bin_lid_open_L', row='C9', title='Turret bin lid lifted and shut', note='hinge side is a reading: the kit shows no hinge barrel')
    r.stand((c.x, c.y + 1.1, DECK_Z), FWD)
    grip = ra.hand_pose((0.0, 0.4, 0.02), (0, 0, 1), (0, -1, 0))
    with r.step('lift', seconds=1.2, player='lift the lid by its rear edge'):
        r.hand('R', ctrl, grip=grip, a=0.0, b=1.0)
        r.joint(ctrl, float(D[ctrl]['max']), a=0.25, b=1.0)
        r.event('bin_open', at=1.0)
    with r.step('open', kind='hold', seconds=0.8, player='the bin is open'):
        r.hand('R', ctrl, grip=grip, a=0.0, b=1.0)
    with r.step('shut', seconds=1.0, player='let it down'):
        r.hand('R', ctrl, grip=grip, a=0.0, b=1.0)
        r.joint(ctrl, 0.0, a=0.0, b=0.9)
    return r.finish()


def breech_cleared():
    """D4. A jammed breech: work the lever, the block drops, clear it, close. The kit pose has the
    block down (open), so the sequence starts by showing it closed."""
    D = bpy.data.objects
    c = D['CTRL.Breech_Block'].matrix_world.translation.copy()
    r = ra.Repair('breech_cleared', row='D4', title='Jammed breech cleared with the lever',
                  note='lever 02J66 is a reading of the kit; the block\'s closed height (+0.22 m) is where it centres in the ring')
    r.stand((c.x - 0.6, c.y + 0.5, 1.05), -90.0)
    lever = ra.hand_pose((0.0, 0.28, 0.0), (0, 0, -1), (0, 1, 0))
    with r.step('closed', kind='hold', seconds=0.8, player='the block is up and will not drop'):
        r.joint('CTRL.Breech_Block', 0.22, a=0.0, b=0.0, start=0.22)
        r.hand('R', 'CTRL.Breech_Lever', grip=lever, a=0.3, b=1.0)
    with r.step('lever', kind='cycle', seconds=0.9, repeats=3, input='stroke', player='heave on the breech lever (count not verified)'):
        r.hand('R', 'CTRL.Breech_Lever', grip=lever, a=0.0, b=1.0)
        r.joint('CTRL.Breech_Lever', 40.0, a=0.1, b=0.5)
        r.joint('CTRL.Breech_Lever', 0.0, a=0.6, b=0.95)
        r.event('breech_lever', at=0.5)
    with r.step('block_drops', seconds=0.8, player='the block drops: the breech is open'):
        r.hand('R', 'CTRL.Breech_Lever', grip=lever, a=0.0, b=1.0)
        r.joint('CTRL.Breech_Lever', 50.0, a=0.0, b=0.4)
        r.joint('CTRL.Breech_Block', 0.0, a=0.3, b=0.7, start=0.22)
        r.event('breech_open', at=0.7)
    with r.step('clear', seconds=1.2, player='clear the chamber by hand'):
        r.hand('L', 'CTRL.Breech_Block', grip=ra.hand_pose((0.0, -0.1, 0.25), (0, -1, 0), (0, 0, 1)), a=0.1, b=0.9)
        r.event('chamber_clear', at=0.6)
    with r.step('close', seconds=0.8, player='let the lever back: the block rises'):
        r.hand('R', 'CTRL.Breech_Lever', grip=lever, a=0.0, b=1.0)
        r.joint('CTRL.Breech_Lever', 0.0, a=0.0, b=0.6)
        r.joint('CTRL.Breech_Block', 0.22, a=0.3, b=0.8)
        r.event('breech_closed', at=0.8)
    return r.finish()


def gun_back_into_battery():
    """D2. Mount bolts sheared, the gun stuck back: lever it forward into battery with the crowbar
    (concession; the manual's method is the jack). Mount bolts are not identified in the kit."""
    tools = ra.kit_tools()
    bar = tools['crowbar']
    D = bpy.data.objects
    c = D['CTRL.Gun_Recoil'].matrix_world.translation.copy()
    r = ra.Repair('gun_back_into_battery', row='D2', title='Gun levered back into battery',
                  note='starts with the gun 0.5 m back; mount bolts not identified in the kit; heaves not verified')
    r.stand((c.x - 0.55, c.y + 1.2, 1.05), FWD)
    r.bring(bar)
    with r.step('stuck_back', kind='hold', seconds=0.8, player='the gun stands back out of battery'):
        r.joint('CTRL.Gun_Recoil', 0.5, a=0.0, b=0.0, start=0.5)
    with r.step('heave', kind='cycle', seconds=1.2, repeats=17, input='stroke', tool=bar, player='one heave on the bar behind the breech'):
        r.joint('CTRL.Gun_Recoil', 0.47, a=0.4, b=0.8, start=0.5)
        r.hand('L', bar, a=0.0, b=1.0)
        r.event('heave', at=0.5)
    with r.step('in_battery', seconds=1.0, tool=bar, player='the gun is home'):
        r.joint('CTRL.Gun_Recoil', 0.0, a=0.0, b=0.7)
        r.event('in_battery', at=0.7)
    with r.step('mount_bolts', seconds=1.2, player='fit the spare mount bolts (not identified in the kit)'):
        r.event('mount_bolts_in', at=0.8)
    return r.finish()


def hand_traverse():
    """C3. Hand traverse on the commander's handwheel. Degrees of turret per turn are not verified."""
    D = bpy.data.objects
    w = 'CTRL.Traverse_Handwheel_Commander'
    c = D[w].matrix_world.translation.copy()
    r = ra.Repair('hand_traverse', row='C3', title='Hand traverse by the commander\'s handwheel', note='0.5 degree of turret per turn is a placeholder, not verified')
    r.stand((0.87, 1.31, 1.05), FWD)
    level = ra.hand_pose((0, 0, 0), palm_to=(0, 1, 0), fingers=(0, 0, 1))
    with r.step('turn', kind='cycle', seconds=0.9, repeats=20, input='turn', player='one turn of the handwheel'):
        r.turn(w, 360.0)
        r.turn('CTRL.Turret_Traverse', 0.5)
        r.hand('R', w, grip=T(0.0, 0.0, 0.09), a=0.0, b=1.0, orient=level)
        r.event('traverse_click', at=0.5)
    with r.step('laid', seconds=0.8, player='the turret has come round'):
        r.turn('CTRL.Turret_Traverse', 9.5, a=0.0, b=0.8)
    return r.finish()


def hand_elevate():
    """C3. Elevation by the handwheel. Degrees of gun per turn are not verified."""
    D = bpy.data.objects
    w = 'CTRL.Elevation_Handwheel'
    c = D[w].matrix_world.translation.copy()
    r = ra.Repair('hand_elevate', row='C3', title='Gun elevated by the handwheel', note='0.5 degree of gun per turn is a placeholder, not verified')
    r.stand((c.x + 0.25, c.y + 0.6, 1.05), FWD)
    level = ra.hand_pose((0, 0, 0), palm_to=(-1, 0, 0), fingers=(0, 0, 1))
    with r.step('turn', kind='cycle', seconds=0.9, repeats=10, input='turn', player='one turn of the elevation handwheel'):
        r.turn(w, 360.0)
        r.joint('CTRL.Gun_Elevation', 0.5, a=0.0, b=1.0, start=0.0)
        r.hand('R', w, grip=T(0.0, 0.0, 0.09), a=0.0, b=1.0, orient=level)
    with r.step('laid', seconds=0.8, player='the gun is laid'):
        r.joint('CTRL.Gun_Elevation', 5.0, a=0.0, b=0.8)
    return r.finish()


def turret_ring_freed():
    """C1. A fragment wedged in the ring gap: knock it out with the hammer from outside, then the
    turret traverses again. The fragment is not modelled."""
    tools = ra.kit_tools()
    hammer = tools['hammer']
    r = ra.Repair('turret_ring_freed', row='C1', title='Fragment knocked out of the turret ring', note='fragment not modelled; blows not verified')
    ring = Vector((0.0002, 1.0124, 1.8352))
    spot = ring + Vector((0.9, -0.52, 0.03))
    r.stand((1.35, -0.2, DECK_Z), 110.0)
    r.bring(hammer)
    strike = hammer_pose(hammer, spot, (-0.5, 0.85, -0.15), (0.6, -0.3, 0.7))
    back = swing_back(hammer, strike, (-0.5, 0.85, -0.15))
    with r.step('blow', kind='cycle', seconds=0.8, repeats=6, input='strike', tool=hammer, player='one blow at the ring gap'):
        r.place(hammer, back, a=0.0, b=0.4)
        r.place(hammer, strike, a=0.42, b=0.58)
        r.place(hammer, back, a=0.62, b=1.0)
        r.hand('R', hammer, a=0.0, b=1.0, hold=True)
        r.event('hammer_strike', at=0.58)
    with r.step('free', seconds=1.6, player='the fragment is out: the turret turns'):
        r.turn('CTRL.Turret_Traverse', 12.0, a=0.2, b=1.0)
        r.event('ring_free', at=0.2)
    return r.finish()


ALL = [starter_hand_crank, jack_off_the_plate, engine_hatch_open, fan_grille_off_L, fan_grille_off_R, deck_cap_FL,
       track_link_replace_L, track_link_replace_R, spare_link_off, road_wheel_off_L3, arm_chained_up_L3, sprocket_cap_off_L, track_tension_L,
       tool_off_hammer, tool_off_crowbar, tool_off_axe, tool_off_shovel, tool_off_wire_cutters, extinguisher_off, tow_cable_eyes_off_L,
       vision_block_swap_cupola, driver_visor_block_swap, hull_mg_out, headlamp_off, mudguard_off_rear_L, ventilator_cowl_off, gunner_sight_swap,
       coax_mg_out, aa_mg_off, round_from_rack, carburettor_off, air_filter_off, aerial_swap, bin_lid_open_L, breech_cleared, gun_back_into_battery,
       hand_traverse, hand_elevate, turret_ring_freed]


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
