# Procedural environment authoring

Panzerrechnersystem's editor uses a hybrid model: deterministic environmental recipes establish a coherent
base, then ordinary non-destructive object edits provide the final art direction. The procedural
result is not evaluated every frame and never silently reshuffles after an artist moves something.

## Current editor workflow

1. Open **Brushes** and choose Mixed Forest, Forest Floor, Battlefield Debris, or Aftermath; or
   Shift-click model cards across any Pizza tabs to make a custom mix.
2. Press **B** and paint locally, or press **I** once to initialize the recipe across the act.
3. Tune the generated objects with the existing transform, duplicate, delete, prefab, and scene
   block tools. Generated objects are ordinary placement operations from this point onward.
4. Use **Story Details** to add or refine bodies and burned wrecks where the story needs a readable
   focal point.
5. Use **Lights** for local fire/work/cold/emergency lights or select Sun/Sky to tune sun direction,
   exposure, fog, and storm preview. F5 stores these renderer-neutral authoring records alongside
   the existing map edits.
6. Open **Materials** to author texture-mix, mud, scorch, and frozen-red-snow zones. Shift-click up
   to four texture cards to place an equal-weight layered mix. These zones are outlined and
   editable now; the graphics lane consumes the saved records for actual terrain blending/decal
   rendering.
7. Open **Scenes** to stamp a lit utility road, furnished forward base, destroyed convoy, or
   overrun artillery position. A stamp validates every required model and companion record first,
   then materializes the entire scene transactionally as normal editable placements.

Whole-act initialization is deterministic and capped at 12,000 ecology objects or 7,000
destruction objects per recipe. Density is reduced before generation when the map would exceed that
budget. A stroke caches its terrain extrema and destruction buckets, and all touched asset types are
repaged from one placement scan instead of rescanning the act for every tree/rock type. Missing
thumbnails stay out of the live render path; bulk preview baking remains an explicit
background/workstation job.

The interactive editor and headless editor screenshots preview the saved Sun/Sky and practical
lights. Practical-light range controls camera culling and the most relevant 32 sources are selected
for the existing renderer. Shipping-game consumption of these records, true spot/area transport,
and renderer-side cutoff/probe behavior are explicit integration points for the graphics/runtime
lane; this editor-authoring commit does not edit `game.rs`, renderer code, or shaders.

## Shared environmental fields

Every recipe currently consumes one sample with:

- terrain height and slope;
- sun exposure derived from surface normal and the authored/current sun direction;
- concavity, coherent soil moisture, and fertility proxies;
- prevailing-wind exposure;
- act-level ruination plus falloff from sites, anchors, wreck fields, and existing destructive
  placements.

The same stable world-cell identity drives candidate position, asset choice, yaw, and scale. Recipe
weights, minimum spacing, cluster scale, and soft field ranges control the result. The saved
`procedural_dressing_layers` record retains the exact seed, recipe, constraints, bounds, and count;
`authored_lights` and `environment_lighting` are separate top-level records so runtime rendering can
consume them without depending on editor code.

The newer authoring records follow the same split:

- `surface_material_zones` stores up to four safe repository-relative image layers plus semantic
  mud/scorch/frozen-blood tuning;
- `scene_recipe_instances` stores recipe revision, seed, anchor, exact resolved asset paths, and
  generated count, never fragile placement indexes;
- `infrastructure_runs` stores connected pole nodes and cable edges independently from their
  current visual stand-ins.

All 1,194 installed Pizza GLBs remain available in their pack tabs. Their 2,151 texture images are
embedded inside the models, not loose terrain tiles, so the live editor does not decode or upload
them at startup. The Materials tab exposes the small set of loose production terrain/support
images directly; exhaustive embedded-material extraction and JPEG baking remain an offline
Linux/RTX 4090 batch task. This keeps the live browser responsive and avoids recreating the earlier
memory and GPU stall.

## Why this shape

This design follows publicly documented production patterns rather than unconstrained random
scatter:

- Hello Games describes a reproducible seed hierarchy and hierarchical dressing from trees through
  secondary plants and rocks to micro-detail in its [No Man's Sky world-generation GDC talk](https://www.youtube.com/watch?v=sCRzxEEcO2Y&t=1640s) and [art-direction talk](https://www.youtube.com/watch?v=vcEA41eBOGs&t=1510s).
- Cloud Imperium's Planet Tech v4 maps climate conditions into terrain materials, vegetation, and
  preset scatter, with exclusions for authored spaces in its [official Planet Tech presentation](https://www.youtube.com/watch?v=IfCc_aDNsAw&t=1636s). Its later soil, nutrients, canopy, and competition presentation is useful direction but was explicitly an early work-in-progress, not evidence of shipped behavior.
- Guerrilla documents Horizon Zero Dawn's artist-authored rules and deterministic local GPU
  placement for complete environments on its [official procedural-placement page](https://www.guerrilla-games.com/read/gpu-based-procedural-placement-in-horizon-zero-dawn).
- Ubisoft's Far Cry 5 workflow uses slope, curvature, illumination, altitude, occlusion, flow, and
  wind as viability fields while retaining handcrafted navigation and progression; see the
  [GDC procedural-world slides](https://media.gdcvault.com/gdc2018/presentations/ProceduralWorldGeneration.pdf).
- Ghost of Tsushima combines physically based sky/relighting with global artistic controls and
  local overrides in its [SIGGRAPH 2021 lighting course](https://advances.realtimerendering.com/s2021/jpatry_advances2021/index.html).

The resulting Panzerrechnersystem principle is: **shared fields → named recipes → stable candidates → authored
deltas**. Coherence comes from systems sharing causes, while composition remains under human
control.

## Scene recipes and current boundaries

The first causal stamps now cover a disabled convoy and an overrun artillery position. Wrecks,
casualties, supplies, debris, scorch, and red-snow zones share a seed and causal cluster instead of
forming a uniform random field. The forward base builds a fenced motor pool, enterable
ground-floor barracks and signals room, practical lights, beds, tables, chairs, radio/medical
props, crates, and fuel cans. It intentionally uses the exact wall, doorway, pillar, garage, and
warehouse families already understood by runtime collision. Arbitrary multi-storey Pizza modules
still lack generalized floor, stair, shelter, navigation, and destruction support, so the editor
does not claim those interiors are playable yet.

The utility recipe stores a real node/edge graph and places poles, holders, lamp housings, lights,
and fitted wire previews. Because the existing placement transform supports yaw only, it rejects a
stamp when connected endpoints differ by more than 0.35 m of terrain height. Full pitched/catenary
wires are a narrow runtime geometry handoff rather than corrupted authored data.

Spatial sound emitters can already be placed and tuned in the editor, but playback, weather-driven
wind/rain/thunder, terrain material mixing, fog transport, and stain appearance remain explicit
audio/graphics/runtime integrations. The editor records are designed so those lanes can consume
them without changing artist-authored maps.

The next authoring expansion is more reviewed causal templates—failed assaults, ruined farms,
field hospitals, abandoned civilian columns, and interior room kits—driven by roads, trenches,
cover, entrances, lines of fire, faction, intensity, and elapsed time. Each should remain
transactional, deterministic, and separately editable.

At larger scale, regeneration should become dirty-tile and dependency-layer based, with exclusion
volumes, frozen procedural objects, density paint, rejection-reason overlays, instance/collision
budgets, and a seed browser that captures thumbnails and retains failing seeds. Dense grass and
micro-groundcover belong in the performance/rendering lane; this editor commit intentionally only
authors and materializes ordinary objects through existing renderer APIs.
