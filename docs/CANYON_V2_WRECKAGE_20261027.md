# Canyon v2 wreckage layer — 27 October 2026

The owner's note of 27 October: the maps read dull and empty; they want far more destroyed armies
and equipment. This pass adds a dense, sourced aftermath layer to the Canyon v2 winter workshop
(`assets/worldgen/workshops/gaea-canyon-run-v2`) without touching the generated base manifest, so
the generator's hand-edit guard, the Gaea source and the v1 outputs stay exactly as they were.

## What was added

`tools/worldgen/canyon_v2_wreckage.py` writes the layer as tagged `add` / `delete` ops into the
workshop's `manifest_edits.json` overlay (the same mechanism the editor's F5 save uses), plus a
`canyon_v2_wreckage` block holding the seed, rules, per-scene summary, per-asset counts, rejection
counts and the source provenance. Re-running the tool replaces its own ops and leaves everything
else in the overlay alone; `--remove` strips the layer; `--seed` / `--density` re-roll or scale it.

| Scene kind | Sites | What stands there |
|---|---|---|
| Knocked-out armour column | 19 | 3–6 KV-1 or Panzer III wrecks in column on one shoulder, one slewed; burnt variants on scorched ground; counter-battery craters; bailed crew; drifts in the lee |
| Retreat line | 22 | 2–4 cargo lorries in echelon on one side (burnt or abandoned, doors open, snow on the tilt); carts and panje sledges on the other with their draught horses dead in the traces; kit scattered on the snow |
| Lone vehicle | 39 | a single lorry, cart, sledge or tank left where it stopped, with its horse, dropped kit and sometimes a crater |
| Overrun 45 mm position | 10 (7 placed) | heaped snow-and-earth parapet, the 53-K gun laid back along the approach, ammunition boxes, the crew, a loose wheel, dropped rifles, craters |
| Strike field | 28 | 3–6 shell craters with thrown frozen-earth rims and scorched floors, shell chips, planks |
| Dissolved unit | 6 | a carpet of dropped rifles and greatcoats, boxes and luggage, the dead, a sledge and its horse |
| Frozen horses | 2 | a supply column halted in line, horses dead in harness, one feeding site with a fire scar |
| Roadside litter | every ~24 m | kit, the dead, a crater or a scorch on one or both shoulders |
| Road-edge drifts | 110 | wind-packed drifts along the shoulders |

Sites are spaced by route metres (`Layer.series`), so the set pieces interleave every 150–250 m
and the shoulders carry litter throughout; `--density` scales every series.

Per-asset result (seed `0x20261027`, density 1.0): 2,131 placements added, 212 loose base props
displaced from under hulls, 2,343 ops in all.

| Asset | Before | Added |
|---|---|---|
| KV-1 wreck (burnt / detail) | 13 / 3 | 29 / 15 |
| Panzer III wreck (burnt / detail) | 14 / 3 | 27 / 26 |
| Cargo lorry (burnt / abandoned) | 0 | 21 / 52 |
| Wreck cart | 20 | 43 |
| Panje sledge (upright / overturned) | 0 | 11 / 8 |
| Dead draught horse (a / b) | 0 | 26 / 31 |
| 45 mm gun / gun-pit parapet | 24 / 0 | 7 / 4 |
| Shell crater scar | 0 (26 terrain craters) | 118 |
| Scorched earth | 11 | 173 |
| Snow drift | 24 | 343 |
| Casualties | ~750 | 225 |
| Dropped rifle / greatcoat heap / loose wheel / shell chip | 0 | 113 / 127 / 5 / 16 |
| Ammunition boxes, crates, jerrycans, barrels, tyres, luggage, boards, planks | — | 711 |

Engine load (`screenshot` headless, shader backend): 13,608 placements in 1,240 pages over 73
prototypes (was 11,689 / 886 / 56); TLAS demand 26,861 of 131,072 instances (was 24,942); the
T-34 navigation floor keeps 69,021 of 306,218 cells clear (was 70,712). No page or instance
budget is near its limit.

## Placement rules (all in the tool, none hand-placed)

- Every item rests on `heights.npy` (bilinear, the engine's sampler); vehicles, guns and horses
  tilt with the local gradient through the placement `tilt_deg`, capped at 9°.
- Valley floor only: height within 9 m of the route's own floor at the nearest fraction, local
  slope ≤ 0.30 (0.26–0.28 for hulls, up to 0.5 for scars and drifts).
- Off the drivable centreline: a hull's near edge stays beyond the course's 12 m lane half-width
  (`gaea_canyon_v2.dress` lane); slewed vehicles use their half-length.
- Clear of buildings (oriented footprint corners against the building rectangles), the garrison
  posts and encounter beats (7 m), the spawn (60 m) and the goal (40 m).
- Base props: trees, rocks, fences, walls, outposts, existing wrecks and the dead block a hull.
  Loose yard debris (crates, boards, planks, bricks, buckets, barrels, logs, drifts) under a hull
  is displaced with a tagged `delete` op rather than left inside the wreck.
- Vehicles slide along the shoulder and outward, never into the lane, when their first spot is
  taken; a column that finds no room on one side tries the other.

## New props

`tools/worldgen/canyon_v2_wreckage_props.py` writes engine-native procedural meshes to
`assets/authoring/battlefield/` with `wreckage-props.receipt.json`: a burnt and an abandoned
bonneted 3-tonne cargo lorry, two dead draught horses in harness, upright and overturned panje
sledges, a crater rim, a gun-pit parapet, a dropped rifle, two greatcoat heaps and a loose spoked
wheel. The lorry, horses and sledges carry the `_wreck_burnt` / `_wreck_detail` suffix so
`src/manifest_wreck_collision.rs` gives them source-surface collision; the low scars and kit are
dressing without collision. They are inferred composite props of attested conditions, not
replicas of a named make; no purchased geometry is copied.

## Provenance

Class: attested recurring condition of the pocket's roads and balkas, late November 1942 to
January 1943; the Gaea course itself is fictional. Nothing readable was added. Sources, recorded
verbatim in the manifest block:

- `docs/ENVSTORY.md` "The Frozen Horses" and `docs/research/data/envstory.json`: the Sixth Army's
  draught horses slaughtered and frozen (Bechler; axes rebounding, pioneer saws).
- `docs/research/PHOTO_REFERENCE_ANALYSIS_20260928.md` [05-01] Strunnikov 1943, an abandoned
  column in echelon on one side of a snow road with kit on the other; [05-02] a burned cargo
  lorry with slat sides and canvas bows; [05-05] a silenced battery behind heaped parapets with a
  loose wheel; [05-03] the Romanian cavalry's saddles (the horses are absent).
- `docs/research/URBAN_INTRO_TABLEAUX_20260928.md`: the frozen dead that could not be buried
  (Beevor ch. 20–21; Stolberg report BA-MA RW4/v.264); horses slaughtered for food.
- `docs/research/HISTORICAL_SCENE_SOURCE_AUDIT_20260925.md` A1-06: the Gumrak/Karpovka road,
  burning stores and abandoned usable equipment, late November 1942 (79th Division diary).
- `docs/research/SOVIET_AT_WEAPONS_BRIEF_20260928.md`: the 45 mm M1937 (53-K) at every level,
  crews of six, horse draught.
- `docs/research/STALINGRAD_URBAN_FABRIC_20260928.md` §6.4: wrecks along one side of a road.

Not claimed: any unit identity, date, casualty count or butcher site for a given scene. The
Tiger's presence and the compressed chronology are validated separately.

## Verification

- `python tools/worldgen/test_canyon_v2_wreckage.py`: 4 checks pass (deterministic and removable;
  every placement on the heightfield, outside the lane, spawn and houses, tilt ≤ 9°; only loose
  debris displaced; cameras follow the route).
- Headless stills, shader backend, 800×450 internal, clear sunset (`1942-12-21T12:40Z`) and
  blizzard, seven recon cameras on the route: `captures/canyon_v2_wreckage_before_<cam>_<weather>.png`
  against `captures/canyon_v2_wreckage_after_<cam>_<weather>.png`, cameras `start`, `approach`,
  `settlement`, `factory`, `ridge`, `descent`, `final`; logs beside them. Reproduce with
  `python tools/worldgen/canyon_v2_wreckage.py --shots PREFIX` (add `--renderer raymarch` on the
  raymarching branch).
- Not done: a live drive through the layer (collision against the new hulls is the engine's
  existing source-surface proxy path, not re-measured here), and a Blender assembly of the overlay.

## Reproduce

```sh
python tools/worldgen/canyon_v2_wreckage_props.py      # the new meshes (idempotent)
python tools/worldgen/canyon_v2_wreckage.py            # the layer into manifest_edits.json
python tools/worldgen/canyon_v2_wreckage.py --report   # per-asset counts of the current layer
python tools/worldgen/canyon_v2_wreckage.py --remove   # strip it again
```

Both tools take `--root` for a checkout that holds the assets and the release binary. The first
run keeps a copy of the untouched overlay at `artifacts/authoring/canyon-v2-wreckage/manifest_edits.before.json`.
