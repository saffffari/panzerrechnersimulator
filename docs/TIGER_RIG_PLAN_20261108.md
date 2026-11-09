# Tiger gen2: how the breakable things are rigged (8 November 2026)

The owner, 8 November: "lets go down the list of things that are going to need repair sequences
and plan then implement how we are going to rig them. lot of screws on this model that will need
to be properly integrated as screw objects". This is the plan for every row of
`docs/TIGER_DESTRUCTIBLES_20261108.md`, and the record of what is built. The rear plate is done
first, as he chose.

File: `tiger_gen2_live.blend` (backup before this pass: `tiger_gen2_live.pre-rearplate-20261108.blend`).
Tools: `tools/tiger_production/rig_lib.py`, `split_lib.py`, `fasteners.py`, `fastener_census.py`,
`rear_plate_rig.py`.

## The five building blocks

Every repair in the game is a short run of hand actions. Each action maps to one of five things in
the rig, so a repair sequence is a list of them in order.

| Block | In Blender | What the game does with it |
|---|---|---|
| Hinge | `FRAME.<Name>` + `CTRL.<Name>`, `rig_kind` hinge, angle on local X, `min`/`max` | swing a part between limits; `continuous` marks a crank that goes right round |
| Slide | same pair, `rig_kind` slide, metres on local X | push, pull, lift |
| Stowed part | `rig_kind` stow, free on every channel, identity = on its seat, `released_by` | pick the whole thing up once its latches and fastener sets are open |
| Socket | a hinge with no parts and `accepts` = a stowed part | where a carried tool is seated and worked (a crank in its port) |
| Fastener | an object in `FASTENERS`: origin on the seat, local +Z the way it comes out, `fastener_kind`, `fastener_shape`, `fastener_size_m`, `fastener_removable`, `fastener_set` | undo one by one with the right tool; a part is free when its set is out |

Rules that hold for all of them:

- A control's zero is the kit pose, with what moves parented under the control. Limits come from a
  swept collision test against the parts round it, and the control says what stopped it
  (`limit_source`).
- A kit part that holds two things which move apart is cut into two objects (`split_lib`). The kit
  parts are fused solids, so each cut is closed again and the way it was closed is a named
  approximation on the control.
- Heads are read off the full CAD, not the reduced mesh, because a 1 mm reduction can turn an
  11 mm round head into a hexagon. Six corners is a bolt, four a square head, anything else round.
  Whether a round head is a rivet or a screw is decided per part and written down.
- Each control carries the play notes `interaction`, `damage`, `repair`, and where one thing holds
  another, `released_by`, `locked_by` or `requires`.

## Built: the rear plate (rows B8, E9, F3)

Stills: `captures/demo-20261101-tiger-rear-plate-rig-20261108-rest.png` and `-open.png`.

What the kit turned out to hold:

- The crank on the geared teardrop housing winds the **jack**. The housing (`28A-O39`) sits on the
  toothed rack of the jack body (`28A-O38`); the arm with the pinion is `28A-O31`; the grip and the
  strap round it were one part, `28A-O34`.
- The engine's hand start is lower down. `28A-B69` is a domed cover with one hex head, on the
  plate's centre line, and the plate has a round hole behind it. `28A-Z16` is a 0.72 m shaft with a
  24 mm square end, in two clips beside it, low on the plate. I take these for the inertia starter's
  port and the starting crank's shaft. **Not verified** against a drawing or a photograph, and the
  kit has no crank throw for the shaft.

| Control | Kind | Carries | Range | Held by |
|---|---|---|---|---|
| `CTRL.Jack` | stowed part | jack unit `0B30`, gear case, the crank | free | the crank strap; the two holder clamps |
| `CTRL.Jack_Crank` | hinge, continuous | arm with pinion, grip | full turns off the tank; -10 to +12 degrees on the plate (throw 0.336 m, the plate is nearer) | `Jack_Crank_Latch` |
| `CTRL.Jack_Crank_Latch` | hinge | the strap's front jaw | 0 to 126 degrees, stopped by the exhaust shroud | its catch loop |
| `CTRL.Starter_Port_Cover` | stowed part | cover `28A-B69` | free | fastener set `starter_port_cover` (one bolt) |
| `CTRL.Starter_Crank` | stowed part | shaft `28A-Z16` | free | its two clips |
| `CTRL.Starter_Crank_Turn` | socket, continuous | nothing; accepts `Starter_Crank` | full turns | needs the cover off |

Parts cut:

- `28A-O34` into the grip (keeps the name), the strap's front jaw `28A-O34L` and its fixed rear
  jaw `28A-O34R`. All three are closed, with no open or doubled edges.
- `28A-Z16` into the shaft (keeps the name, carried straight through where the clips hid it) and
  the two clips `28A-Z16C`.

Fasteners made, 145 objects in 8 sets:

| Set | On | Count | Kind | Head |
|---|---|---|---|---|
| `starter_port_cover` | `28A-B69` | 1 | bolt, comes out | hex, 15.7 mm across flats |
| `jack_holder.000`, `.001` | `28A_HOLDER` x2 | 2 + 2 | bolt, comes out | hex, 14 mm |
| `jack_gear_case_cover` | `28A-O39` | 5 | screw, comes out | round, 10.5 mm |
| `jack_body` | `28A-O38` | 91 | rivet, fixed | round, 11 and 9 mm |
| `rear_centre_plate` | `28A-J104` | 30 + 2 | rivet, fixed; bolt, comes out | round 12.3 mm; hex 24.5 mm |
| `rear_plate_top_edge` | `30Z02` | 4 | bolt, comes out | hex, 35 mm |
| `rear_plate_disc` | `30Z02` | 8 | screw, comes out | round, 34 mm |

The jack body alone went from 3,290 triangles to 772 once its rivets were objects.

Named approximations in this pass:

1. The strap is one closed ring in the kit. It is cut by the plane through the grip's axis that
   faces the plate; the front half swings on a pin put at the strap's solid end, and the catch
   loop goes whole with it. The kit shows no pin, no bracket and no parting line right through.
2. The port cover has no hinge in the kit, so it comes off whole.
3. The shaft's clips stay closed and in one piece; the shaft is lifted out through them.
4. A fastener is a plain prism for the head. There is no shank or thread. The thread size on hex
   heads (`fastener_thread_guess`) is read from the across-flats by the DIN 931 table.
5. "Rivet" for the round heads on the jack body and on plate `28A-J104` is my reading of those
   parts. The two round discs on the rear plate, each with four screws, are not identified.

Left open on the rear plate: the two holder clamps and the shaft clips as opening parts; the track
tensioner adjusters (not yet found in the kit; the parts I named in row A6 were wrong); the gear
case's ratio (pinion teeth can be counted from the full CAD); exhaust shrouds, hooks and the chain
as stowed parts.

## Built: the engine deck (row B7)

Specs: `tools/tiger_production/rig_specs.py` (`DECK`), built by `rig_build.py`. Sides: the tank's
left is +X in the file, and `_L` names are the +X parts.

| Control | Kind | Carries | Range | Held by |
|---|---|---|---|---|
| `CTRL.Deck_Hatch` | hinge | engine hatch `34Z03`, intake cover `34B24`, plate `34B18`, arms `34D28`, blocks `34F06` | 0 to 100 degrees; 35 with the turret at 12 o'clock, where the lid meets the turret bin | the two side bolts |
| `CTRL.Deck_Hatch_Lock_L`, `_R` | slide | bolt `34F08` | 0 to 0.04 m | |
| `CTRL.Deck_Hatch_Bar` | hinge | folding bar `34D11` at the hatch's rear edge | -38 to 44 degrees | |
| `CTRL.Fan_Grille_L`, `_R` | stowed part | lattice grille `34F14` / `34F13` and its folding bar | free | two wing nuts each (`fan_grille_L`, `_R`) |
| `CTRL.Fan_Grille_Bar_L`, `_R` | hinge | folding bar `34D11` | -2 to 90 degrees | |
| `CTRL.Fan_Cover_L`, `_R` | stowed part | plate `34F11` / `34F12` under the lattice | free | the grille over it |
| `CTRL.Deck_Cap_FL`, `FR`, `RL`, `RR` | stowed part, unscrews | filler cap `34D42` | free | its thread |

The four wing nuts `35D15` are tagged as fasteners as they are (kit parts, turned by hand). The
full-CAD census found no other removable heads on the deck parts: what it found are posts and
hinge pins.

Named approximations and unknowns:

1. The hatch hinge is put on the front feet of the gooseneck arms, at the hatch's top face. This
   follows the real vehicle (front hinge, lid opens toward the turret) and the kit's use of ARM
   for hinge arms; no hinge barrel is modelled. Not checked on a drawing.
2. The 100 degree open limit is chosen. The kit has no stay, and nothing stops the lid before 165
   degrees with the turret out of the way.
3. The side bolts have no keeper in the kit. Their 0.04 m travel is the distance that draws the
   bolt end inside the hatch edge.
4. What the folding bars are (lifting handles, clasps or stays) is not known. They are used as
   lifting handles.
5. Which edge the wing nuts clamp, and which fluid each cap covers, are not identified.

Still open on the deck: the two louvred radiator grilles are part of the roof `33B01` and are not
cut out; the round cover `34B23` behind the hatch is not identified.

## Built: the running gear (rows A1 to A7)

Script: `tools/tiger_production/running_gear_rig.py`. Stations are numbered 1 to 8 from the front
on each side; discs are `in`, `mid`, `out` from the hull outward.

| Control | Kind | Count | Range or note |
|---|---|---|---|
| `CTRL.Arm_<L|R><1-8>` | hinge on the torsion bar's axis | 16 | swept with the wheels on and the track ignored: up to 35 degrees with nothing in the way (18 on R2, 34 on L8 and R8), down 5 to 6 degrees where the next station's disc stops it |
| `CTRL.Wheel_<id>` | hinge, continuous | 16 | the station's three discs turn together |
| `CTRL.Wheel_<id>_in`, `_mid`, `_out` | stowed part | 48 | each disc draws off its axle outward |
| `CTRL.Sprocket_L/R`, `CTRL.Sprocket_Cap_L/R` | continuous hinge; stowed hub cap | 2 + 2 | tooth ring not cut from its disc |
| `CTRL.Idler_L/R` on `CTRL.Idler_Tension_L/R` | continuous hinge on a slide | 2 + 2 | slide 0 to 0.08 m rearward |
| `CTRL.Track_Tensioner_L/R` | socket for the track wrench | 2 | at the conical cap low on the rear plate; drives the idler slide |
| `CTRL.Final_Drive_L/R`, `CTRL.Final_Drive_Cover_L/R` | stowed parts | 2 + 2 | workshop job |
| `CTRL.Track_Pin_<L|R>_a`, `_b` and `CTRL.Track_Link_<L|R>` | two slides and a stowed link | 4 + 2 | the joint to break: the rearmost link on the idler |
| `CTRL.Spare_Link_1` to `_7` | stowed part | 7 | spare links on the turret side |

Also made: `GUIDE.Track_L` and `GUIDE.Track_R`, closed curves through the 96 track units a side in
loop order (pitch 130.4 mm), and `track_side`, `track_index` on every unit, so the game can run
the links along the curve and break the track at any unit.

Fasteners: 294 objects in 54 sets, read from the full CAD. Wheel nuts are hex heads: six of 28 mm
on each outer disc and on the middle disc of the 0B17 stations, eight of 19 mm on the inner disc of
the 0B18 stations. Fifteen 28 mm bolts hold each final drive box; eight screws each sprocket cap;
twelve each idler hub; one 32 mm bolt each arm.

Named approximations: an arm and its torsion bar turn as one rigid part; the idler slides in place
of swinging on a crank the kit does not have, and its travel and the 4 mm a turn of the adjuster
are placeholders; the adjuster's cap is still part of the rear plate; the joint to break is one
fixed joint a side; the upper limit of 35 degrees on most arms is the end of the sweep, not a bump
stop, because the kit has none.

## Built: tools and tow cables (rows F1, F2, E10)

Script: `tools/tiger_production/tools_rig.py`.

| Control | Kind | Note |
|---|---|---|
| `CTRL.Tool_Axe`, `Tool_Shovel`, `Tool_Hammer`, `Tool_Crowbar`, `Tool_Wire_Cutters` | stowed part | each held by its clamps |
| `CTRL.Tool_<name>_Clamp_<n>` | hinge, 7 in all | strap swings up: 50 degrees where the turret overhangs (axe, shovel), 106 to 140 elsewhere |
| `CTRL.Extinguisher` | stowed part | bracket strap not separate |
| `CTRL.Tow_Cable_L`, `_R` | stowed part | cable with its two eyes, rigid; carries its guide curve |
| `CTRL.Tow_Point_Front_L/R`, `Tow_Point_Rear_L/R` | socket | at the four shackles |

The seven clamps shared one mesh. It was cut once where the kit changes colour, into a base with
its wire loop and a strap, and each clamp got its strap as a second object. The hinge pin is put
at the end away from the wire loop (named approximation). The 1.79 m bar `32O06` is taken for the
crowbar. A tow cable does not bend in the rig: the game's rope starts from the guide curve. Track
tools and cleaning rods (F4, F5) are not in the kit; the drift and track wrench are stand-ins.

## Built: turret, gun, hull fittings, engine bay (rows A8, B2 to B6, C1 to C9, D1 to D9, E1 to E5, E11)

Script: `tools/tiger_production/turret_hull_rig.py`. **Most parts in this group were read from
their kit names, boxes and a few fitted axes, not looked at one by one.** Each control says so in
its `source`. Treat the identifications here as first readings.

The kit has no turret assembly: its parts hang in ten sibling assemblies. The rig now has the
chain `Turret_Traverse` (upright axis through the ring gear's fitted circle, radius 1.039 m),
`Gun_Elevation` under it, `Gun_Recoil` under that, with `Breech_Block` and `Breech_Lever` on the
recoiling mass. The mantlet rides the elevation and not the recoil, which is what row D2 asks for.

| Control | Kind | Range | Note |
|---|---|---|---|
| `CTRL.Turret_Traverse` | hinge, continuous | all round | carries the ten turret assemblies |
| `CTRL.Gun_Elevation` | hinge | -8 to +15 degrees | the vehicle's limits from memory, not verified. In the kit the sweep stops at -2 and +5, so the trunnion axis (put through bar `01A33`) or the kit's clearances are off |
| `CTRL.Gun_Recoil` | slide | 0 to 0.58 m | nominal recoil from memory, not verified; the kit's barrel collar meets bar `01A33` at 0.21 m |
| `CTRL.Breech_Block` | slide | 0 to 0.22 m | the kit pose is the block dropped (open); +0.22 m centres it in the ring |
| `CTRL.Breech_Lever` | hinge | -18 to 56 degrees | arm `02J66`, taken for the lever |
| `CTRL.Traverse_Handwheel_Commander`, `CTRL.Elevation_Handwheel` | hinge, continuous | | the two handwheels `05J55`; each names what it drives |
| `CTRL.Traverse_Gear_Gunner`, `CTRL.Traverse_Pedal` | stowed part; hinge -10 to 10 | | gunner's gear `10J49` (wheel not separate); pedal travel chosen |
| `CTRL.Bin_Lid_L/R` | hinge | 0 to 50 degrees | hinged along the forward edge (a reading) |
| 34 single stowed parts | stowed part | | muzzle cover, gunner's sight, coaxial MG, cupola MG, ventilator cowl, recoil cylinders, side viewers, bow MG, headlamp, driver's visor block, four mudguards, engine, two carburettors, two air filter bowls, two fuel tanks, two radiators, gearbox, steering unit, radio, aerial |
| `CTRL.Vision_Block_*` | stowed part, 10 | | seven cupola blocks and three hatch and roof blocks |
| `CTRL.Seat_1` to `_8` | stowed part | | which crewman's each is, is not identified |
| `CTRL.Round_001` to `_090` | stowed part | | every 8.8 cm round, with its long axis |

Not rigged in this group, and why: the driver's controls, gearbox selector and steering wheel (E6
to E8: the lever and pedal parts are not identified); the visor shutter (B2); the bow MG's ball
(B5); fan blades as turning parts (E4: fused into their housings); radiators cannot be lifted out
in an animation while the louvred grilles are still part of the roof (E3); the fixed extinguisher
system (E10, not found); fuel leaks and bungs (E2, no rig motion).

## Fasteners model-wide

`tools/tiger_production/fastener_census.py ALL` read every kit part's full CAD once: 2,113 heads
on 189 of 367 parts (1,670 round, 370 hex, 73 square; parts that repeat, like wheels, count once).
The census file is `artifacts/tiger_all_fastener_census_20261108.json` (not committed).

`tools/tiger_production/fasten_all.py` then turned every hex and square head into a bolt object,
on all 62 parts that have them: 419 objects, every cut closed. With the rear plate, running gear
and deck that makes **935 fastener objects**: 684 bolts, 53 screws and 4 wing nuts that come out,
and 194 rivets that do not.

Round heads elsewhere are still moulded into their parts. Whether each part's round heads are
rivets or screws is a per-part call, and the census lists them: the largest are the driver's
floor plate `20K29` (125), the hull tub `16LH01` (123), the fuel tanks `27H12` (104 each), plate
`25K43` (68) and cover `03J69` (44). One mesh, `25K43_PLATE`, has 13 open edges after this pass;
it has not been checked whether they were there before.

## Repair animations

The owner, 8 November: "with each repair i want you to animate the repair within blender as a
sequence of interactions performed by the player that we can bring into the game. tools and player
manipulation should be included in the animations."

How they are stored (`tools/tiger_production/repair_anim.py`, sequences in `repairs.py`):

- One Action per repair, `REPAIR.<id>`, with a slot for every object that moves in it: rig
  controls, fastener objects, tools, the two hand targets `HAND.L` and `HAND.R`, and the standing
  point `PLAYER.Stand`.
- A repair is a run of steps. Each step is a frame range with a pose marker `step:<name>`. What
  the step means for the game is JSON on the action: what the player does, the tool, the fastener
  set, what must be true first, and its kind.
- Step kinds: `oneshot` plays once when triggered; `cycle` holds one stroke (one swing of a
  spanner, one turn of a crank) and a count, and the game repeats it at the player's pace; `hold`
  is a state the player stays in (lid open, bay open).
- Every repair has its own block of frames on the scene's one timeline, 30 frames a second, with
  scene markers `R:<id>` and `s:<step>`. Between blocks the tank is on its rest pose.
  `repair_anim.focus('<id>')` sets the preview range to one repair.
- `repair_anim.export(path)` samples every step: degrees or metres per frame for hinges and
  slides, position and rotation in the tank's frame for everything else, plus the step table and
  events (`ratchet`, `fastener_out`, `hatch_open`, ...). Written to
  `assets/tanks/tiger/tiger_gen2_repairs.json` beside the rig file.

What is and is not animated:

- Parts, fasteners and tools move as the rig allows and are checked by numbers
  (`repair_anim.check`): travel, and every part back on its seat at the end.
- Hands are targets, a palm position and direction baked to follow what they hold. The game's
  arms reach for them. There is no body animation: the standing point is where the feet are.
- Tools the kit lacks are plain stand-ins: spanners sized to the head, and a crank handle for the
  starting shaft.
- Turns to free a fastener are a guess: one diameter of thread at the DIN coarse pitch of the
  guessed size. Counts marked "not verified" in a step have no source.

Sequences built so far:

| Id | Row | Seconds | Steps | What it shows |
|---|---|---|---|---|
| `starter_hand_crank` | E9 | 16.8 | 14 | cover bolt out, cover off, shaft from clips into the port, handle on, wind, trip, all back |
| `jack_off_the_plate` | F3 | 24.3 | 26 | strap open, four holder bolts out, jack to the ground, wind, back on, bolts in, strap shut |
| `engine_hatch_open` | B7 | 8.8 | 8 | both bolts drawn, bar up, lid up to 35 degrees, held, down, bolts home |
| `fan_grille_off_L`, `_R` | B7 | 18.2 | 17 | two wing nuts off, bar up, grille onto the hatch, plate out, bay open, all back |
| `deck_cap_FL` | B7 | 5.4 | 5 | cap unscrewed, set down, filler open, cap back |
| `track_link_replace_L`, `_R` | A1 | 16.2 | 15 | drift and hammer on both pins from between the tracks, broken link out, spare in, pins driven home from outside |
| `spare_link_off` | A2 | 6.4 | 4 | spare link off the turret side, onto the deck, back |
| `road_wheel_off_L3` | A3 | 33.5 | 38 | arm jacked, six nuts off, outer disc drawn and laid down, back on, nuts on, jack down |
| `arm_chained_up_L3` | A4 | 4.3 | 4 | arm jacked to 20 degrees and held (chain not modelled) |
| `sprocket_cap_off_L` | A5 | 32.9 | 44 | eight screws out, hub cap off, back on |
| `track_tension_L` | A6 | 3.2 | 3 | track wrench on the adjuster, turned, idler moves rearward |
| `tool_off_hammer`, `_crowbar`, `_axe`, `_shovel`, `_wire_cutters` | F1 | 5.7 to 7.2 | 5 to 7 | clamps open, tool lifted and held, back in, clamps shut |
| `extinguisher_off` | E10 | 3.6 | 3 | bottle off its bracket, held, back |
| `tow_cable_eyes_off_L` | F2 | 3.8 | 3 | both eyes lifted off their posts and put back (rigid cable) |
| `vision_block_swap_cupola`, `driver_visor_block_swap` | B3, B2 | 4.0 | 3 | block pulled inward, slot empty, spare seated |
| `hull_mg_out`, `coax_mg_out`, `aa_mg_off` | B5, D7, D8 | 4.0 to 4.8 | 3 | gun drawn out, held, put back |
| `headlamp_off`, `mudguard_off_rear_L`, `ventilator_cowl_off` | B6, A8, C7 | 4.0 to 4.8 | 3 | part off, held, back |
| `gunner_sight_swap` | D6 | 4.8 | 3 | telescope drawn rearward, mount empty, spare seated |
| `round_from_rack` | D9 | 4.4 | 3 | a round drawn from a side rack and put back |
| `carburettor_off`, `air_filter_off` | E1, E5 | 5.2 | 3 | engine hatch held open, part lifted out of the bay and back |
| `aerial_swap` | E11 | 4.2 | 4 | rod unscrewed, lifted out, spare screwed in |
| `bin_lid_open_L` | C9 | 3.0 | 3 | lid lifted, held, shut |
| `breech_cleared` | D4 | 4.5 | 5 | block up and stuck, lever heaved, block drops, chamber cleared, closed |
| `gun_back_into_battery` | D2 | 4.2 | 4 | gun 0.5 m back, heaved forward with the crowbar, home, mount bolts |
| `hand_traverse`, `hand_elevate` | C3 | 1.7 | 2 | one turn of the handwheel as a cycle, then laid |
| `turret_ring_freed` | C1 | 2.4 | 2 | hammer blows at the ring gap, then the turret turns |

39 sequences in all. The many three-step "out, held, in" sequences are honest minimums: both
hands on the part, the part's real path out of its seat, and back. They do not show clamps or
fixings that the kit does not model.

For the game there are two files beside the rig file: `tiger_gen2_repairs.json` (the sampled
sequences) and `tiger_gen2_rig.json` (`tools/tiger_production/rig_export.py`: every control with
its frame, axis, limits, notes and parts; every fastener with its seat, direction, set and head;
the track and cable guide curves).

## Planned: every other row

"Cuts" are parts to split first. "Sets" are fastener sets to read from the CAD. Sequence is the
repair as ordered rig actions.

### Tracks and running gear

Built, see above, rows A1 to A7. Still open: the sprocket's tooth ring as its own part (A5), a
tyre-less wheel variant and broken disc pieces (A3), the adjuster cap cut from the rear plate (A6),
mudguards and track guards (A8, with the hull rows).

### Hull, turret, armament, engine and drivetrain

Built as first readings, see above. Still open: everything listed there as not rigged; the engine
as a removal tree with its lines and mounts (E1); ring bolts as a fastener set (C1); mount bolts
(D2); a parting of each MG into gun and barrel (B5, D7); checking each identification in the
viewport or against a drawing.

### Tools and stowage

F1, F2 and F3 built, see above. Still open: the cables as real ropes with eyes that reach a tow
point (needs the game's rope), posts and cable clamps that open, track tools and cleaning rods
(not in the kit).

## Order of work

Every row now has a first rig and, where there is a hand job, a sequence. What follows is the order
for the open items.

1. Rear plate, done except the items listed as open.
2. Engine deck and its locks (B7), done except the louvred grilles.
3. Tracks and wheels (A1 to A6), which also need the jack and the tensioner.
4. Tow cables as ropes (F2), then pioneer tools (F1).
5. Turret and gun controls (C2, C3, D2 to D7).
6. Engine removal tree (E1 to E5), driver's controls (E6 to E8).
7. The rivet-or-screw decision for the round heads, part by part, from the census.

The game reads none of this yet. The export has to carry the controls and the fastener objects
(`tools/tiger_production/export_pivots.py` writes every non-mesh object with its properties today;
fasteners are meshes and need their own table).
