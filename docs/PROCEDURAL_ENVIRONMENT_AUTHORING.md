# Procedural environment authoring

Kessel's editor uses a hybrid model: deterministic environmental recipes establish a coherent
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

The resulting Kessel principle is: **shared fields → named recipes → stable candidates → authored
deltas**. Coherence comes from systems sharing causes, while composition remains under human
control.

## Next editor-authoring layers

The next step is causal aftermath stamps rather than more uniform debris scatter. An armored
ambush, artillery strike, burned convoy, failed assault, ruined farm, or field hospital should emit
a correlated chain: impact and scorch, broken vegetation, wreck trajectory, directional debris,
bodies and dropped equipment, then optional smoke/light/sound/hazard tags. These templates should
use roads, trenches, cover, entrances, lines of fire, faction, intensity, and elapsed time, and
remain separately editable.

At larger scale, regeneration should become dirty-tile and dependency-layer based, with exclusion
volumes, frozen procedural objects, density paint, rejection-reason overlays, instance/collision
budgets, and a seed browser that captures thumbnails and retains failing seeds. Dense grass and
micro-groundcover belong in the performance/rendering lane; this editor commit intentionally only
authors and materializes ordinary objects through existing renderer APIs.
