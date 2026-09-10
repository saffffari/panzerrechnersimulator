# Infantry stance, source recovery and v9 contract

The German NPC now has ten static meshes evaluated from the original CC0 weighted rig: five
standing-to-crouch stages each with relaxed or raised arms. The 0.4-second transition selects the
same discrete stage for rendering, eye position, gameplay body height, speed and rifle dispersion.
Training imports this same renderer-free implementation. Existing frozen-v8 training predates this
capability change and needs transfer evaluation; its scores are not v9 gameplay validation.

Source: **nisu**, [Rigged lowpoly WW2 soldier](https://opengameart.org/content/rigged-lowpoly-ww2-soldier),
CC0-1.0. Original `assets/flavor/Characters/german-ww2-soldier-cc0/source/LowpolySoldier.fbx`
and texture are unchanged. New derivatives live only in `baked/infantry_crouch_v1/`; active old
capture binaries cannot discover them through their old asset paths. The manifest records hashes,
evaluated joints, bounds, head landmarks and deformation checks for every frame.

`tools/bake_german_infantry_crouch.py` uses a headless factory-startup Blender process with two
CPU threads and no rendering. The original weighted skeleton bends both knees forward using
evaluated pole-plane calibration, preserves skin volume, holds ankle targets and foot rotations,
and lowers the pelvis by 0.65 m while moving it backward 0.22 m. One standing-derived scale and
ground datum apply throughout. Earlier experimental outputs in this new derivative folder were
replaced after checks exposed FBX rest compensation and sideways knee rolls; original assets were
not changed. Neither runtime body sinking nor a height scale simulates the bent legs.

| Stage | Body top, m | Head midpoint eye proxy, m | Head forward offset, m |
|---|---:|---:|---:|
| 0 | 1.7500 | 1.647875 | 0.006202 |
| 1 | 1.5875 | 1.485375 | -0.048798 |
| 2 | 1.4250 | 1.322875 | -0.103798 |
| 3 | 1.2625 | 1.160375 | -0.158798 |
| 4 | 1.1000 | 0.997875 | -0.213798 |

Both arm poses have these same values. The eye proxy is the midpoint of the evaluated source
Head bone, not an anatomically calibrated eye landmark. Rifle target memory records only the
last *visible* torso offset and body height; a hidden posture or direction change cannot update
the remembered aim point. Shared blast checks and tank auxiliary aim follow the posed torso.
The horizontal body collider remains the existing broad 0.5 m AABB; this work does not claim
triangle-accurate human injury geometry. Locomotion and the separate simple rifle still need
dedicated animation review; the German corpse continues to use the existing prone fallback.

The selected Soviet asset is **Michael Jones (mikhailgolubkov09)**,
[WW2 Soviet Soldier](https://sketchfab.com/3d-models/ww2-soviet-soldier-331f36489ce440549dc1a6acb82fd931),
CC-BY-4.0. The source and all 17 expected baked frames are absent from this checkout. Scoped
archive/member and transfer-manifest discovery found no recoverable copy; the receipt is
`artifacts/astra-audit-20260905/soviet-source-recovery-search.json`. The exact public page was
verified through normal Chrome UI, including its CC-BY attribution and Original GLB 1 MB button.
Clicking that official download reached Chrome's `sketchfab-prod-media.s3.amazonaws.com is blocked`
page with `ERR_BLOCKED_BY_CLIENT`. No block was bypassed and no original file was recovered.
The imported Soviet model is **not integrated**. Until a source rig/derivative exists, the actual
rigid Soviet fallback uses `StandingOnly`: a requested crouch is recorded but does not grant an
invisible smaller body. The human foot actor retains its explicit existing `InstantCrouch` adapter.

Validation: nine standalone shared infantry tests pass, including retained rifle/machine-gun
regressions, five-stage transitions at 20/120/240 Hz, no root sinking, visible posture memory,
and actual hitscan/blast cover behavior. All ten derivative skin, floor, ankle and knee-plane
checks pass. `captures/astra/german_crouch_cpu_geometry_v9.png` is an actual-mesh front/side CPU
inspection, showing bent knees and planted feet. It is not a game render. The new full loader/head
marker Cargo test and first-person gameplay video remain to be run by the coordinated root gate.

Runtime telemetry now distinguishes `crouch_requested`, `crouch_phase`, `crouch_stage`, effective
`crouched`, `pose_support`, `body_height_m`, `eye`, and the last observed body height. If the entire
German library fails validation, the runtime uses the old standing mesh and `StandingOnly`, rather
than granting unsupported posture. Complete shipped assets are required for training/runtime parity.

## User-supplied Soviet source recovery, September 5

The later user-supplied `/home/alex/Downloads/ww2-soviet-soldier.zip` resolves the missing-source
condition described above. Archive SHA-256 is
`384ec50bbc8876423c4c5b3355b32cc1f274b0a03f75a4f43d167dbb00943784`.
Its original `source/soldier.glb` and `textures/WithAo_0.png` are retained unchanged under
`assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/imports/20260905-user-384ec50b/`.
No archive scripts were present or executed. The ZIP has no license file; the exact original
Sketchfab page was independently inspected through normal Chrome UI and identifies **Michael
Jones (@mikhailgolubkov09)** with an explicit **CC-BY-4.0** link. The source page, attribution,
original archive/member hashes and verification distinction are recorded in
`artifacts/astra-audit-20260905/soviet-user-import-20260905.json`.

The original is a textured A-pose mesh with 1,996 triangles and no skeleton or animation.
The existing CPU-only pipeline refits the preserved German donor skeleton to this source,
transfers weights, and exports the 49-bone authoring master plus all 17 runtime poses. The active
new derivative root is
`assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/versions/20260905-user-384ec50b-h175/`.
The preceding 1.72 m trial derivative is also preserved; the active bake uses 1.75 m to agree
with the standing combat proxy. Rest/aim body heights agree; ordinary gait has authored bob and
foot motion, so the existing broad body AABB is not an exact per-pose anatomical collider.
No crouch frames exist in this 17-pose library: the Soviet actor correctly retains StandingOnly
capability rather than receiving an invisible crouched hitbox. German crouch is unaffected.

All 17 local skin-deformation checks and the 23-file bake receipt pass. Before/after binding
fingerprints verify original source positions, topology, UVs and material slots remain unchanged;
maximum rest displacement is below one micrometre. Static geometry, not runtime skeletal
playback, is still the game animation authority. A small procedural rifle is included in each
frame and is explicitly identified as game-generated in the bake receipt.

`src/soviet_animation.rs` now names the versioned paths. Its transactional runtime loader requires
all 17 meshes and the same atlas, valid finite geometry, and the exact audited oriented triangle
UV fingerprint. The exporter legitimately varies normal-seam split vertices, so the incorrect
vertex-count equality was replaced with a surface validation that retains winding and face
multiplicity. See `artifacts/astra-audit-20260905/soviet-user-runtime-surface-20260905.json`.
The new actual-asset Cargo test verifies all 17 texture-bearing poses, standing/aim height and
fallen final-death geometry. Coordinated Cargo and first-person GPU proof remain required before
claiming the model is visibly verified in the game.
