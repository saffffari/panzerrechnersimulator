# Spring 1943 vegetation

Trees, shrubs and war-damaged trees for the spring maps (a May steppe river town in Ukraine or
southern Russia, 1943), replacing the PSX Nature trees whose dark pines and grey-brown leaf cards
do not fit. Generated, not hand-modelled:

```sh
python3 tools/vegetation/spring1943.py            # -> $KESSEL_ASSET_ROOT/assets/vegetation/spring1943/*.glb
                                                  #    + spring1943.catalog.json (names, sizes, triangles, optics)
python3 tools/vegetation/spring1943.py OUT --only tree_oak_a,shrub_hazel_b
blender --background --python tools/vegetation/preview_lineup.py -- ASSET_DIR out.png [prefixes]   # shape review
python3 tools/vegetation/swap_workshop_trees.py SRC_WORKSHOP DST_WORKSHOP   # a copy with the library trees swapped
python3 tools/vegetation/capture_trees_test.py all                         # engine stills of that copy
```

The GLBs are assets (gitignored, on the asset drive); the scripts are the source. Everything is
deterministic (seeds are CRC32s of asset names). Pure Python + numpy; Blender is only the optional
preview.

## Representation

Foliage is opaque geometry. The raymarch backend alpha-tests only the grass kind, in the G-buffer
and its own shadow program, and keeps grass-kind batches out of the RT acceleration structure, the
cabin shadows and the far cascade; every other triangle shadows and occludes as drawn. A textured
leaf card would therefore shadow as a solid quad, and turning tree batches into grass-kind batches
would drop them from those passes. So each leaf is a triangle: **twigs** of 3-5 single-triangle
leaves fanned round a shoot tip, each leaf about 3 x its real length (birch 0.18 m, lime 0.27 m,
oak 0.30 m, a horse chestnut leaflet 0.38 m; `LEAF_SIZE`), so one leaf spans a few pixels at
20-40 m on the 1024x576 grid. Twigs gather on the skin of **clumps** on the crown's shell, with
sky between clumps; every clump is fed by a drawn twig branch from a **scaffold limb** (limbs are
a k-means of the clumps, so the architecture grows toward the foliage), every limb from the
trunk. Birch and willow leaves hang (weeping). Horse chestnut carries upright candles; apple,
hawthorn and elder carry small blossom trusses (May).

The first version (2026-11-02, morning) drew a twig's worth of leaves as one 0.4-1.6 m polygon;
at 5-40 m those read as oversized paper cards and were replaced the same day.

Leaf area is the price of leaf-sized geometry: leaf area = triangles x length^2 x width / 2, so a
crown's fill is bounded by its triangle budget. Leaf triangles are set in proportion to the crown
radius (1,400 per metre, 4,000-11,000): the lime and the fruit trees read full, the large oak and
poplar crowns read open (fitting mid-May, when oak leaves are still expanding). Denser crowns at
true leaf size need alpha-masked foliage in the renderer (above) or a larger budget.

Budgets: an intact tree 4,312-11,186 triangles, a burnt tree 1,073-2,456, a shattered tree
262-702, a fallen tree 4,461-11,330, a shrub 887-1,835 (burnt 414-454), a stump about 35. There
is no instance LOD: a map's tree count times these is drawn in every pass that sees it.

Frame: engine Y up, metres, origin at the trunk's ground contact; the trunk continues 0.25 m below
to sit into slopes. This equals the Blender bakes' glTF Y-up export (engine = (blender_x,
blender_z, -blender_y)). Placement scale 0.85-1.15 keeps true sizes (the library trees needed
1.3-2.2).

## Materials (physically based, no painted light)

Constant glTF factors; the loader merges them into its nearest one-row material atlas.

- Leaves: PROSPECT-D reflectance at the renderer's lanes (680/550/440 nm) through
  `tools/vegetation/leaf_optics.py`, a line-for-line port of `src/leaf_optics.rs` (the meadow's
  model). Young May leaves per species (chlorophyll 22-38 ug/cm2, oak with a little anthocyanin,
  willow with a higher structure parameter for its silky hairs), three tones (chlorophyll x0.8 /
  1.0 / 1.2). Scorched leaves (brown pigment, little water) and wilted leaves from the same model.
  Rho and tau per species are written to the catalog. **Tau is not rendered**: GLB surfaces shade
  as opaque reflectors (logged in docs/PHYSICAL_TRUTH_AUDIT.md).
- Bark, charcoal, fresh splinters, stripped wood, soil, roots, petals: constant albedos from
  published reflectance ranges (the `MATERIALS` table in the generator names each).

## Names and the engine

- `tree_<species>_<v>`, `tree_<species>_burnt_<v>`, `tree_<species>_shattered_<v>`: live
  destructible trees. `src/trees.rs::is_live_tree_asset` keys on the first token `tree`; the trunk
  contact diameter is measured from this mesh at the bow's height (1.2 m), so the generator keeps
  sprays and hanging broken limbs clear of the trunk there. Heights at or above 6.5 m are big trees
  (shells and the Tiger's shove), below that small trees (they snap under the tracks).
- `fallen_<species>_<uprooted|snapped>_<v>`, `stump_<species>_snapped_<v>`, `shrub_<species>_<v>`,
  `shrub_burnt_<v>`: static dressing with no collision, as the library's logs, stumps and ferns.
  `stump_X_snapped_a` is the stump the trunk of `fallen_X_snapped_a` broke from (same trunk
  radius, break at 0.7 m); place the fallen tree with its origin at the stump, its length along +X
  before yaw.
- No catalog registration is needed to load them (`catalog/runtime_assets.v1.json` is the
  reviewed closure for packaging, not a load gate). Add entries when a shipped act manifest
  adopts them.

## The set

Species fit the place: white willow, black poplar and silver birch on the river; lime and horse
chestnut in the streets; oak and wild pear in field margins and woods; apple and cherry in
gardens; hazel, elder, hawthorn, dog rose, blackthorn and young willow scrub. Sizes below are of
the generated files at scale 1 (m); `spring1943.catalog.json` is the authoritative list.

| Asset | Kind | Size (m) | Triangles |
| --- | --- | --- | --- |
| `tree_birch_a/b/c` | tree | h 16.1 / 11.8 / 16.4, crown 7 / 6 / 9 | 4,606-5,369 |
| `tree_poplar_a/b/c` | tree | h 24.2 / 21.7 / 22.7, crown 18 / 17 / 17 | 9,062-10,621 |
| `tree_willow_a/b/c` | tree | h 13.3 / 12.2 / 18.0, crown 12 / 10 / 16 | 6,536-9,920 |
| `tree_linden_a/b` | tree | h 15.0 / 12.9, crown 10 / 9 | 5,297-6,347 |
| `tree_chestnut_a/b` | tree | h 17.8 / 11.8, crown 14 / 12 | 6,583-8,320 |
| `tree_oak_a/b/c` | tree | h 19.1 / 14.8 / 18.9, crown 20 / 16 / 18 | 8,558-11,186 |
| `tree_pear_a/b` | tree | h 7.9 / 8.4, crown 5 / 6 | 4,312-4,320 |
| `tree_apple_a/b/c` | tree | h 6.4 / 4.9 / 6.1, crown 6 / 6 / 7 | 4,474-4,590 |
| `tree_cherry_a/b` | tree | h 7.5 / 7.0, crown 7 / 6 | 4,360-4,463 |
| `tree_{birch,poplar,oak}_burnt_{a,b}`, `tree_{willow,linden,apple,cherry}_burnt_a` | tree | as the species; charred to 35-75 % of height, 60-85 % of clumps bare | 1,073-2,456 |
| `tree_{birch,poplar,linden,oak}_shattered_{a,b}`, `tree_{willow,chestnut,pear}_shattered_a` | tree | h 4.4-14.8, splintered top, broken limbs | 262-702 |
| `fallen_{oak,birch,apple}_uprooted_a` | fallen | length 20.9 / 13.4 / 5.0, on its root plate | 4,461-11,330 |
| `fallen_{poplar,willow,linden,birch}_snapped_a` | fallen | length 19.0 / 11.9 / 14.7 / 14.6 | 4,737-8,074 |
| `stump_{poplar,willow,linden,birch}_snapped_a` | stump | h 1.4-1.5, d 0.32-0.68 | 33-36 |
| `shrub_{hazel,elder,hawthorn,dogrose,blackthorn,willowscrub}_{a,b}` | shrub | h 1.0-4.2 | 887-1,835 |
| `shrub_burnt_a/b` | shrub | h 3.6 / 1.8 | 414-454 |

## Known limits

- Leaf transmittance is not rendered (back-lit crowns are too dark by the transmitted share).
- One spray stands for a twig's worth of leaves; gap fraction is a property of the generator's
  clumping, not a measured per-species value.
- Shrubs, fallen trees and stumps have no collision or destruction.
- Damaged trees do not fracture further when shot; they fall whole as every live tree does.
