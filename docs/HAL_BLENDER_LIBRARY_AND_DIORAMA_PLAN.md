# Hal Blender Asset Library and Diorama Plan

Status: planning and reminder document only. Do not run the full library build, open the asset
yard, or generate preview batches on the MacBook. Perform this work on the Linux Hal workstation.

Last updated: 2026-09-02

## Goal

Use Blender for the work it is particularly good at:

- browsing and kitbashing the complete private asset collection;
- composing dense hero scenes, interiors, wrecks, and destruction by hand;
- building reusable collection assets, material palettes, and Geometry Nodes brushes;
- studying composition, scale, light, weather, and environmental storytelling as small dioramas;
- exporting approved modular assets or self-contained scene blocks back to Panzerrechnersystem.

The Panzerrechnersystem editor remains the authority for broad map layout, procedural population, gameplay
placement, spatial sounds, and final in-engine tuning. Blender is the detailed authoring and
look-development companion, not a replacement map format. A Blender render is evidence for a
visual idea; the Panzerrechnersystem renderer is the final visual truth.

This decision supersedes the older line in `docs/SPEC_REMAINING.md` saying that all authoring must
happen in-engine. The narrower rule now is: broad procedural worlds and final gameplay placement
stay in Panzerrechnersystem; intricate hero assets, interiors, reusable scene blocks, and visual studies may be
authored in Blender and brought back through a tested export contract.

## What already exists

We did build the foundation of this system before:

- `tools/worldgen/build_asset_library.py` imports Pizza GLBs, makes one compressed `.blend` per
  pack/category, marks each imported collection as an asset, creates stable catalogs, and requests
  Blender previews.
- `tools/worldgen/assemble_blend.py` converts a procedural Panzerrechnersystem manifest and terrain into an
  editable Blender world while applying `manifest_edits.json`.
- `tools/worldgen/render_shot.py` renders a ground-level study from Panzerrechnersystem/engine coordinates.
- `assets/worldgen/scene_blocks/historical_scene_catalog.v1.json` is the canonical sourced scene
  shortlist and placement-constraint catalog.
- `docs/ENVSTORY.md` is a larger 23-scene idea bank.
- `CLAUDE.md` defines the environmental-storytelling and physical-truth rules.

What does **not** currently survive on this Mac:

- no generated `assets/pizza/_asset_library/` directory;
- no Pizza `blender_assets.cats.txt` or generated library `.blend` files;
- no saved world `.blend` corresponding to the old Act 2/3 blockout renders;
- no documented Tiger source file at
  `assets/tanks/tiger/Panzerkampfwagen_VI_Tiger_Ausf.E.blend`.

Before rebuilding, search Hal, backup drives, and old private worktrees for those exact names. Do
not overwrite a recovered Tiger source or asset library until it has been inspected and compared.

Six rough Blender blockout renders remain as references:

- `assets/worldgen/act2/shot_bridge.png`
- `assets/worldgen/act2/shot_manor.png`
- `assets/worldgen/act2/shot_road.png`
- `assets/worldgen/act2/shot_village.png`
- `assets/worldgen/act3/shot_battle.png`
- `assets/worldgen/act3/shot_wrecks.png`

They are useful for remembering the earlier workflow, but they are sparse blockouts with flat
lighting and debug labels—not finished diorama targets.

## Current inventory baseline

Treat `assets.csv` as the inventory index and the actual source files as the binary source of
truth. At this checkpoint it contains 1,663 rows, including:

- 1,194 Pizza GLBs across five installed packs;
- 38 project GLBs under `assets/tanks`, `assets/flavor`, and `assets/88mm`;
- one CC0 character-source FBX;
- 272 audio entries, many of which are auditions or generated authoring artifacts rather than
  production-ready sounds;
- project textures, terrain data, manifests, scene blocks, fonts, and supporting authoring files.

Installed Pizza pack counts:

| Blender top-level catalog | Models |
|---|---:|
| PSX Mega Pack II v1.8 | 549 |
| PSX Mega Pack 3.1.3 | 489 |
| PSX Tech v1.2.3 | 63 |
| PSX Nature v1.7.3 | 49 |
| Modular Retro FPS Kit v1.5 | 44 |
| **Total** | **1,194** |

The generated Blender library is private authoring data. It contains paid source assets and must
not be committed, pushed, placed in Git LFS, or shipped as a standalone library. Transfer it only
between owned machines.

## Stop: fix the old builder before the full run

Do not run the current builder over the complete collection unchanged. Its core design is useful,
but the following issues must be fixed on the editor/authoring branch first:

1. Its `Tree branches separated` skip rule matches the only installed PSX Nature directory. The
   current script would silently omit all 49 Nature assets and build only 1,145 of 1,194 Pizza
   models.
2. Running with a `pack_filter` rewrites the shared catalog with only that pack while leaving old
   `.blend` files behind. Incremental runs can therefore orphan earlier catalogs.
3. Import and preview failures are printed but do not fail the build. The final count reports
   requested assets instead of successful assets.
4. Files are written directly to their final locations. An interruption can leave a partial
   `.blend`, stale catalog, or mixed-version library.
5. It does not read IDs, categories, use state, previews, or license/provenance metadata from
   `assets.csv`.
6. Preview framing is not standardized, and blank previews are accepted without a completeness
   receipt.
7. The build can inherit the user's Blender startup file or add-ons unless Hal launches it with
   factory settings.

Required hardening:

- drive the model list from `assets.csv`, then verify every path still exists;
- preserve separate pack/version catalogs and stable asset IDs;
- stage output in a temporary sibling directory and atomically publish only after validation;
- produce a JSON or CSV receipt containing asset ID, source path, source hash, output `.blend`,
  catalog ID, import result, preview result, Blender version, and build timestamp;
- make a missing import, duplicate ID, missing catalog, or missing preview a non-zero build result;
- make filtered/incremental builds merge-safe, or remove that mode until it is safe;
- run with a clean factory startup and a pinned Blender version;
- resume at category boundaries so a failed preview batch does not require starting over;
- never modify the original Pizza GLBs.

## Hal setup checklist

- [ ] Use a clean Hal checkout/worktree with the intended editor-authoring commit recorded.
- [ ] Copy the complete private `assets/` warehouse onto Hal's local NVMe.
- [ ] Confirm 1,194 installed Pizza GLBs before adding any newer packs.
- [ ] Search Hal/backups for the previous `_asset_library`, catalog file, and Tiger source `.blend`.
- [ ] Choose one Blender LTS build, record `blender --version`, and keep all collaborators on a
      version that can open its files.
- [ ] Confirm the NVIDIA driver and Blender can see the RTX 4090.
- [ ] Check GPU use before preview batches. Other GPU workloads may be occupying VRAM; do not stop
      them without separate authorization.
- [ ] Keep the generated library and workfiles private and untracked.
- [ ] Back up the source archive and any recovered hand-authored `.blend` before conversion.

Suggested private layout on Hal:

```text
assets/
  pizza/
    _extracted/                 # untouched purchased GLBs
    _asset_library/             # generated Blender library, private
      blender_assets.cats.txt
      psx_mega_pack_ii/
      psx_mega_pack/
      psx_tech/
      psx_nature/
      modular_retro_fps_kit/
  blender/
    kessel_library/             # project-owned and quarantined project assets
    dioramas/
      act1/
      act2/
      act3/
    materials/
    geometry_nodes/
    recovery/                   # recovered historical work pending comparison
```

After the builder is fixed, start with one small category—not the whole library:

```sh
cd /path/to/tank_simulator
blender --background --factory-startup \
  --python tools/worldgen/build_asset_library.py -- \
  assets/pizza/_extracted assets/pizza/_asset_library "PSX Tech"
```

The incremental catalog bug must be fixed before using the filtered command above. When the smoke
test passes, run one complete build without a filter and validate its receipt before registering
`assets/pizza/_asset_library` in Blender Preferences as an Asset Library.

## Blender library design

### Purchased Pizza catalogs

Each installed pack remains its own top-level catalog. Preserve the source version in metadata even
if the shorter catalog label omits it. Under each pack, expose the source categories and additional
search tags such as:

- Buildings and structural modules
- Doors, windows, floors, roofs, stairs, pillars, and beams
- Furniture and interior dressing
- Industrial machinery and utilities
- Lighting fixtures
- Debris and destruction
- Crates, ammunition, tools, radios, and medical props
- Nature: trees, branches, logs, rocks, grass, and ground clutter
- Weapons
- Wires and cable pieces

Use collections as the primary assets so multi-object GLBs remain intact. Attach searchable custom
properties for `asset_id`, source pack/version, original relative path, license class,
`currently_used`, `runtime_packaged`, object category, and subcategory.

### Panzerrechnersystem project catalogs

Build separate local catalogs for:

- Tiger exterior, interior parts, sockets, crew/station references, and 8.8 cm ammunition;
- Soviet and German vehicle components and wreck variants;
- reviewed characters, pose studies, bodies, and body-placement rigs;
- reusable building shells, interior-room kits, and structural-damage modules;
- scene-block anchors, gameplay-clearance guides, Tiger scale references, and Panzerrechnersystem cameras;
- lights and fixtures with physically meaningful metadata;
- terrain decals and meshes: tracks, craters, scorch, red snow, mud, puddles, and debris masks;
- reusable Geometry Nodes tools and multi-asset brushes;
- completed diorama collections approved for reuse.

The current project models include unresolved provenance. Keep anything marked `unreviewed` or
`kessel-provenance-unrecorded` in an obvious **QUARANTINE — DO NOT SHIP** catalog until ownership
and redistribution status are recorded. Private visibility in Blender does not make an asset safe
for a public build.

### Materials and texture studies

Create material assets for authoring and look development, not baked lighting cheats:

- clean, tracked, compacted, wind-scoured, and blood-stained snow;
- frozen ground, thawing mud, deep mud, track-rut water, and flooded field water;
- wet/dry masonry, soot, scorch, exposed steel, rust, ash, char, and dust;
- interior ivory paint, worn steel, grease, glass, fabric, wood, and chipped concrete;
- terrain mixing groups that blend by slope, height, curvature, water table, traffic, and weather.

Maintain true material values. Do not pre-darken assets to imitate the final mood, bake the sun or
fog into albedo, or use unphysical fill lighting. Weather/exposure studies should remain adjustable.

Newer loose texture packs can be brought in later, after their source and license records are
registered. Do not overwrite PSX Mega Pack 3.1.3 with a newer version in place; import another
version as a separate source family and compare shared assets by hash and appearance.

Deferred private intake to review on Hal includes the PSX Mega Pack 3.2.1 archive and the available
Pizza Mystery packages. Treat 3.2.1 as a parallel version, not an in-place replacement: it contains
new objects as well as changed copies of shared models. De-duplicate identical archives by hash,
register purchase/license evidence, inventory their GLBs and loose textures, and only then add new
top-level catalogs. Missing Mystery volumes should remain visibly missing rather than being inferred
or silently represented as complete.

### Geometry Nodes and multi-asset brushes worth building

These are strong candidates for Blender because they need art-directable procedural relationships:

1. **Forest ecology brush** — several tree families, saplings, grass, rocks, sticks, fallen logs,
   stumps, and litter, driven by slope, height, soil/wetness, sunlight/canopy, wind exposure, road
   distance, and a deterministic seed.
2. **Wind and snow-drift brush** — deposits snow on leeward faces, hollows it around obstacles,
   aligns loose debris with wind, and exposes high ridges.
3. **Rubble/destruction brush** — derives brick, plaster, timber, glass, twisted metal, dust, and
   larger structural fragments from source wall type, collapse direction, blast origin, and
   severity.
4. **Trench and revetment generator** — spline-driven cut, parapet, duckboards, timber supports,
   drainage, firing bays, dugout mouths, and disturbed soil.
5. **Telephone/utility run** — pole sockets, terrain-following placement, physically sagging wire,
   broken spans, insulators, lamp attachments, and branch connections.
6. **Road and retreat trail** — ruts, tracks, churned shoulders, dropped kit, repairs, broken-down
   vehicles, and causal debris clusters rather than uniform scatter.
7. **Wreck-event composer** — impact direction, final vehicle pose, shed parts, tow cables, fuel or
   ammo evidence, crew escape path, nearby cover use, scorch, fragments, and snow/mud disturbance.
8. **Interior room dresser** — barracks, command post, aid station, workshop, cellar, and warehouse
   rules with door swings, clear walking lanes, usable surfaces, believable clusters, and damaged
   variants.
9. **Aftermath/body placement tool** — restrained poses tied to cover, blast direction, aid points,
   evacuation routes, and time/weather state. It must support a no-gore review layer and never use
   bodies as random decoration.
10. **Wetland/floodplain brush** — shallow water, reeds, drowned grass, mud islands, submerged
    timber, reflective channels, stranded debris, and hidden-crossing guides.

Every procedural tool needs a seed, non-destructive controls, an inspectable output collection, and
a way to realize only the final promoted result. Store its environmental inputs in metadata so the
scene can be regenerated instead of becoming an unexplained pile of objects.

## Diorama authoring doctrine

Every scene must communicate one coherent human event without dialogue, subtitles, readable lore,
or an explanatory sign. The event should be inferable from space, objects, damage, absence,
weather, sound intent, movement traces, and consequences.

Historical horror must come from sourced evidence and restrained composition. Do not invent an
atrocity, victim identity, or melodramatic gore merely to make a scene darker. Record source IDs and
the allowed placement class from `historical_scene_catalog.v1.json` in the `.blend` metadata.

Build each diorama at three readable scales:

1. **Landmark read:** silhouette and event type from tank distance.
2. **Event-chain read:** cause, movement, and consequence from roughly 20–50 metres.
3. **Hand-distance read:** one or two intimate details discovered on foot.

Use four standard views:

- 1.72 m first-person walking camera;
- Tiger commander/hatch camera;
- gunner optic at both useful fields of view;
- one overhead or orthographic authoring view, marked non-shipping.

Lock cameras and exposure before comparing lighting or atmosphere. Each act gets controlled A/B
studies:

- **Act I:** diffuse blue-grey whiteout; night/blizzard with one justified warm source.
- **Act II:** cold rain and low haze; post-rain low sun with wet reflections.
- **Act III:** bright clear blue sky and soft clouds; late pink sunset through smoke and forest.

Blender speaker empties may annotate intended sound location, radius, and behavior, but they are not
the authoritative sound emitters. Recreate approved emitters in the Panzerrechnersystem editor until a validated
metadata exporter exists. The same rule applies to sun, weather, fog, and scene lighting: record the
look recipe, then reproduce and validate it in the game renderer.

## First study wave: visual-language labs

Do these before expensive hero modeling. They answer what survives Panzerrechnersystem's 640×360 presentation
and establish a shared campaign visual language.

| Study | What it must prove |
|---|---|
| Scale and readability yard | Metre scale, origins, material response, and which silhouettes/details survive at game resolution using a Tiger, 1.72 m human, doorway, cart, tree, rubble, and ground patches. |
| Act I winter lab | Snow depth cues, blue-grey whiteout, flare/fire restraint, footprints, wreck silhouette, and wind-driven debris. |
| Act II thaw lab | Wet masonry, mud, shallow floodwater, reeds, ruts, low haze, and a readable hidden vehicle route. |
| Act III forest lab | Pine variation, canopy layering, firebreak sightlines, wet litter, broken trees, smoke, bright daylight, and pink-sunset color separation. |
| Interior light lab | Cellar, bunker, warehouse, and Tiger-interior materials lit only by modeled apertures or credible fixtures. |
| Destruction grammar lab | Intact, recently hit, burned, collapsed, scavenged, snowed/rained over, and long-abandoned versions of one modular structure. |

## Canonical production diorama backlog

The 12-scene production shortlist already exists in
`assets/worldgen/scene_blocks/historical_scene_catalog.v1.json`. Use those records—not names alone—
for source truth, constraints, visual beats, and placement class.

| Act | Study ID | Primary question for the Blender study |
|---:|---|---|
| 1 | `a1_burned_cellar_household` | Can an escaped household be read from approach, broken opening, cellar, courtyard trenches, and interrupted domestic objects without text? |
| 1 | `a1_grain_elevator_extraction_convoy` | Can a landmark-scale silo connect clearly to a human logistics chain of disturbed grain, sacks, carts, guards, and prisoners? |
| 1 | `a1_pitomnik_failed_evacuation` | Can white negative space, a lost runway, wrecks, stretcher rows, and converging footprints carry the whole event? |
| 1 | `a1_red_army_house_frozen_courtyard` | Can restrained post-surrender evidence remain legible and humane at both tank and foot scale? |
| 2 | `a2_floodwater_hidden_crossing` | Can a tank route vanish under shallow floodwater while still reading through banks, submerged timber, tracks, reeds, and engineer stores? |
| 2 | `a2_kustrin_pompeii_block` | Can one reusable ruined-block grammar create explorable facades, cellars, courtyards, roofs, fortress masonry, and wet rubble? |
| 2 | `a2_reitweiner_sporn_command_complex` | Can trench geometry, shelter depth, telephone runs, observation slits, and interior objects communicate command hierarchy? |
| 2 | `a2_dog_drawn_casualty_evacuation` | Does an intimate mud-road evacuation vignette survive the deliberately low-resolution final image? |
| 3 | `a3_dahme_bridge_bottleneck` | Can armor, trucks, wagons, horses, wounded people, baggage, phones, forest, and water read as one desperate traffic flow rather than random clutter? |
| 3 | `a3_forsthaus_halbe_aftermath` | Can a specific lodge remain the focal landmark while road, rail, forest, craters, and vehicle traces radiate the event outward? |
| 3 | `a3_immediate_graves` | Can a quiet aftermath scene communicate scale through fresh earth, tools, ordinary surroundings, and restrained markers without spectacle? |
| 3 | `a3_tangermunde_bridge_crossing` | Can long-depth composition, ruined-span silhouette, disarmament, baggage limits, and westward movement remain readable? |

## Additional studies and reusable scene kits

After the canonical shortlist has a coherent visual grammar, draw from `docs/ENVSTORY.md` for:

- Barmaley Fountain against the ruined railway station;
- the Stalingrad Madonna dugout;
- the Univermag basement command corridor;
- Gumrak's wrecked-aircraft and abandoned-supply edge;
- the absurd supply-drop snowfield;
- frozen horse columns and starvation evidence;
- Christmas bunker interior and the flare ring on the horizon;
- Grain Elevator defensive interior;
- Gerhardt's Mill/Pavlov's House communication-trench space;
- Tractor Factory assembly-line wreck tableau;
- flooded Oderbruch roads and mired vehicles;
- the searchlight wall in rain and fog;
- the hillside observation post;
- Halbe's jammed forest road;
- the Autobahn blocking line;
- Tigers leading the breakout through a shattered town;
- the quiet forest cemetery epilogue.

Other useful scene-kit studies:

- furnished military outpost assembled from industrial shells rather than a generic prefab;
- wrecked convoy with a readable order of events;
- lost battalion aftermath along a retreat spline;
- playable warehouse, aid station, barracks, and command-room interiors;
- streetlight/telephone-pole kit with connected wires and damaged variants;
- ruined village “chimney field” kit;
- frozen-body, red-snow, churned-ground, and abandoned-equipment layers that can be toggled
  independently for sensitivity and readability review;
- before/recent/abandoned chronology variants of the same scene.

## Review experiments

Run these on every serious study:

- **Five-second read:** show an unlabeled 640×360 frame and record what event a reviewer infers.
- **No-gore pass:** hide bodies and blood. Space, objects, damage, tracks, and absence should still
  communicate the event.
- **Asset/custom heatmap:** tint reused assets green, altered assets amber, and custom hero work red.
  This reveals where the composition is becoming unnecessarily expensive.
- **Chronology pass:** compare immediate, weathered, scavenged, and abandoned versions.
- **Density ladder:** render sparse, intended, and overloaded variants from locked cameras.
- **Diorama wall:** render every study from one neutral oblique camera and plinth to make a single
  comparable campaign-wide contact sheet.
- **Missing-hero proxy rule:** never begin expensive modeling during composition. Prove that the
  proxy object matters, then add it to the custom-asset queue.

## Required output from each study

Each study folder should contain:

- a clean versioned `.blend` with named collections and no packed purchased source archive;
- neutral/clay and fully dressed contact sheets from the same locked cameras;
- 640×360 composition previews plus larger clean masters;
- locked-camera lighting/weather A/B renders;
- an asset bill of materials split into reused, modified, custom-needed, and provenance-blocked;
- source IDs, provenance class, placement constraints, and a pass/fail note for every canonical
  visual beat;
- camera/exposure/weather/light notes needed to reproduce the look in Panzerrechnersystem;
- a list of intended spatial sounds and emitter behavior, without treating Blender speakers as
  runtime truth;
- a short performance note: object count, unique meshes, materials, texture memory, instances, and
  likely collision needs.

Promoted studies additionally need one of these delivery forms:

1. **Modular scene block:** reusable GLBs plus a `kessel.scene-block` JSON, one stable ground anchor,
   deterministic placement metadata, semantic sockets/connectors, and a draft terrain footprint.
2. **Hero landmark:** a carefully bounded GLB at `assets/hero/act<N>/<slug>.glb`, with documented
   origin, scale, materials, collision needs, and authoring source.

Coordinate contract:

```text
engine = (blender_x, blender_z, -blender_y)
```

Blender uses Z-up. Panzerrechnersystem uses Y-up and engine +Z forward. Use Blender's glTF Y-up export and verify
the result in an empty engine test scene before placing it in a campaign map.

Do not export baked sunlight, world terrain, fog cards presented as physical fog, camera-facing
lighting cheats, duplicate hidden geometry, debug labels, or the entire purchased library inside a
hero GLB.

## Asset-library acceptance gate

The Hal library is complete only when:

- [ ] all 1,194 installed Pizza GLBs appear exactly once in the receipt;
- [ ] all five Pizza packs appear as separate top-level catalogs;
- [ ] all 49 Nature assets are present;
- [ ] every successful asset has an ID, source path, hash, catalog, license tag, and usable preview;
- [ ] failed imports/previews are zero or are explicitly quarantined with reasons;
- [ ] a clean Blender file can search and drag assets from every pack;
- [ ] a scale/orientation sample from each pack lands correctly beside the Tiger/human references;
- [ ] multi-material, alpha, foliage, glass, and emissive samples have been visually checked;
- [ ] the generated library can be rebuilt deterministically from the private source tree;
- [ ] no purchased archive, complete source library, or unreviewed project asset enters Git or a
      public runtime package.

## First Hal session order

1. Recover and inspect any old library/Tiger `.blend` from Hal or backups.
2. Record the exact repository commit, Blender version, GPU/driver state, and source-asset counts.
3. Fix and test the library builder's Nature exclusion, catalog merging, receipts, and atomic output.
4. Build one small-category smoke library and inspect import, scale, materials, search, and previews.
5. Build the full five-pack private Pizza library and validate the 1,194-entry receipt.
6. Add the project/quarantine catalogs without mixing license states.
7. Create the scale/readability yard and three act look labs.
8. Build one canonical scene from each act as a vertical slice.
9. Review the three vertical slices at Panzerrechnersystem resolution before starting custom hero modeling.
10. Export one approved scene block, place it in the Panzerrechnersystem editor, and compare Blender versus
    exact-nearest in-engine captures.

The immediate success condition is not “all dioramas finished.” It is a trustworthy private asset
library, one consistent scene-study format, three act-level visual-language studies, and one proven
Blender-to-Panzerrechnersystem round trip.
