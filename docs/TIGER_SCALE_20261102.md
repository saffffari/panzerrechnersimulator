# The Tiger at the real vehicle's size — 2 November 2026

## The finding

The damage session's registration (`claude/fea-damage-20261102`, `docs/research/TIGER_CAD_COMPONENTS_20261102.md`)
measured that the game's baked Tiger (`assets/tanks/tiger/baked/tiger_hull.glb`, `_turret.glb`,
`_gun.glb`) is the owner's Rye Field RM-5010 1/35 kit CAD tessellated at 38.919815 per kit
millimetre (the node scale in the GLBs), identity rotation, 0.17 mm RMS over 446 solids: 1.112
times the kit's 1:35. The hull stood 7.13 m long and 4.12 m wide against the real 6.32 m and
3.705 m. Everything authored in the hull and turret frames sits on that oversize bake.

## The approach: the frames keep their units, one scale brings them to the world

Rebaking and re-authoring would touch hundreds of numbers. Instead `tank::Parts::frame_scale`
(1 for the enemies' bakes, `TIGER_FRAME_SCALE` = 35 / 38.919815 = 0.8993 for the Tiger) is
applied once, at the end of `Tank::hull_model`, about the hull origin at the ground contact. The
turret and gun matrices compose on it, so the bake, the interior rig, the cradles, the hatches,
the head box, the lamps and every constant authored in the frames scale with it, as do the 267
turret-frame and 279 hull-frame matrix uses and their 38 inverses (world to frame) in the source.
The two rig numbers that were used as world metres, `center_h` and `hull_half_extents`, are now
stated in world metres in `TIGER_PARTS`.

## What is real-size inside the frames, and so must be entered in frame units

The frames are the kit's units (1 unit = 0.8993 m). A real-size thing placed by a number in the
frame must be entered as metres / frame_scale (`Parts::local`):

- the printed order sheet (210 x 150 mm paper): `order_printer::strip_model` and
  `hanging_sheet_box` take the machine frame's scale (the turret's, or 1 on the camp table) — done;
- the crew's eye heights and reaches entered in the turret frame (`Game::station_eye`: the
  loader's 1.29 m above the floor, the crouch 0.50 m; the radioman's seat; the walk positions),
  the hands' reach, the climb ritual's handholds, the personal-space capsules, the crew poses:
  83 sites compose a frame point into the world (`turret_model().transform_point3(..)`,
  `hull_model().transform_point3(..)`), of which the ones that add a human-size offset are the
  second stage (`game.rs` 21, `crew_solo.rs` 13, `tiger_refuel.rs` 3, `game_acoustics.rs` 3,
  `equipment.rs` 3, `tiger_cupola.rs` 2, `small_arms_runtime.rs` 2 and single sites elsewhere);
- the crew's bodies and the carried things are world meshes posed at frame anchors: their
  anchors move with the frame, their size does not.

## Status

Stage one landed behind a gate: `KESSEL_TIGER_TRUE_SCALE=1` turns the frame scale on; off, the
bake keeps its 1.112 size, so the main line stays consistent until the second stage. With the
gate on, the crew-solo harness passes the same 15 of 20 checks as before and the climb all five,
and the unit tests that compose real-size meshes through the frames show the second stage's
work: the ragdoll segments and the held round are drawn through the scaled frame (a tenth
small aboard, their size in the world when detached), so every real-size mesh placed in a frame
must carry S(1 / frame_scale) after its local pose, and every human-size offset in a frame
`Parts::local`. The frame scale, the world-metre footprint and hit centre, and the sheet. The
crew harnesses (`crew-solo`, `climb`) before and after are the gates for the second stage, the
re-fit of the human-size offsets listed above; their stills are
`captures/demo-20261101-tiger_scale_before_20261102a_*` and the `_after_` set.
