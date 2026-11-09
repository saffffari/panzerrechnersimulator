# Bringing the gen2 Tiger into the game at true size (9 November 2026)

The owner's decision on 9 November: the new Tiger comes in at its true size, not scaled up to
match the present one. This note records what that touches and the order of work.

## What was measured

Bounding boxes of the models the game loads today, against published dimensions. Width is the
clean comparison (lengths of the wreck models include their gun barrels).

| Vehicle | Game width (m) | Real width (m) | Ratio | Use in game |
|---|---|---|---|---|
| Tiger I (present) | 4.12 | 3.705 | 1.11 | player's tank |
| T-34/76 | 3.57 | 3.00 | 1.19 | enemy tank |
| T-34-85 wreck | 3.57 | 3.00 | 1.19 | scenery |
| Pz III wreck | 3.95 | 2.95 | 1.34 | scenery |
| IS-2 wreck | 3.31 | 3.09 | 1.07 | scenery |
| Tiger II wreck | 4.01 | 3.755 | 1.07 | scenery |
| Panther wreck | 3.54 | 3.42 | 1.04 | scenery |
| KV-1 wreck | 3.25 | 3.32 | 0.98 | scenery |
| **gen2 Tiger export** | **3.715** | 3.705 | 1.00 | new |

So the two playable tanks are both too large, and by different amounts: today a T-34 is 87 % as
wide as the Tiger where it should be 81 %. A true-size Tiger beside the present T-34 would be
only 4 % wider than it. **True size therefore means the T-34 too** (scale 0.84), or the pair
looks wrong in the other direction. Real widths are from memory of standard references and
should be checked before the T-34 is rescaled.

## What exists now

`tools/tiger_production/export_gen2_glb.py`, run headless on the rig file, writes into
`assets/tanks/tiger/gen2/`:

- `tiger_gen2.glb`, 25.6 MB: 2,489 meshes, 1.79 M triangles (the present Tiger is 4.13 M), 301
  rig controls and 925 fasteners as named nodes in the rest pose, 34 flat-colour materials, each
  control's kind, limits and notes as node extras. Width 3.715 m, length with gun 8.48 m.
- `tiger_gen2_tools.glb`: the repair tools and hands.
- `tiger_gen2_export.json`: counts, bounds, the frames, and the fit to the present game Tiger.

Axes match the present Tiger: +Y up, +Z forward, +X the tank's left. The rig and repair JSON
files (`tiger_gen2_rig.json`, `tiger_gen2_repairs.json`) stay in the rig file's frame; a point
(x, y, z) there is (x, z, -y) in the glb.

## Order of work

1. **Garage.** A new scene that loads the gen2 Tiger at true size with its controls, fasteners
   and repair sequences, in a night-time industrial garage built from the pizza assets under
   overhead lamps. New code only; the playable Tiger is untouched. This is where the rig, the
   repair player and the hands-first animation get proven.
2. **World scale.** Rescale the T-34 (models, gun rig offsets, collision, crew positions) and
   check what else assumes the inflated size: soldier and crew heights (already real at 1.75 m),
   buildings, hatch apertures. The trained tank policies see ranges and bearings, not sizes, but
   hit geometry changes, so their gauntlets are rerun before anything becomes the default.
3. **The playable Tiger.** 54 source files name the present baked Tiger or its mesh nodes
   (`tiger_exterior_finishes.rs` alone 140 times; `cabin_medium.rs`, `tank.rs`, the visor,
   periscope, optics, refuel and interior-rig modules). Each is moved to the gen2 part names:
   cabin cameras and crew stations, hatches, interior rig and racks, cabin heat and fire
   surfaces, collision and track contact, the driver's panel and dial. The armour model already
   works at real size (`game_fea.rs` corrects for the 1.112 drawing scale), so that correction is
   removed rather than added.
4. **Close the debt.** The "Visual-size debt" entry in the physical truth audit is retired when
   the last module is over.

Steps 2 and 3 change how the picture reads, so each gets before and after stills and becomes the
default only on the owner's yes.

## Open

- Whether the scenery wrecks are rescaled individually (their ratios run from 0.98 to 1.34).
- The gun elevation and recoil limits in the rig are from memory, not checked against a source.
- 1,670 round heads are still moulded into their plates; rivet or screw is undecided per part.
