# Integration log

`integration-1` is the rolling integration branch; `main` is the main build and is fast-forwarded to
an `integration-1` merge only when that merge builds (`cargo build --release --locked --bin
panzerrechnersystem`) and passes the unit tests (`cargo test --release --locked --bin
panzerrechnersystem`, run from the asset checkout so tests that read `assets/` find them). The full
gate also has a headless smoke (one raymarch `screenshot` and a short autonomous Canyon pilot run);
while the GPU is on hold (owner, 2026-10-27 evening: KSP and other work on the box) the smoke is
recorded as **pending GPU window** and run in one batch when the hold lifts. `raymarching` is the
base of everything and is merged first each round from `origin/raymarching`. Per-round performance
lives in `docs/MILESTONE_1_PERF_20261027.md`.

One line per merge: branch @ commit — conflicts — build / tests / smoke — `main`.

## Round 0 — 2026-10-27 (integration-1 created from raymarching 1759f33)

- run-loop @ 6b37c06 (local tip, then unpushed) — none — build ok / 52 module tests ok / smoke n/a — not advanced (main was out of scope then)
- claude/canyon-wreckage-20261027 @ 413ee0c — none — ok / ok / n/a — not advanced
- claude/spatial-audio-20261027 @ 272e358 — `docs/PHYSICAL_TRUTH_AUDIT.md` "Updated:" line (unioned) — ok / ok / n/a — not advanced
- missions @ 718133a — ledger header (unioned); `src/canyon_pilot.rs` ×3: missions' retry tail on the old timed reload dropped for run-loop's stage-machine reload, missions' "no engage with the lever open" kept — ok / ok / n/a — not advanced
- platoon @ 7f14c2d — ledger header + section (both sections kept); `src/main.rs` ×2 (module list, subcommand table unioned) — ok / ok / n/a — not advanced
- integration-1 fixes 3f1c693..da1789e: autopilot telemetry runs, `-Logged` launcher, sim-phase timers, and the Canyon pilot fixes. **The hamlet stall the missions and run-loop sessions both hit was the Tiger jamming on its own knocked-out T-34s in the 20 m street, not the wreckage props** (all 300 hard wreckage bodies are ≥ 13 m from the route); fixed by the hull bypass (83946a7), then slot skip / reload abandon (ae6ad8c), hull escape (7ea14f7), live hulls (7804cf8), escape commit (da1789e). Report 516b9fd.

## Round 1 — 2026-10-27

- raymarching @ 36efd5f (8 new: near-camera washout fix + RT pack guard, Tiger snow, internal-resolution grids, ray-query occlusion/reflections, wear and grime, cloud shadows/fog banks, lamp shadow rays, light shafts) — `tools/launch_windows.ps1`: both sides added a grid switch; kept raymarching's `-Resolution` (it also writes the saved preferences, which otherwise override `KESSEL_INTERNAL_RES` — integration's `-InternalRes` was silently ineffective) with `-InternalRes` as its alias; kept `-NoVsync` after it — merged in 24308ea
- run-loop @ dc68d84 (docs: CANYON_PILOT_FULLRUN) — none — merged in 24d7780
- missions @ 9f50627 — none — merged in 571a687
- platoon @ 4b6d5f3 — none — merged in c096fa9; its added "Platoon" title row broke `ui::pause_menu_tests::fixed_profile_removes_only_graphics_routes_and_preserves_setting_actions` (8 ≠ 7; the branch fails it alone too) → fixed forward in 4faa0c4
- **Gate for 4faa0c4**: build ok (`--locked`, warnings only); unit tests: 1,890 passed / 89 failed / 61 ignored, failure set **identical** to `origin/raymarching` 36efd5f run the same way (1,861 / 89 / 61) — the 89 are tests that read git-ignored assets from a worktree; from the asset checkout with `KESSEL_ASSET_ROOT` 14 remain, all pre-existing on the base (campaign_save ×6, capture paths, resource paths, asset catalog/library, equipment gaze, mechanic showcase ×2, atmosphere eye) — no new failures; smoke **pending GPU window** (owner's hold) — `main` **advanced** to 4faa0c4
