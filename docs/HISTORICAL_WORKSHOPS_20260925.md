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
