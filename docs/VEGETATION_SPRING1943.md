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
and its own shadow program; every other triangle shadows and occludes as drawn. A textured leaf
card would therefore shadow as a solid quad. Instead each **spray** (a twig's worth of leaves) is
an irregular, slightly cupped six-triangle polygon whose outline is the cut-out, 0.4-1.6 m
(about 0.21 x the crown radius, so a spray is a few pixels at combat range on the 1024x576 grid).
Sprays gather in **clumps** on the crown's shell, leaving sky between clumps; every clump is fed
by a drawn twig from a **scaffold limb** (limbs are a k-means of the clumps, so the architecture
grows toward the foliage), every limb from the trunk. Birch and willow sprays hang (weeping).
Horse chestnut carries upright candles, apple, hawthorn and elder flat petal sprays (May).

Budgets: an intact tree 2,500-4,600 triangles, a damaged tree 200-2,500, a fallen tree up to
4,300, a shrub 300-800, a stump about 35.

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

| Asset | Kind | Size | Triangles |
| --- | --- | --- | --- |
| `tree_birch_a/b/c` | tree | h 16.5 / 11.8 / 16.4 | 3,300-3,950 |
| `tree_poplar_a/b/c` | tree | h 25.8 / 21.5 / 22.5, crown 19-24 | 3,850-4,600 |
| `tree_willow_a/b/c` | tree | h 13.4 / 12.2 / 19.8, multi-stemmed, leaning | 3,450-3,650 |
| `tree_linden_a/b` | tree | h 15.3 / 13.0 | 3,550-4,400 |
| `tree_chestnut_a/b` | tree | h 17.5 / 12.4, candles | 3,350-4,050 |
| `tree_oak_a/b/c` | tree | h 20.4 / 15.1 / 19.4, crown 19-24 | 3,850-4,300 |
| `tree_pear_a/b` | tree | h 7.8 / 8.4 | 2,850-3,250 |
| `tree_apple_a/b/c` | tree | h 6.3 / 5.0 / 6.3, crown 5.8-7.7, late blossom | 2,550-3,300 |
| `tree_cherry_a/b` | tree | h 7.8 / 7.0 | 2,500-3,100 |
| `tree_{birch,poplar,oak}_burnt_{a,b}`, `tree_{willow,linden,apple,cherry}_burnt_a` | tree | as the species; charred to 35-75 % of height, 60-85 % of clumps bare | 1,350-2,500 |
| `tree_{birch,poplar,linden,oak}_shattered_{a,b}`, `tree_{willow,chestnut,pear}_shattered_a` | tree | snapped at 3.5-12 m, splintered top, broken limbs | 200-380 |
| `fallen_{oak,birch,apple}_uprooted_a` | fallen | length 21 / 13.5 / 5.2, on its root plate | 3,150-4,350 |
| `fallen_{poplar,willow,linden,birch}_snapped_a` + `stump_*_snapped_a` | fallen, stump | length 13-19; stumps 1.4-1.5 high | 3,900-4,100; 35 |
| `shrub_{hazel,elder,hawthorn,dogrose,blackthorn,willowscrub}_{a,b}` | shrub | h 0.8-3.8 | 380-770 |
| `shrub_burnt_a/b` | shrub | h 3.7 / 1.8 | 320-330 |

## Known limits

- Leaf transmittance is not rendered (back-lit crowns are too dark by the transmitted share).
- One spray stands for a twig's worth of leaves; gap fraction is a property of the generator's
  clumping, not a measured per-species value.
- Shrubs, fallen trees and stumps have no collision or destruction.
- Damaged trees do not fracture further when shot; they fall whole as every live tree does.
