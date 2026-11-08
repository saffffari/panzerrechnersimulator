# Integration audit — 2026-11-07

Hermes AI performed a read-only branch, worktree and source audit. This report describes the pre-integration state at native HEAD `9fc6553`; the integration owner created `codex/integration-20261107` while this audit ran. Test/build results produced by the integration owner supersede the validation limits below.

## Decision

The current native work can be preserved as a reviewed baseline, but “merge every branch” would restore several superseded or explicitly unfinished experiments. The principal missing recent deliverable is the **latest complete web tree**, including its uncommitted native-model rebuild. A baseline commit is not evidence that the editor sustains 60 FPS or that the engine overhaul is complete.

At audit time the native working tree had **48 modified tracked files and 22 untracked files**, with **1,941 additions and 786 deletions** in the tracked diff. `git diff --check` passed. No audit builds or renders were launched; the integration owner is running fresh checks and the same-window WORLD probe.

## Changes requiring integration or an explicit disposition

| Branch / worktree | Native-only / other-only commits | Disposition |
| --- | ---: | --- |
| `codex/hermes-web-playground` | 2 / 4 | Integrate the four commits below, then its complete dirty source and native assets. This is the only active web implementation. |
| `claude/fea-rewind-20261103` | 62 / 1 | `f498846` removes a cookoff/turret reset after restoring a rewind checkpoint. Current fresh playback deliberately invalidates that checkpoint, so the old patch is superseded in the current path. Do not apply blindly. |
| `panzerrechnersimulator_entfernungsmesser-sonderausgabe` | 126 / 2 | Preserve `457f7fc` and `db5b3ce` as a separate laptop packaging/performance edition. They touch main/range/graphics/resource and need deliberate porting; not a missing native feature fix. |
| `worktree-agent-a4c16a0a98ea5b00b` | 206 / 1 | Preserve `9ca453d`; commit explicitly says blood-volume WIP, unfinished and uncompiled. Do not merge into the verified baseline. |
| `claude/fea-armour-parts-wip` | divergent | Preserve `f39f471`; explicitly uncompiled drawn-part armor/material model. It is a separate modeling change, not a safe pending fix. |
| `claude/windows-wip-20261101` | 26 / 2 | No novel non-merge patch according to `git cherry`; retain the branch. |
| `editor-stable/checkout` detached `e13048b` | 13 / 13 | All non-merge commits patch-equivalent. Dirty installed mirror is not an independent feature branch; do not copy it over current source. |

The four web commits are:

- `5448670` — isolated browser playground and native preset loader.
- `332325d` — native shot lists and round indices.
- `4736765` — Canyon play mode, squad AI, native preset endpoint.
- `52b7d10` — cursor-ray sphere aiming and coarse damage presentation.

They add only `web/`. The web worktree also has **10 modified tracked and 24 untracked files**. Those changes include `native-tiger.ts`, ring/time code and tests, asset preparation/review scripts, native GLBs, native interior manifests, and the CC BY 4.0 crew attribution. The hull GLB is about 10 MiB, with the remaining native assets about 3 MiB. `git diff --check` passed there. Run `npm test` and `npm run build` on the integrated snapshot; neither proves visual/editor parity by itself.

Safe integration sequence: merge the committed web branch into the integration branch; copy its current changed tracked and untracked source/assets onto that result without altering the source worktree; exclude `node_modules/`, `dist/`, local build state and caches; review machine-local QA reports separately. Then run checks and commit the complete native plus web baseline. Preserve the original worktrees until the integrated output is verified.

## Other dirty worktrees

| Worktree | Dirty files at audit | What is actually there |
| --- | ---: | --- |
| `armored-calculator` | 8 tracked + 5 untracked | Unfinished explicit offline calculator/timeline/queue. It raises plate reference resolution to 128,000 particles and changes the lower particle cap, and expands cache allowance from 4 to 64 GiB. Blind merging would undo current interactive limits. Preserve for deliberate offline-port review. |
| `fea-damage` | 2 tracked | Older parallel precomputed fragment paths and a fixed 100 ms delayed tissue commit. These contradict the subsequent continuous temporal collision model; preserve but do not restore. |
| `fx-fire-sparks` | 1 tracked | Novel exposed-envelope fire-power correction and test. It constructs a voxel HashSet in the aggregate path; useful lighting experiment, but measure overhead and compare current lighting before applying. The current runtime does not contain `fire_envelope_share`. |
| `sky-daynight-base` | 7 staged | A comparison rollback removing 398 lines of physical-sky/moon functionality, including earthshine and disc changes. Not a pending forward feature. Preserve staged experiment. |
| `editor-stable/checkout` | 45 tracked + 19 untracked | Installed mirror with equivalent ancestry; compare/install from the integration result, never use this as the source of truth. |
| `demo-20261101` | 15 untracked | No tracked changes; inspect/preserve local generated files independently of integration. |
| `t34-eyes-20261102` | 3 untracked | No tracked changes; retain local files. |
| `tank_simulator` | 2 tracked + 3,547 untracked | Old September checkout with main/lock changes and many dated artifacts. Not a current gameplay branch to merge wholesale. |

`cabin-panels`, `fea-ai-videos`, `fea-range`, `periscope-optics`, `shot-rings-presets`, `sky-daynight`, `tissue-solid-fracture`, `fx-build-tmp`, and the locked `agent-a8ab3c507f3f814d3` worktree are clean and their tips are already ancestors of the current native base. Archived historical divergent branches were not treated as pending work.

## WORLD/maps correctness findings

1. **Look transfer is incomplete.** `EditorApp::attach` and `take_range_return` copy look selector, grade, dither pattern, fog and parts of sky state, but omit the complete `display_tuning` and the `master_retro` control. The WORLD Render panel changes `renderer.master_retro` directly; returning to RANGE drops that change. RANGE palette/ordered-dither/white-balance tuning also is not imported into WORLD. This blocks claiming identical look tuning across mode switches. Use one shared import/export representation, including the tuned flag and retro behavior. The Render panel exposure row is in `display_grade` and *is* copied; the separate sky exposure override is not.
2. **Map initialization is not transactional.** Selection validates the manifest JSON, then drops the old GPU world and persists the new selected path before `Range::new` loads all dependent assets. If a manifest exists but an asset is broken, initialization exits and leaves the bad selected path for the next launch. Keep a last-known-good selection or roll back on load failure. The WORLD-attach failure path does attempt a RANGE fallback, but that is insufficient for a broken map payload.
3. **Bridge payload is locally installed asset state.** The selector hardcodes `assets/worldgen/workshops/bridge-look-editor-20261105/manifest.v1.json`. Verify the asset payload is present in the intended installation/package; a source commit alone does not deliver that map. Missing selection is reported by `request_map`, but a stale persisted selection can still prevent startup.
4. **Per-map state isolation is largely sound.** A foreign map state removes camera, Tiger placement, sky, battle, section, snow and wear; its key names match the actual serializer. Look/fog and shots deliberately remain shared. Check a visually clear initial bridge view because inherited fog is not reset.
5. **Terrain-only save was implemented.** `save_document` captures dirty terrain without requiring selected props, and scene-block loading now accepts empty props when terrain exists. The end-to-end save/reload probe must still verify this behavior on the current build.

## Performance and validation boundaries

### Follow-up fixes after the audit

The integration owner authorized Hermes to fix findings 1 and 2. `EditorSessionLook` now carries the full display profile, internal dimensions, master Retro, grade/dither/look and fixed camera exposure through both directions, with support for old flat presets. An unsupported resolution/profile is rejected before renderer targets are modified. The WORLD Retro row now changes the active profile and persists that edit. RANGE applies resolution changes through the renderer's complete target-graph rebuild, which retains scene/UI registrations.

Map selection is now written only after RANGE successfully builds. A failed pending map can return once to the last successful selection; it cannot recursively retry that same failed map. On startup, a missing/invalid persisted manifest falls back to Canyon. Manifest validation already checks terrain and asset references; a later runtime construction failure is the additional recovery case.

Four pure display-transfer regression tests and one map-recovery regression test were added. They still require the integration owner's fresh test build. `git diff --check` passed after these fixes. The owner's earlier 29-test pass and same-window smoke predate this follow-up and are not presented as verification of these new edits.

The pending native changes distinguish interactive and reference tissue resolution, suppress repeated presentation work during catch-up, and add particle broadphase/cache work. These are plausible reductions, not an established frame budget. Passing shader/unit/build checks and offscreen readback timings cannot establish locked 60 FPS. Record live frame percentiles and stalls against 16.67 ms; treat a 33.33 ms frame as a missed requirement.

Keep the baseline source commit separate from the next overhaul commit. A sparse chunk storage/local destruction prototype and bounded dirty-remesh stress harness can provide a measured starting point; do not claim a full runtime replacement until the gameplay/renderer integration and representative live scenes are actually measured.
