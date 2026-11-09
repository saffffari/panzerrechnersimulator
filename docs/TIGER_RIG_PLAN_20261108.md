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

## Planned: every other row

"Cuts" are parts to split first. "Sets" are fastener sets to read from the CAD. Sequence is the
repair as ordered rig actions.

### Tracks and running gear

| Row | Controls | Cuts and sets | Repair sequence in rig terms |
|---|---|---|---|
| A1 track runs | a guide curve per side; each link a stowed part on it; `Track_Pin` slide per joint | pins are already separate (`37N01`) | drive one pin out (slide), lift the broken link (stow), seat a spare, drive the pin in |
| A2 spare links | each link stowed; bracket latch hinge | bracket from link | open the bracket, lift the link |
| A3 road wheels | `Wheel_<station>_<n>` stowed; hub nuts as a set | wheel discs from hubs where fused; tyre ring from disc | jack the arm (A4 hinge), undo the hub set, pull the outer wheel |
| A4 swing arms | `Arm_<station>` hinge about the torsion bar axis, limits from the bump stops | none expected | chain the arm up: hinge to its upper limit and hold |
| A5 sprockets | `Sprocket_L/R` hinge, continuous; tooth ring stowed | tooth ring from hub; ring bolts as a set | undo the ring set, turn the ring, bolt up |
| A6 idlers, tensioners | `Idler_L/R` hinge on the crank arm; `Tensioner_L/R` socket for the track wrench | find the adjusters first | cap off (stow), wrench in the socket, turns move the idler arm |
| A7 final drives | housing covers stowed; bolt circle as a set | none | none in the field |
| A8 guards | each panel stowed; a bend hinge at its root | panels where fused | lever clear (hinge) or tear off (stow) |

### Hull

| Row | Controls | Cuts and sets | Repair sequence in rig terms |
|---|---|---|---|
| B1 armour | none (FEA owns it) | plates must be watertight solids | none |
| B2 driver's visor | `Visor_Shutter` slide; vision block stowed | block from its frame | pull the block inward, seat a spare |
| B3 vision blocks | each block stowed inward; retaining clamp hinge | block from housing | open the clamp, pull, seat |
| B4 hull hatches | done | lock arms as their own hinges | free the lock arm, swing the lid |
| B5 hull MG | `Ball_MG` two-axis mount; gun stowed inward | gun from ball | release, pull the gun in, seat another |
| B6 headlamp | lamp stowed; socket | lamp from its foot | lift off |
| B7 engine deck | built, see above; louvred radiator grilles still part of the roof | cut the louvre panels out of `33B01` | as animated |
| B8 rear plate | built, see above; exhaust shrouds and hooks still to do | shroud bolts as sets | lift fittings off |

### Turret

| Row | Controls | Cuts and sets | Repair sequence in rig terms |
|---|---|---|---|
| C1 turret ring | traverse hinge, continuous; ring bolts as a set | bolt heads along the ring | knock the fragment out; bolts out and in |
| C2 powered traverse | `Traverse_Pedal` hinge | pedal from its box | rock the pedal; tap the coupling |
| C3 hand gear | `Traverse_Handwheel`, `Elevation_Handwheel` hinges, continuous; elevation arc | wheels from gearboxes | turn; shear pin as a fastener |
| C4 to C6 hatches | done | latch arms as their own hinges | as now |
| C7 ventilator | fan hinge, continuous; cowl stowed | fan from housing | lift the cowl |
| C8 basket | floor plate bend hinge; seats stowed with a fold hinge | seats from posts | lever the floor |
| C9 stowage bin | `Bin_Lid` hinge; discharger tubes stowed | lid from bin | open, lift |

### Armament

| Row | Controls | Cuts and sets | Repair sequence in rig terms |
|---|---|---|---|
| D1 barrel | recoil slide; muzzle brake stowed | brake from tube | none |
| D2 mantlet | mantlet on the elevation hinge, not on the recoil slide; mount bolts as a set | bolt heads on the mount | jack into battery (slide), fit the set |
| D3 recoil gear | filler caps as fasteners; cylinders stowed | caps from cylinders | cap out, top up, cap in |
| D4 breech | `Breech_Block` slide; `Breech_Lever` hinge; `Safety` hinge | block from ring | lever down, clear, lever up |
| D5 cradle | elevation hinge; `Travel_Lock` hinge | lock from its bracket | free the lock |
| D6 gunner's sight | sight stowed inward; clamp hinge | sight from its mount | open the clamp, pull, seat the spare |
| D7 coax MG | gun stowed from its cradle; feed cover hinge | gun from cradle | lift out, seat another |
| D8 AA MG | mount hinge on the ring; gun stowed | gun from mount | lift off |
| D9 ammunition | each round stowed; rack clamp hinges | rounds from racks (already separate) | open the clamp, pull the round |

### Engine and drivetrain

| Row | Controls | Cuts and sets | Repair sequence in rig terms |
|---|---|---|---|
| E1 engine | a removal tree of stowed parts; fuel line and plug leads as sockets | carburettors, pipes, leads from the block | undo sets in order, lift |
| E2 fuel tanks | filler caps as fasteners; tanks stowed | caps from tanks | cap out; bung in the hole |
| E3 radiators | cores stowed upward; filler caps | cores from boxes | grille off (B7), lift the core |
| E4 fans | fan hinges, continuous; drive coupling slide | fan from cover | slide the coupling out |
| E5 air filters | bowls stowed; clamp hinges | bowl from body | open clamps, drop the bowl |
| E6 gearbox | selector hinge; emergency levers hinges | levers from the box | work the emergency levers |
| E7 steering | wheel hinge; two emergency lever hinges | wheel from column | pull the levers |
| E8 driver's controls | one hinge or slide per pedal, lever and wheel, limits swept | each from its bracket | straighten: none in the rig |
| E9 starting gear | built, see above | crank throw to be made | cover off, shaft in, turn |
| E10 extinguishers | bottle stowed; bracket strap hinge | bottle from bracket | open the strap, swap |
| E11 radio, aerial | aerial rod stowed in a socket; set knobs as hinges | rod from base | unscrew the rod, screw in a spare |

### Tools and stowage

| Row | Controls | Cuts and sets | Repair sequence in rig terms |
|---|---|---|---|
| F1 pioneer tools | each tool stowed; each bracket strap a hinge | tool from its brackets | open the straps, lift |
| F2 tow cables | each cable a rope along its guide curve with an eye at each end; eyes are stowed parts with sockets on the shackles | rebuild the cables from the guides | lift an eye off its hook, carry it, seat it on a shackle |
| F3 jack | built, see above; the rack still to move with the crank | rack from body, holder clamps | free the strap and clamps, carry, wind |
| F4 track tools | stowed, with strap hinges | find them first | lift |
| F5 cleaning rods | stowed, with strap hinges | find them first | lift |

## Order of work

1. Rear plate, done except the items listed as open.
2. Engine deck and its locks (B7), done except the louvred grilles.
3. Tracks and wheels (A1 to A6), which also need the jack and the tensioner.
4. Tow cables as ropes (F2), then pioneer tools (F1).
5. Turret and gun controls (C2, C3, D2 to D7).
6. Engine removal tree (E1 to E5), driver's controls (E6 to E8).
7. A model-wide fastener census on the full CAD and a rivet-or-screw decision per part.

The game reads none of this yet. The export has to carry the controls and the fastener objects
(`tools/tiger_production/export_pivots.py` writes every non-mesh object with its properties today;
fasteners are meshes and need their own table).
