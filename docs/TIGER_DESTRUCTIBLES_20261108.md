# Tiger gen2: destructible components and their repairs (first pass, 8 November 2026)

Source: `tiger_gen2_live.blend` (the RM-5010 kit CAD at true size: 1,481 mesh objects in 19 kit
assemblies, 31.1 million triangles) and `docs/fea/tiger_parts.csv` (kit number, physical material
and FEA group of every part). Each row is a thing that can break in play and needs a repair
process, a rig state, or a decision that it has none.

How to read this:

- **Kit parts** are kit numbers as they appear in the object names (`TG_RM5010-<number>_<NAME>`).
  The kit's own spellings are kept (ROLLOR, BLACKET, BROCK).
- **In game** names what exists today in `src/tiger_damage.rs` (`Fault`, `Spare`) or elsewhere.
  "none" means the damage and its repair are both still to build.
- **Rig prep** is what the Blender master needs before the game can show the break and the repair.
- Repairs marked **(concession)** were workshop jobs in service; the owner's rule of 2 November is
  that every fault has a field technique, so a hit is not a trip to the workshop.
- Not verified: I worked from part names, assemblies and the parts sheet. Rows marked **(confirm)**
  name a part I have not yet looked at in the viewport.

## A. Tracks and running gear

| # | Component | Kit parts | How it breaks | Effect in play | Field repair | In game | Rig prep |
|---|---|---|---|---|---|---|---|
| A1 | Track runs, 2 x 96 links | `0B28` x192 (TRACK, PIN, 2 PLATE each) | link or pin broken by a hit or a mine; track thrown in a hard turn | that side has no drive; the hull slews | drag the run back over the wheels, fit a spare link, hammer the pin | track repair ritual, `Spare::TrackLink` | links as a chain along a curve; a break point at any pin; loose-run pose |
| A2 | Spare links, turret and bow | `0C10` TRACK x7, BROCK x14 | shot off the brackets | fewer spares | none (pick links up from the ground or a wreck) | count only | each link and bracket a separate removable part |
| A3 | Road wheels, 16 stations x 3 | `0B17` x8, `0B18` x8 (ROLLOR x3, WASHER) | disc shattered, rubber tyre burnt or torn off | rough ride, slower, higher chance of throwing the track | jack the arm, unbolt and drop the wrecked outer wheel, run without it | none | per-wheel removal; a tyre-less variant; broken disc pieces |
| A4 | Swing arms and torsion bars | `0B17/0B18` ROLLAR (arm) | arm bent, torsion bar snapped | the station sags, the track rides on the next wheels | chain the arm up clear of the track (concession) | none | arm rotation about the bar axis; sagged pose |
| A5 | Drive sprockets | `0B19` x2 (ROLLAR x2, PIN, RING) | tooth ring cracked or teeth sheared | track slips or jumps under load | unbolt the tooth ring and turn it to unworn teeth (concession) | none | tooth ring separate from hub; rotation axis |
| A6 | Idlers and track tensioners | `0B20` x2 (GUIDE x2, RING); adjuster in `0B24` (SCREW, TURN) | idler cracked, adjuster bent | slack track, thrown track | re-tension at the rear plate with the track wrench | none | idler crank arm travel; adjuster screw as a turned control |
| A7 | Final drives | `18-22`, `18-23` COVER, `29D56` DRIVER | housing cracked, gear stripped | no drive on that side at all | none in the field: tow (see F2) | none | housing separate; oil-leak anchor points |
| A8 | Mudguards and track guards | PLATE parts along the hull sides, `0B16`/`0B26` **(confirm)** | bent into the track, torn off | scraping, a guard fouling the track | lever it clear or tear it off with the crowbar | none | per-panel bend and detach |

## B. Hull, armour and openings

| # | Component | Kit parts | How it breaks | Effect in play | Field repair | In game | Rig prep |
|---|---|---|---|---|---|---|---|
| B1 | Armour plates, 11 groups | `36F03`, `28B-B/K`, `16LH0`, `30Z02`, `33B01`, `29Z29/30`, `01A03`, `04S01`, `12Z01`, `10S03` | gouge, scoop, crack, hole, sprung weld | permanent weak point; spall inside | none; the scar stays | FEA voxels and scars | watertight solids per plate, weld seams as edges |
| B2 | Driver's visor and its block | in `36F03`; GLASS, block **(confirm)** | glass crazed or shattered, visor shutter jammed | driver blind ahead except by periscope | fit a spare vision block; free the shutter with the hammer | `Spare::VisionBlock` | shutter slide; removable block |
| B3 | Vision blocks, all posts | GLASS x11 (`0C13` x8, `0B26` x2, `0B25` x1) | cracked, shattered | that view dead | pull the block, fit a spare | `Spare::VisionBlock` | each block removable inward; cracked variant |
| B4 | Driver and radio operator hatches | `0B26` (DOOR x2, ARM x4, LOCK, LOCKARM) | hinge knuckle or lock arm jammed | cannot open or cannot shut | hammer and crowbar from outside | rigged; fault none | done: `CTRL.Hatch_Driver`, `CTRL.Hatch_RadioOperator` |
| B5 | Hull MG and ball mount | `28B-K` GUN, MG 34 parts x16 in `0B25` | barrel bent, gun wrecked, ball jammed | no bow MG | barrel change; swap in the cupola MG | `MgBarrel`, `MgWrecked`, `Spare::AaMg` | ball pivot; gun removable inward |
| B6 | Headlamp | `0B25` LIGHT | shot off or smashed | no light for night driving | none (drive dark) or refit from a wreck | none | lamp removable; socket |
| B7 | Engine deck: hatch, grilles, locks | `0B26` COVER x12, FAN COVER x4, LOCK x19, BUCKLE x3 | grille holed or blocked, hatch jammed, burning fuel through the grilles | overheating, engine-bay fire, no access for repairs | clear the grilles; free the hatch with the crowbar | none | deck hatch hinges, grille panels removable, each lock a control |
| B8 | Rear plate fittings | `0B24` (44 parts): EXHAUST x2 and shrouds, HOOK x4, HOLDER x4, CHAIN, jack `0B30` | shrouds and pipes shot away, fittings torn off | exhaust glow and flame seen at night; lost jack or hooks block other repairs | none for the exhaust; refit tools from a wreck | none | each fitting detachable; exhaust without shroud variant |

## C. Turret

| # | Component | Kit parts | How it breaks | Effect in play | Field repair | In game | Rig prep |
|---|---|---|---|---|---|---|---|
| C1 | Turret ring | ring gap between `10S03` and `33B01` | fragment wedged in the gap; bolts or bearing wrecked | no traverse | knock the fragment out from outside; pack the bearing and bolt down with spare ring bolts (concession) | `RingJammed`, `RingWrecked`, `Spare::RingBolts` | traverse axis; bolt heads as parts along the seam |
| C2 | Powered traverse | `05U13` MOTOR, `05J27` gearbox, `05J26` PEDLAL | hydraulic fault, pedal linkage bent | hand traverse only | "Kupplung abklopfen"; straighten the linkage | `HydraulicFault` | pedal rock; drive shaft from the hull |
| C3 | Hand traverse and elevation | `05J55` ROLLOR (two handwheels), `06J56` elevation gearbox | gear stripped or jammed | gun stuck in azimuth or elevation | free with the hammer; fit a shear pin (concession) | none | handwheel rotation; elevation arc and gearbox |
| C4 | Cupola and its hatch | `0C12` (ARM x2, HOLDER x2, PULL, COVER), blocks in `0C13` | spindle bent, crank jammed, blocks cracked | commander cannot open up or cannot close | straighten or free the spindle, grease it; spare blocks | rigged; fault none | done: `CTRL.Cupola_Lift`, `CTRL.Cupola_Swing` |
| C5 | Loader's hatch | `0C11` (ARM x3, ROLLAR, PLATE, COVER) | hinge or latch jammed | loader's exit and the empties' way out lost | hammer and crowbar | rigged; fault none | done: `CTRL.Hatch_Loader`; roof part `12Z57` sits in the swing arc (open question) |
| C6 | Escape hatch | `0C14` (BLACKET x2, PLATE, COVER) | lid jammed or blocked by stowage | no side exit | free the lock spindle | rigged; fault none | done: `CTRL.Hatch_Escape` |
| C7 | Roof ventilator | `0C13` FILTER, COLLAR | fan destroyed, cowl crushed | gun fumes stay in the turret | clear the cowl; none for the fan | fumes model exists, fault none | fan rotation; cowl detach |
| C8 | Basket floor and seats | `05U25` BASE, `25H10/11` PLATE, SEAT x8 in `0C05` | floor bent against the hull, seat torn off | traverse binds; a crewman has no post | lever the floor clear with the crowbar | none | basket parented to the turret; floor plate bend pose |
| C9 | Turret stowage and smoke dischargers | bin and fittings in `0C10` **(confirm)** | bin holed, dischargers set off or shot away | kit lost; smoke around the tank at the wrong moment | none | none | bin lid hinge; discharger tubes separate |

## D. Armament

| # | Component | Kit parts | How it breaks | Effect in play | Field repair | In game | Rig prep |
|---|---|---|---|---|---|---|---|
| D1 | 8.8 cm barrel and muzzle brake | `0C01` BARREL x5, COUPLING | tube gouged, holed or bent; brake shot off | firing risks a burst; without the brake the recoil over-travels | none for the tube; decide whether a damaged gun may fire | none | muzzle brake separate; gouged and bent variants |
| D2 | Mantlet and gun mount | `01A03` SHIELD | mount bolts sheared | gun held in full recoil | jack it back into battery, fit spare mount bolts (concession) | `MountSheared`, `Spare::MountBolts` | mantlet on its own non-recoiling frame (today it rides the recoiling gun, an open bug) |
| D3 | Recoil brake and recuperator | `02J50`, `02J51` OIL BOTTLE | cylinder holed, oil lost | the oil safety cuts the firing circuit | plug and top up the 5.1 l | `RecoilLeak` | cylinders separate; filler cap as a control |
| D4 | Breech and firing gear | `02U03` ROOM, `02J54` BLOCK | block jammed, electric firing dead | no shot until cleared | clear the block; fire by the emergency circuit | breech states exist, fault none | block drop, lever, safety as controls |
| D5 | Gun cradle and elevation arc | cradle group (35 parts), `02J07` BASE | trunnion or cradle hit | elevation stiff or lost | none beyond C3 | none | trunnion axis, recoil slide, travel lock |
| D6 | Gunner's sight | `01J18` VIEWER | knocked out of true; optics smashed | reticle lies; or no reticle | boresight through the bore; fit the spare telescope | `SightOutOfTrue`, `SightSmashed`, `Spare::Telescope` | sight removable; aperture in the mantlet |
| D7 | Coaxial MG 34 | `03J19` GUN, MG parts x16 in `0C03` | barrel bent, gun wrecked | no coax | barrel change; swap in the cupola MG | `MgBarrel`, `MgWrecked` | gun removable from its cradle |
| D8 | Cupola AA MG and mount | `0C09` (GUN x2, HOLDER x3, CASE) | shot off the ring | no AA gun, no spare gun for D7 and B5 | none | `Spare::AaMg` | mount on the cupola ring; gun detachable |
| D9 | Ammunition and racks | 4 sponson racks (`25M06` BLACET x16, AMMO x64), floor rounds (`23M03`, `21M08`), 90 in all | rack bent, case holed, propellant lit | rounds jammed or unusable; fire and cook-off | lever jammed rounds free; throw damaged rounds out | cook-off and case heat exist; rack fault none | each round its own part (they are, at 95,392 triangles each: reduce first); rack clamps as controls |

## E. Engine bay and drivetrain

| # | Component | Kit parts | How it breaks | Effect in play | Field repair | In game | Rig prep |
|---|---|---|---|---|---|---|---|
| E1 | Engine | `0B29` (14 parts): `28U05` MAIN, CARBURETOR x2, PIPE x2 | block holed, fuel line cut, ignition dead, seized | power loss, stall, fire | patch the line, change plugs, clear the carburettor; a seized engine is a tow | engine operability and workshop removal exist | the engine disassembly tree with a removal path |
| E2 | Fuel tanks, left and right | `27H12` BOX x2, `28H21/22` COVER | holed | leak, fire, short range | drive a wooden bung into the hole; pump fuel across | fire exists, leak none | tanks separate; filler caps |
| E3 | Radiators, left and right | `27K76`, `27K77` COOLBOX | core holed | coolant loss, overheating | plug the core, top up with water | `engine_cooling.rs` | cores removable upward through the deck |
| E4 | Cooling fans, left and right | `27H13` COVER, `27H25` PLATE, FAN COVER x4 | blades bent, drive jammed | that side overheats | free the fan, or disconnect its drive | none | fan rotation; drive coupling |
| E5 | Air filters | `28U20` FILTER TANK x2 | holed or choked | power loss, engine wear | clean, or bypass | none | filter bowls removable |
| E6 | Gearbox | `21K49` BOX | selector fault, oil lost | stuck in one gear | emergency manual gear selection | powertrain model exists, fault none | selector lever, emergency levers as controls |
| E7 | Steering unit | `21K51`, `21K54` BOX, `21K59` COVER | hydraulic oil lost | wheel steering dead | steer on the emergency brake levers; top up oil | none | steering wheel, the two emergency levers |
| E8 | Driver's controls | WHELL x2, PEDAL x3, HAND x4 in `0B16`/`0C05` | linkage bent or shot through | a control goes dead | straighten the rod | none | each control with its travel |
| E9 | Starting gear | electric starter, inertia starter and its hand crank through the rear plate **(confirm in `0B24`: TURN, SCREW)** | battery flat, starter burnt out | no electric start | two men on the crank at the rear | none | crank port cover, crank as a turned control (the owner's startup sequence) |
| E10 | Fire extinguishers | fixed engine-bay system **(confirm)**, hand extinguisher `34B67` JAR | bottle spent or holed | the next fire burns | swap the hand bottle; none for the fixed system | fire and extinguish exist | bottle removable from its bracket |
| E11 | Radio and aerial | `21K57` BOX, ANTENNA | set shocked, aerial shot off | no net | re-seat plugs and fuses; screw in a spare rod | `RadioShock` | aerial rod detachable; set face controls |

## F. Tools and recovery kit (each one gates a repair above)

| # | Component | Kit parts | How it is lost | What stops working | Recovery | In game | Rig prep |
|---|---|---|---|---|---|---|---|
| F1 | Pioneer tools | AXE, SHOVEL x3, TOOL (crowbar, hammer, wire cutters **(confirm which)**) | shot off, burnt, left behind | every crowbar and hammer repair | take one from a wreck or the depot | toolbox and hammer exist | each tool a pickup with its bracket |
| F2 | Tow cables and shackles | WIRE x3 (`35C45CM-L/R`, `36Z53`), HOOK x7, Wire Hook x4 | cut by fire, torn off | no towing, no self-recovery | splice or replace from another tank | recovery rig exists in part | cables as rope with end eyes (6.6 million triangles today: rebuild as a curve) |
| F3 | Jack and jack block | `0B30` JACK Unit (5 parts) | shot off the rear plate | wheel and track jobs (A3, A4) | from a wreck or the depot | none | jack as a working screw control |
| F4 | Track tools | track wrench, pin drift, cable for pulling the run **(confirm)** | lost | A1, A6 | from a wreck | track cable exists | pickups |
| F5 | Gun cleaning rods and spare barrel box | on the hull side **(confirm)** | shot away | MG barrel changes | none | none | removable |

## Counts

50 rows.

- Field repair already in the game: 11 (A1, B2, B3, B5, C1, C2, D2, D3, D6, D7, E11).
- System partly modelled, no fault or repair for it: 14 (A2, B1, C7, D4, D8, D9, E1, E2, E3, E6,
  E10, F1, F2, F4).
- Rigged in Blender, no fault: 4 rows, five hatches (B4, C4, C5, C6).
- Nothing built: 21 (A3 to A8, B6 to B8, C3, C8, C9, D1, D5, E4, E5, E7, E8, E9, F3, F5).

## What this needs from the model before any of it can be built

1. **Topology.** 31.1 million triangles, against a game budget in the low hundreds of thousands.
   The worst: ammunition 7.0 million (90 rounds at 95,392 each), cables 6.6 million, tracks 4.3
   million, the hull tub and its fittings 9.9 million. The engine does not turn meshes into
   distance fields: it rasterises triangles and builds triangle ray-tracing structures from
   them, so every triangle costs.
2. **Materials.** The live file still carries the CAD's 26 colour slots (`mat_0` to `mat_25`).
   The 38-material library exists (`tiger_materials.blend`, `tools/tiger_production/materials.py`)
   and has not been applied to this file.
3. **Structure.** Everything sits flat in the scene collection under the kit's assembly empties.
   Each row above wants its own collection, a pivot or removal path, and a damaged state.
