# Pizza Doggy owner-private Blender yard

The private yard is a visual inspection tool for every Pizza Doggy GLB currently present in the
owner's ignored source tree. It is not a shipping asset, an SDK, a template, or a redistributable
Blender library.

Two owner-private source closures were verified on 2026-09-01:

- The Mac's ignored runtime/source subset has 1,194 GLB 2.0 files totaling 614,246,516 bytes.
- Hal has the full historical 1,243-model closure: the same 1,194 plus the 49-file standard PSX
  Nature variant. Hal also has 49 `*_orig.glb` preservation copies produced by
  `fix_sideways_glbs.py`; these are source backups, not additional Pizza Doggy assets, and are
  deliberately excluded.

The full Hal closure is:

| Pack | GLBs |
|---|---:|
| PSX Mega Pack 3.1.3 | 489 |
| PSX Mega Pack II v1.8 | 549 |
| PSX Nature v1.7.3 | 98 |
| PSX Tech v1.2.3 | 63 |
| Modular Retro FPS Kit v1.5 | 44 |

The 1,243-model inventory counts two vendor PSX Nature variants: standard and
`Tree branches separated`. The Mac has only the separated variant; Hal has both. The builder always
excludes generated `*_orig.glb` copies and requires the caller to state the expected closure count.

Build the full yard on Hal while reading its ignored warehouse without changing it:

```sh
blender --background --factory-startup \
  --python /home/alex/panzerrechnersimulator-asset-warehouse/authoring/pizza-yard/build_pizza_asset_yard.py -- \
  --source-root /home/alex/tank_simulator/assets/pizza/_extracted \
  --output-dir /home/alex/panzerrechnersimulator-asset-warehouse/authoring/pizza-yard/output \
  --expected-count 1243 \
  --render
```

The generated directory is ignored. It contains the compressed `.blend`, a per-file manifest with
source hashes/original bounds/display scales, a Markdown report, and a rendered overview. Open the
`PIZZA_DOGGY_ASSET_YARD` scene for the normalized display. The second
`PIZZA_DOGGY_RAW_SOURCE_LIBRARY` scene preserves imported dimensions in pack/category/asset
collections. Each raw asset collection is marked for the Blender Asset Browser. Display instances
and lightweight viewport-label empties are searchable and carry the same owner-private metadata.

Validate the saved file in a new Blender process:

```sh
blender --background \
  artifacts/pizza-doggy-blender-yard-20260901/pizza_doggy_all_1243.blend \
  --python tools/worldgen/build_pizza_asset_yard.py -- \
  --validate-blend \
    artifacts/pizza-doggy-blender-yard-20260901/pizza_doggy_all_1243.blend \
  --manifest artifacts/pizza-doggy-blender-yard-20260901/manifest.json \
  --expected-count 1243
```

Pizza Doggy's retained purchased agreement permits use and modification in a game, but not
standalone resale, sharing, or redistribution of the source assets, edited copies, Blender
libraries, asset packs, templates, or general-purpose bundles. Keep the yard owner-private and
re-review retained purchase evidence plus current vendor terms before public/commercial release.
