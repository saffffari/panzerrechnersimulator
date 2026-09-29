# Historical environment workshops — 25 September 2026

Three independently editable Pizza-asset assemblies are ready for placement planning. They contain
908 placed pieces across a household refuge, a factory cellar ward with a separate doctor's cellar,
and the cold aftermath of burning stores at Gumrak. The buildings, dimensions, furniture, snow,
damage arrangement and local routes are reconstructions; none is a surveyed replica.

The finished [99-second showcase](../captures/astra/panzer_winter_showcase_20260925.mp4) includes the
three workshops, winter wrecks, physical reload, exposed-cupola P08, map zoom and the orange cabin
fire. It is 1080p/24fps with stereo game audio. Full decoding, representative frames, all capture
outcome checks and the final black/silent tail were verified. The edit receipt and eight chapter
sources are in `artifacts/showcase-20260925/showcase.receipt.json`.

## Open and edit

Close the current game normally, then run one of these from `/home/alex/panzer/shader-graphics`:

```sh
./tools/launch_historical_editor.sh household
./tools/launch_historical_editor.sh ward
./tools/launch_historical_editor.sh gumrak
```

The ordinary editor opens the latest saved workshop and its review camera. Each workshop has its own
live-edit directory under `artifacts/authoring/live-historical-<scene>-20260925`. The launcher uses the
same single-instance lock as the winter game. `--check` checks paths without launching.

Scene blocks are in `assets/worldgen/scene_blocks/historical_20260925/`. Versioned workshop manifests,
asset closure, review cameras and source hashes are indexed by
`artifacts/authoring/historical-20260925/workshops.json`. Original Pizza files are unchanged; charred
material variants retain source geometry and provenance sidecars. The current winter courtyard is
preserved. These three studies are separate places and dates, not a linked Tiger itinerary.

## Placed in the urban-intro district (29 September 2026)

By the owner's decision of 28 September the three scenes stand in the Act I urban-intro district
and beside its route west, as building-attached tableaux of `catalog/urban_intro_tableaux.v1.json`
(ranks 12-16), each with its sources, windows, relationships, inferred elements, reference
photographs and exclusions in its provenance. They move with their host buildings. The scene
blocks were read, not placed: their props are re-seated in the hosts' own rooms, and their shells,
ramps, steel frames and furniture that the hosts' rooms replace are left out (the catalog says
which). Copies of the three scene blocks sit beside the workshop assets for reference; placement
comes only from the tracked catalog and `tools/worldgen/urban_intro_district.py`.

- Household (rank 12): a burned two-storey merchant house on a courtyard (`urban_intro.c3r2.e05`).
  Its brick-vaulted ground floor held, so its cellar shelter is whole under the burned storeys;
  the charred timber lies on the ground floor's rubble. The courtyard trench is dug into the
  ground beside the house, with no connection to the cellar.
- Ward (rank 13) and the doctor's cellar (rank 14): two ruined houses' cellars 58 m apart. The
  ward's host is a residential block, against the scene catalog's rule of a factory cellar; the
  district has none, and the conflict is recorded in the place envelope.
- Gumrak (ranks 15 and 16): a burned store and a burned house the district tool builds on either
  side of the old road where it comes back to level ground 840 m from the spawn: a route
  composite, not Gumrak's plan.

Evidence: `captures/urban2_*` (inspect, fixed-exposure and review work-lamp views of every scene;
docs/URBAN_INTRO_20260928.md, "Cellars and scenes").

## What the sources support

| Assembly | Attested basis | Authored interpretation |
| --- | --- | --- |
| Household | Pozdnyakova's March 1943 interview describing a September 1942 burned home, cellar refuge, escape through house windows and a separate trench | A later winter remnant; the rooms, ramp, household objects and trench dimensions are inferred |
| Factory ward | Anonymous 305th Division corporal's diary, 15–18 December 1942, KP 25224: factory cellar, wounded, floor fire and a doctor located in another ruined house's cellar | Two separate accessible cellar shells, bedding, cold hearth and the distance between them are inferred |
| Gumrak stores | Anonymous 79th Division diary, late November 1942, KP 25221: houses, food and clothing burning at Gumrak | A cold storage-yard aftermath; cause, elapsed time, buildings, containers and traffic arrangement are inferred |

The separate Karpovka-road account of deliberate burning and ready weapons is excluded from the
Gumrak assembly. No named victims, invented bodies, unit insignia, exact casualty counts or personal
possessions are asserted. Read [the source audit](research/HISTORICAL_SCENE_SOURCE_AUDIT_20260925.md)
for source links, translation boundaries and corrections to earlier catalog shorthand.

## Engine support and evidence

Authored `mode: scene` workshops now suppress the legacy city, automatic story scatter, default
pickups, procedural grass and automatic combat population. The authored placements and ordinary
collision remain active. Campaign and combat-playtest population policies are unchanged.

Manifest floor/roof triangles provide local weather shelter: retained roof faces cover the player,
while holes, collapsed bays and outside approaches remain exposed. This uses the existing spatial
bins, not a building-sized box. The raster renderer suppresses camera-near snow to 16m and blends
back to unchanged weather by 22m; this is an explicit approximation, not per-flake ray tracing.
Source triangle tests and ordinary in-game ramp traversal are both
recorded in the authoring receipts and showcase capture manifests.

The showcase is current game rendering driven by scripted ordinary inputs with the game's own audio.
Starting positions, peaceful capture policy and fire ignition are staged and disclosed in its JSON
fixtures. It is not native mouse/keyboard play, archival film or a historical recreation of a specific
crew's final moments. Terminal crew death remains black and silent, with no attacker replay.
