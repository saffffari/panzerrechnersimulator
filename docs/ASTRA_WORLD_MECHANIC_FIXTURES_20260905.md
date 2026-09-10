# First-person world mechanic fixtures — 2026-09-05

Five new scenarios use the ordinary Game first-person camera, input path and audio mixer. They are scripted review fixtures, not approved recordings. Static JSON/action/timing and versioned world asset-closure checks passed; the receipt is `artifacts/astra-audit-20260905/world-mechanic-fixtures-preflight.json`. Runtime, visual and listening review remain separate.

| Scenario in `assets/review/first_person/` | Actual interaction and evidence | Scope limit |
| --- | --- | --- |
| `fp_playtest_can_place_throw_empty.json` | E picks up the actual can, transfers its 20 L, places it, recovers it, then ordinary throw releases the empty 4.5 kg prop. Check its original slot, airborne speed and eventual grounded zero velocity. | Setup discloses 400 L in the tank and an actual reachable rear can. No scripted velocity. Persistence is within this continuous session, not save/reload or complete obstacle collision. |
| `fp_playtest_can_throw_full.json` | Ordinary pickup and throw preserve all 20 L and the actual 19.3 kg mass. Compare its launch speed with the empty case. | Same source prop and throw implementation, with no assigned motion. |
| `fp_snow_shelter_transition.json` | Walk through the actual compiled Act-I building doorway and back; record precipitation and geometric shelter state. | Act-I standalone world is used because the Act-II Playtest shells do not implement this live damaged-roof shelter query. Snow visibility/occlusion still needs the video. |
| `fp_feature_shelf_search.json` | Ordinary contextual R searches the actual reachable shelf. Record empty/searchable state and exact per-kind supplies, then repeat R. | Setup never opens, searches, grants or rerolls loot. Positive transfer and emptied state are automatic assertions; compare 2-second and final inventories before approving no-duplicate coverage. No save/reload in this clip. |
| `fp_playtest_authored_sources.json` | Walk past two actual placed emitters and lights, then return. Check decoding failures, once-only trigger counts, moving-listener gains, static source coordinates and retained lights. | Explicit calibration composite, not a historical soundscape or a naturally occurring hammer strike. Audibility, stereo placement, fixture appearance and exposure need actual media review. |

The isolated `src/scene_environment_readout.rs` is a child of `scene_environment`. `Game::world_mechanic_status(&Renderer)` reports existing state only. `debug_stage_feature_building("entry" | "shelf")` places the actor at an actual building-relative anchor after generic warmup, then uses the normal update/camera/input path. Shelf feet are at `world_center.y + 0.1`, matching the compiled ground floor; reach and structural line of sight are checked before placement. The entry feet are grounded against the actual heightfield. The helper does not modify roofs, collision, damage, container state, weather or supplies.

The source calibration lane was cooked non-destructively by `tools/worldgen/cook_review_sources.py`. Its immutable manifest is:

`assets/worldgen/revisions/playtest-source-review/20260905T123108169822Z_d02466f3/manifest.v1.json`

`artifacts/astra-audit-20260905/source-placement-review-cook.json` records operations, asset bounds, source hashes and exact source coordinates. Two owned Pizza `speaker_mx_1` models sit on bounded `metal_barrel_hr_1` props; two `lamp_mx_1_a_off` models accompany explicit point-light records. Lamp fixture support/appearance remains an art-review item. Each speaker plays the real `assets/sounds/hammer_strike.ogg` once on physical entry. The cue is intentionally disclosed test audio. It does not fill the outstanding need to select verified source-appropriate recordings for historical scenes.

The 54-row coverage ledger now distinguishes the verified v6 simulation-state receipts for map pencil/pins, separate roof lids, actual 400→420 L refueling, garage/warehouse contacts and cellar descent from visual/audio approval. The earlier dark Playtest videos remain rejected evidence. The v7 daylight proof approves its own lighting readability only and confers no traversal approval.
