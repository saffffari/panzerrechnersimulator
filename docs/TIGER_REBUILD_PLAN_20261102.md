# Tiger rebuild from the kit CAD — plan, 2 November 2026

The owner's direction: the game Tiger is eventually rebuilt from the Rye Field Model RM-5010
full-interior kit CAD (`TG_RM5010-0A_Tiger I`), maintained in Blender as the production master,
rigged, named for FEA, with real materials, and baked to the game from there. This page is the
plan. Nothing in it is built yet beyond the identification pass of 2 November.

## Where things stand

| File | What it is | Use |
|---|---|---|
| `assets/tanks/tiger/Panzerkampfwagen_VI_Tiger_Ausf.E.blend` | The hand-edited game master (July), an import of the same kit CAD with generic names (`Hull_0619`), 4.1 M triangles | Bake source of every game GLB |
| `/windows/shop/ts_assets/cad_tiger_i_rm5010_complete.blend` | The 28 October conversion of the kit STEP: 1,481 named parts in 245 assembly empties, 31 M triangles | The rebuild's source |
| `assets/tanks/tiger/tiger_production.blend` | The game master with kit names, system collections, material colours and FEA tags (2 Nov) | Interim; superseded by the rebuild |
| `docs/fea/tiger_parts.csv`, `catalog/tiger_parts.v1.json` | Per-part kit identity, material, FEA group, volume, mass | Carries into the rebuild unchanged |

The two models share a frame up to scale: game = 38.905 × kit + (−0.004, −2.637, 0.467) m, no
rotation. 1,433 game parts match a kit part one to one (centroid within 1 cm, volume within 5 %),
64 are pieces of a kit part, 21 are authored in the game master. The kit has 38 parts the game
master lacks (crew seats, some covers and arms, the turret-floor fuel cans, a gasket).

## Scale: the kit is true to the vehicle at 1:35; the game Tiger is 11 % too large

Measured live in `tiger_gen2_live.blend` (the kit conversion x35, ground at z = 0), against the
Tiger I Ausf. E's published dimensions:

| Dimension | Historical | Kit x35 | Game Tiger (x38.905) |
|---|---|---|---|
| Length with gun | 8.45 m | 8.47 m | 9.42 m |
| Width over combat tracks | 3.705 m | 3.71 m | 4.12 m |
| Road wheel diameter | 800 mm | 797-800 mm (vertex radius) | 889 mm |
| Combat track width | 725 mm | 730 mm | 812 mm |
| Belly clearance | 0.47 m | 0.454 m | 0.51 m |
| Track centres | ~2.82 m | 2.78 m | 3.09 m |
| Height to cupola | ~3.0 m | ~3.0 m (open hatch and rear-deck ring 3.13-3.27) | ~3.4 m |

So gen2 is built at x35. Moving the game to it changes the Tiger's size everywhere the runtime uses
it (`TIGER_PARTS`, collision, suspension, crew eyes, interior apertures, workshop catalogs), so the
switch is its own step with its own regression.

## What the kit model gives and what it lacks

Gives: every part, named by kit number and part (`28U05_MAIN` = engine block), grouped by kit
assembly (Base Unit, Engine System, Front Armor, Gun Units, Track Unit, Rollor Systems); all three
MG 34s (hull, coax, cupola ring mount `0C09`); exact kit geometry.

Lacks:
- **Materials.** Its 26 materials are SolidWorks display colours (white plates, black track,
  magenta glass), not physical materials. Real materials come from the part classification.
- **A runtime tessellation.** 31 M triangles; re-tessellate from the STEP at a game chord
  tolerance (FreeCAD, as `tools/bake_tiger_cad_components.py` already does) rather than decimate.
- **Rig.** No pivots, hatches, recoil, cupola, TPZ, levers, lights.
- **Real wall thicknesses.** A plastic kit models castings and boxes solid and thin parts thick
  for moulding: the summed mass is 118.5 t against the real 57 t. Hollow parts (engine block,
  road wheels, radiators, boxes) need shells before FEA mass is right.

## Steps

1. **Rig transfer.** Copy every pivot, parent chain and custom property of the game master onto
   the kit parts through the 1,433 exact matches; bring the 21 game-authored parts across as they
   are. Keep each part's game node name as `game_node` so the engine runs unchanged.
2. **Names.** `SYSTEM.Part_kitnumber`, with an English part glossary (the SolidWorks zip carries
   English and Chinese names per part; `tools/tiger_cad_register.py --stage sw` reads them).
3. **Mechanisms.** A pivot or controller for every hatch, latch, panel, lever, the cupola MG ring
   mount, the engine deck covers, the gun's elevation and recoil.
4. **Engine and drivetrain.** An assembly tree deep enough to disassemble and reassemble the
   engine, cooling, fuel and transmission, with fastener order, and replace panels.
5. **Materials.** Real Blender materials per physical class, then the engine's textures.
6. **FEA.** Armour bodies with measured thickness per region against the nominal table; hollow
   shells for solid kit parts; the parts sheet's material values sourced.
7. **Bake.** A production bake that writes the game node names, then a runtime migration to
   `fea_id`. Regression: bridge and camp checks, interior stills, collision and shell tests, the
   workshop catalogs (`removed_roots`, `native_hull_*`).

## The cupola MG meanwhile

It is in the game master under `cupola_gun_pivot` and in the kit as `0C09`. The bake excludes it
(`tools/bake_tiger_geo.py`, owner 11 July: it blocked the forward periscope). It comes back with
the rebuild as a traversing ring mount, parked clear of the vision blocks.
