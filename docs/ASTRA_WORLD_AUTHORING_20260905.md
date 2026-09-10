# Versioned world and scene authoring foundation

This pass adds a repository-local MCP stdio server, a shared Python revision/cook engine, a
separate Playtest map and actual runtime consumers for authored scene lights and sounds. The
original campaign worlds, edit overlays, saves and purchased source archive are preserved.
These are CPU-validated studies; an asset count or successful cook is not an in-game visual,
collision, traversal or historical-accuracy verdict.

## What was missing

`src/editor.rs` already supports terrain sculpting, grounded navigation, asset placement,
practicals, sound metadata, F7 scene capture, F5 save, F9 reload and F10 play-hop. There was no
current native editor MCP. `tools/pixelkpfw` is an image pixel/dither tool. The old MCP documents
describe a different Unreal bridge. Scene block sockets and source metadata existed, but a stamp
did not relocate audible emitters or working lights into gameplay.

The new MCP is an **authoring bridge**, not a live viewport RPC. It cannot inspect the current
mouse selection, move the open editor camera, render a preview, send messages or execute arbitrary
shell commands. Returned manifest paths are opened in the existing editor or selected at runtime.
The editor's normal overlay remains index-based; a new cook materializes a matching overlay into
stable placement identities and refuses stale bases. It does not guess how edits map onto a new
seed's unrelated buildings.

## Entry points

Run `python3 tools/worldgen/editor_mcp.py` from any directory. Example client configuration lives
in `tools/worldgen/mcp.example.json`; no user-wide MCP configuration was changed. The protocol is
newline-framed JSON-RPC over stdio, with protocol output only on stdout. It implements initialize,
ping, tools/list and tools/call for MCP 2025-06-18.

The eight tools are `pizza_asset_search`, `world_inspect`, `world_fork`, `scene_validate`,
`scene_cook`, `world_transaction`, `playtest_generate` and `act_regenerate`. Their structured results
include paths, actual source counts and validation state. The server accepts repository-contained
paths, finite bounded transforms and at most 4096 transaction operations. It exposes no file delete
or source-world overwrite operation. Failed publications stay in a `.pending_…` directory with a
failure receipt; only successful complete revisions receive their final directory name.

CLI equivalents:

```sh
python3 tools/worldgen/world_authoring.py inspect assets/worlds/astra-20260905/playtest/manifest.json
python3 tools/worldgen/world_authoring.py assets bed
python3 tools/worldgen/world_authoring.py fork assets/worldgen/act1/manifest.v1.json my-level
python3 tools/worldgen/world_authoring.py scene assets/worldgen/scene_blocks/room.scene-block.v1.json room-study --act 2
python3 tools/worldgen/world_authoring.py regenerate assets/worldgen/act2/manifest.v1.json act2-study --seed 20260907
```

`world_transaction` supports place/move/remove by stable ID, sculpt, stamp_scene and sound/light
upserts. Clients must submit the current revision hash, which includes manifest, terrain file and
adjacent edit content. A failed late operation publishes no partial edit. An unchanged placement
retains its ID after unrelated deletion or ordering changes. Legacy placement IDs are derived from
asset+transform+duplicate ordinal; they do not promise correspondence between independently
regenerated seeds. Existing terrain-overlay instances require explicit baking before a new sculpt
transaction; ordinary forks preserve them as-is.

## World mode and scene mode

Full level regeneration keeps the selected source macro-heightfield and navigation markers,
adds small authored relief, grades connecting roads, and builds suitable Pizza modular districts
and sparse bare groves. Each output is explicitly noncanonical. New layouts retain the existing
ground drape, so its old road/field class pattern needs a later matching material cook. Act III's
source is a Halbe prototype; this operation does not establish the canonical Berlin/Reichstag
finale or turn generic marker names into verified locations.

Scene mode uses one rigid local +Y-up datum. Terrain, props, point lights, sound emitters and
first-person sockets share the same translation/yaw. Historical metadata retains source IDs/URLs,
placement class and constraints. Exact-landmark campaign stamps require the declared act/position
and compatible source tags. Standalone or Playtest relocation is explicitly labelled diagnostic,
so a local study is not presented as the historical place. F7 now captures practical/sound sources
inside the captured terrain core; scene stamps namespace their IDs so repeated rooms are separate
sources. The scene palette discovers versioned subdirectories.

Optional scene `environment.sound_emitters` follows the existing spatial-audio record schema.
`environment.authored_lights` currently permits point sources only: spot and area entries are
rejected rather than silently drawn as a different light. The JSON Schema is still scene-block v1
with an additive optional environment field and optional stable item IDs.

Runtime sound uses ordinary Audio sinks, so live play and offline first-person video share the
same source graph. Source coordinates stay fixed while listener position and heading update stereo
gains. Direct obstruction uses gameplay line-of-sight plus a low-pass; it is not a measured room
impulse response, diffraction solver or a claimed reconstruction of witness acoustics. Limits are
16 active voices, 16 MiB decoded mono cache, 2 million frames per clip and one new clip decode per
update. Long continuous environments should use short authored loop recordings. Muting, switching
worlds and restarting the capture graph stop old voices. Point sources join ordinary surface and
volume lighting with normal visibility, without camera-dependent fill.

## Published Playtest

The menu default is `assets/worlds/astra-20260905/playtest/manifest.json`, installed once from the
immutable revision `assets/worldgen/revisions/playtest/20260905T114300836114Z_ab7bd740/manifest.v1.json`.
Later cooker runs never replace that default automatically. The runtime selector API is
`act_manifest::select_runtime_manifest(Some(path))`; clearing it restores the ordinary act paths.

The map spans 768 m and contains 619 placements from 32 actual assets: a garage, warehouse, shed,
five modular ruins, 70 branch-only deciduous trees, and contextual rocks/stumps/logs/ferns/fences.
Two derived bare tree GLBs exclude the source's named leaf child from the scene graph while
preserving all source binary geometry and the original archive. Actual source RGB from three
preserved Pizza ground tiles is blended in linear light into road soil and grove litter. The old
generator's active texture paths were absent; the preserved archive was used read-only.

Spawn is `(0,-0.3884,180)`, facing north. Target lanes use X -110/0/110 at Z -180/-240/-300 with earth
berms behind them. Opponents come from ordinary Game systems, not decorative tank models.
Garage first-person entry is `(-67,1.5788,99)`, warehouse `(-112,1.4325,41)`; both have authored
doorways, floor modules and a clear center aisle. Their exact rendered collision/traversal still
requires the real first-person capture. Ground cover uses the existing runtime grass path; its
solid stalks render primary G-buffer depth but remain absent from RT secondary visibility and
grass self-shadowing. No frame-visible grass density is claimed from this cook alone.

Three separate full-act study revisions were also cooked: 5323/2803/2735 placements, each with
29 unique assets. Exact paths and hashes are in
`artifacts/astra-audit-20260905/world-authoring-cook-receipt.json`. Their fewer placements are a
bounded new-layout prototype, not a claim to supersede the current campaign's larger dressing.

## Checks

`python3 -m unittest tools/worldgen/test_world_authoring.py -v`: 12 passing tests covering source
preservation, stable IDs, terrain conflict detection, partial-operation rollback, rigid prop/source
placement, historical anchor protection, unsafe paths and point-light contract. A real subprocess
stdio initialization/tools-list test returned 8 tools and no stdout contamination. Playtest and all
three studies passed the bounded Python manifest/terrain/source reader. New Rust tests cover
scene-source relocation, moving-listener stereo and loop/one-shot sample-clock behavior; integrated
Cargo status and visual/audio capture verification belong to the root's coordinated build report.

## Standalone service mechanics

Playtest owns an optional 420 L tank-fuel value, with no dummy campaign objective runtime. Both
campaign and Playtest use the same existing authored burn law and content-conserving can transfer.
The engine stops and cannot restart on empty. Four allocated physical fuel cans are seeded at the
Tiger rear, garage, warehouse and shed. Ordinary E pickup/transfer/drop and throwing remain the
interaction path; empty and partially filled cans persist.

`Game::debug_stage_playtest_refuel` is an explicitly labelled review fixture. It parks the Tiger,
sets review fuel to 400 L, grounds the foot player and one full 20 L can at the rear, and returns
the staged positions. It executes no interaction. A first-person capture must perform ordinary E
actions and inspect `tank_fuel_liters` to establish the actual 400→420 L result.

Local scene and Playtest manifests also suppress the hard-coded Act II campaign channel/deck
movement gate. Their climate ID alone no longer invents a waterway absent from their terrain.
Campaign/full-level Act II keeps its original navigation rule. Player tank, foot actor, allies,
enemies and post-collision regrounding use the same authored-mode flag.

The first integrated Rust preflight exposed a missing `ops: []` in new empty edit files. The cooker
now emits that required array and its reader rejects the omission. A regression covers the exact
contract. Corrected versions were published without changing geometry or source assets; the prior
invalid revisions and default Playtest directory are preserved. The consolidated receipt records
all superseded paths.

Five executable world cases now live under `assets/review/first_person/`: Playtest refueling, garage
traversal, warehouse traversal, grove movement and the sourced cellar descent. Optional integer
`expect[].at_s` checks evaluate 1 Hz state checkpoints; absolute ranges can verify an interior entry
and the later back-wall stop. The traversal uses an actual doorway center 2 m left of the building
center: paired door modules retain a central solid jamb and are not a single wide opening.
