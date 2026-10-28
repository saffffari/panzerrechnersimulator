# Milestone 1 — integrated build, performance and gameplay log (27 October 2026)

Branch `integration-1` (worktree `C:\shop\panzerrechnersystem-integration`), cut from `raymarching`
1759f33 and merged with every branch that was pushed and reported done. The playtest is the engine's
own telemetry: `KESSEL_SESSION_LOG` (one JSON line per second, src/session_log.rs) with
`KESSEL_GPU_PROFILE=1` (one fenced frame every 2 s with per-pass GPU timestamps), the console log,
a 5 s process-memory sampler, and `gfxbench 240`. Every number below is read from a file under
`artifacts/milestone-1-20261027/` by `tools/analyze_session_log.py`; nothing is estimated.

**Read this first — the measurement conditions were contended.** For the first two hours the
RTX 4090 sat at 99–100 % utilisation and ~22 GB of 24 GB VRAM with seven other
Panzerrechnersystem processes from sibling sessions (renderer, terrain, scatter, crew-solo,
armor-scars, gallery, missions, run-loop) plus Kerbal Space Program; system RAM touched 0 GB free
once (one sibling process held 36–43 GB). Later runs (A, B, D) shared the GPU with KSP and one to
three sibling capture processes. `nvidia-smi` was never idle, so no figure here is a clean solo
figure: GPU pass times are fenced (they time this process's work, but the GPU was time-sliced
with the others), and frame-wall percentiles include waits for a GPU the game did not own. CPU
simulation and render-submission times are this process's own threads and are the most
trustworthy numbers in the report. A clean re-run on an idle box is the first follow-up.

## At a glance

- **Build**: `integration-1` = `raymarching` + `run-loop` (incl. the unpushed pilot commit 6b37c06)
  + `claude/canyon-wreckage-20261027` + `claude/spatial-audio-20261027` + `missions` + `platoon`.
  `crew-solo`, `scatter`, `terrain-detail`, `armor-scars` skipped (not pushed, sessions still working).
  Release build `--locked` clean; 52 unit tests in the touched modules pass.
- **The hamlet stall was not the wreckage props.** The Tiger jammed on the T-34s it had just
  knocked out in the 20 m hamlet street, and the old detour ladder swung it into the frontage. All
  300 hard wreckage bodies lie ≥ 13 m from the route line. Fixed in the pilot (hull bypass on the
  street floor grid); with wrecks present the pilot now **completes the full course** in both
  invincible headless proofs (seed 137 in 23:46, seed 29 in 30:07).
- **Mortal windowed runs (the instrumented playtest)**: six full telemetry runs on the raymarch
  backend, autopilot driving, session log on. The final pilot (run D, seed 29) reached **84 % in
  28:56** before its lone crewman died on a non-penetrating hit; run B (seed 7) burned out at 45 %
  from one penetration; runs A, C, A0, 1 each exposed a pilot deadlock (empty ready rack, hull
  pile-ups, a live T-34 unseen in the street, pivot dithering) that the following commit fixed
  (run D, same seed 29, drove through the spots where runs 1, A0 and A stalled; the fix for run
  C's seed-101 deadlock was not re-run on that seed).
  **No mortal run completed the course.**
- **Performance in the canyon at the default profile (800×450, raymarch)** when the GPU was not
  being fought over (runs A, B, D): **56.7–58.8 fps mean, frame p50 16.6–16.7 ms (the display
  cap), p95-of-p95 25–29 ms, p99-of-p99 39–47 ms**. CPU simulation 8.6–9.1 ms mean (14 ms with an
  enemy inside 150 m), render submission 4.5–4.7 ms; fenced GPU pass sum median **7.0–7.7 ms**.
  Runs 0 and 1, taken while seven sibling game processes shared the 4090, fell to 36–39 fps on
  GPU waits alone.
- **Top optimization target, by a wide margin: the T-34 street-order planner**
  (`winter_encounter_ai` → `street_nav::Leg::plan`), 5.6–8.5 ms per frame in contact — more than
  the whole GPU frame. Second: shell-impact resolution inside `advance_projectiles`, which
  freezes the picture for 140–500 ms in a single step at the moment of a hit. Then the FX update
  (~2 ms constant), the sky cache (`rm-sky`, ~2.3 ms, 30 %
  of the GPU frame, resolution-independent), the G-buffer (geometry-bound). Above 900p the froxel
  volume takes over (14 ms at 1440p).
- **Gameplay**: contact at 12–30 s, first shot at ~31–34 s, reload 6.1–6.2 s median, 7–16 kills
  per run, 2–12 hits taken with 1 penetration in ~6,500 s; the loader cannot reach the hull bins
  once the ready racks are empty (~28 rounds in), so long runs end unarmed.
- **Not done**: the 10-minute human run (no human in this session); a clean, uncontended
  benchmark (the GPU was never idle — every figure is labelled with what else was running).

## What is in the build

| Order | Branch (tip) | Status at merge | Conflicts |
|---|---|---|---|
| base | `raymarching` 1759f33 | pushed | — |
| 1 | `run-loop` 6b37c06 (local tip; origin/run-loop is 64a20a9, one commit behind) | the pilot-fix session's prompt-gated reload commit was committed but not pushed; taken from the shared repo | none |
| 2 | `claude/canyon-wreckage-20261027` 413ee0c | pushed, done | none (the wreck layer itself is the git-ignored workshop overlay `manifest_edits.json`, 2,343 ops) |
| 3 | `claude/spatial-audio-20261027` 272e358 | pushed, done | `docs/PHYSICAL_TRUTH_AUDIT.md` header |
| 4 | `missions` 718133a | pushed, done | ledger header; `src/canyon_pilot.rs` (3 hunks) |
| 5 | `platoon` 7f14c2d | pushed, done | ledger header + section; `src/main.rs` (2 hunks) |
| — | `crew-solo`, `scatter`, `terrain-detail`, `armor-scars` | **skipped**: none is on origin and each owning session was still working | — |

Merge log. Five merges, four with conflicts, all textual. The physical-truth ledger's single
"Updated:" line conflicted three times (spatial audio, missions, platoon each prepended their
entry); each time the line was rebuilt to carry every branch's entry, and the platoon and
run-loop ledger sections, which both landed at the top, were both kept (platoon first). In
`src/canyon_pilot.rs` the missions branch had patched a three-retry tail onto the old
time-scripted reload (`reload_retries`, retry from 11 s), while run-loop's 6b37c06 had replaced
that reload with a prompt-gated stage machine that has its own whole-cycle retry and
`reload_stalled` abandon; the three hunks were resolved to run-loop's stage machine (the
missions retry has nothing left to attach to), and missions' non-conflicting change — do not stop
to engage while the breech lever is still open after an abandoned reload — merged automatically
and was kept. In `src/main.rs` the module list (`mission`, `mission_capture` vs `radio_llm`,
`platoon_proof`) and the subcommand table (`mission-captures` vs `platoon-proof`) were unioned;
the `mission-captures` call needed its closing `);` / `}` restored ahead of `platoon-proof`.
Nothing else conflicted; `cargo build --release --locked` passed after the last merge with
warnings only, and the 52 unit tests matching canyon/mission/platoon/radio_llm/graze/
acoustics_spatial/session pass (one, `canyon_props_bound_rotated_contact_expansion_and_exclude_casualties`,
loads `assets/authoring/canyon-v2/*.glb` by relative path and therefore only passes when run from
the asset checkout; it does).

## The hamlet stall: cause and fix

The missions session saw the canyon pilot stall at 16–17 % of the course (the hamlet frontage)
with the wreckage layer present and pass without it, and blamed the props. Re-run on the merged
build, the blocking geometry is not a wreckage prop. Seed 29, run 0 (pre-fix,
`artifacts/milestone-1-20261027/run0_s29_prefix/`): at 250–310 s the Tiger sat at (−747, 150–180)
in the hamlet street with dead T-34s **7 m, 60 m and 100 m** from the hull
(`game.combat.enemies[].range_m` with `alive: false`). The defence comes up the same
20 m street the route runs down and dies in it; the pilot then rammed its own kills, and its
perpendicular detour ladder (5–17 m off the centreline) put it against the frontage houses
(`TANK STOPPED … LoadBearingWall … canyon.hamlet.frontage.b12.right` in the missions session's
seed-137 log). Measured over the whole course, all 300 hard bodies the wreckage layer adds (vehicle wrecks,
carts, sledges, draught horses, guns, parapets — the ones `manifest_wreck_collision` gives
source-surface collision) lie **13.0 m or more** from the route polyline; the lane half-width is
12 m, so the layer never blocks the lane itself. In the hamlet the nearest are a dead horse and
an overturned sledge 13.4–14.4 m off the centreline at (−760, 69) and (−762, 76) — exactly where
the old 5–17 m perpendicular detour ladder swung the hull. That is why the no-wreck copy passed:
the same detour that scraped past loose yard debris without the layer now meets a hard body or a
frontage wall. The props are where the tool's rules put them; the pilot was the thing to fix.

Fix (`src/canyon_pilot.rs` `hull_bypass`, commit 83946a7): look 26 m ahead along the route for
a dead hull within 6 m of the travel line; aim 9 m past it, 5–9.5 m to the side, on the first
side whose street-navigation floor (the T-34 clear-cell grid the defence itself routes on) is
clear along three parallel lines 2 m apart (the Tiger's width) and which keeps 5 m from every
hull; hold that choice until the hull is 4 m behind; after a stall try the other side first. A
run-up never targets a dead hull. With it, both invincible headless proofs drove the whole course
with the wreckage layer present (`nav_proofs/`: seed 137 completed at 1,425.8 s, seed 29 at
1,806.9 s, all six mission beats done).

The mortal windowed runs then exposed the next failure each time, and each got its own commit
(every run below names the pilot commit its binary was built from):

| commit | what a run showed | change |
|---|---|---|
| 83946a7 | run 0: jammed on dead T-34s in the hamlet street | `hull_bypass` (above) |
| ae6ad8c | run 1: ready racks empty after ~28 rounds, nearest AP slot a hull bin with no prompt from the loader's post, `reload_stalled` returned no input for the rest of the run | skip a slot that never prompts; abandon a dead reload and drive on (the missions branch's rule), retry after 45 s |
| 7ea14f7 | run A0: boxed in by three dead T-34s, avoidance holding the throttle at zero ahead and refusing reverse behind | `hull_escape`: of 16 headings, the longest hull-free, floor-clear corridor, biased to the route |
| 7804cf8 | run C: a live T-34 out of sight 19 m up the street + a dead hull | bypass/escape treat live parked hulls like dead ones |
| da1789e | run A: escape re-chosen every frame, the hull only pivoted | commit to an escape heading for up to 6 s |

Run D on da1789e drove through every spot where a seed-29 run had stalled and reached 84 %.

## Defects found on the way (not caused by the merge)

1. **The physical (non-shader) backend panics at startup on `raymarching` ≥ 8cf48cf**:
   `Device::create_render_pipeline, label = 'bloom' … Unable to find entry point
   'fs_composite_glare'` — the veiling-glare pipelines are created from `bloom.wgsl`, which lacks
   the entry point only `stylized_bloom.wgsl` has (`aborted/headless_physical_backend_bloom_panic.log.gz`). Every
   headless harness run without `KESSEL_RENDERER` hits it. The raymarch session's uncommitted
   working tree already gates both pipelines on `shader_backend`; it is left to that branch.
   Headless runs here use `KESSEL_RENDERER=raymarch`.
2. **The windowed game stops simulating whenever its window loses focus** (deliberate: "a solo
   game pauses on focus loss"), so an unattended run froze at 310 s when another window took
   focus. Autopilot runs are now exempt (still stop when minimized).
3. **The shared Canyon v2 workshop is being rewritten under running sessions**: the terrain
   session rewrote `assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json` at 18:10
   (`terrain.detail`), the scatter session writes `scatter.v1.json` there. This build ignores
   both, but runs here use a frozen copy inside the worktree via `KESSEL_CANYON_MANIFEST`
   (now honoured by the windowed v2 start too) so the course cannot change mid-measurement.
4. Windowed presentation runs at the display rate even with vsync off (DWM composition of a
   windowed Vulkan swapchain): `-NoVsync` frame times still floor at ~16.7 ms. Headroom has to be
   read from the CPU and fenced GPU times, or from `gfxbench`.

## How the runs were made

- **Build**: `cargo build --release --locked --bin panzerrechnersystem` in the worktree (its own
  `target/`, `.cargo/config.toml` 64 MiB stack kept); the binary SHA-256 is in each console log's
  `[launcher]` header. Assets from the main checkout (`KESSEL_ASSET_ROOT=C:\shop\panzerrechnersystem`);
  the Canyon v2 workshop frozen inside the worktree (`KESSEL_CANYON_MANIFEST`), wreckage overlay included.
- **Launcher**: `tools\launch_windows.ps1 -Renderer raymarch -Logged -Autopilot -Seed N -Windowed -NoVsync -Binary target\int1_X.exe`
  (the exe is copied under its own image name because another session's `taskkill /IM
  panzerrechnersystem.exe` closed two runs). Runs were made one at a time in the foreground: an
  attempt to run three side by side starved the two background windows (Windows raises the
  foreground window's GPU priority and throttles covered windows), and was abandoned.
  `-Logged` is the port of `tools/launch_session_logged_playtest.sh`: `KESSEL_SESSION_LOG=artifacts\runtime\session-<stamp>.jsonl`,
  `KESSEL_GPU_PROFILE=1` (one fenced frame every `KESSEL_SESSION_LOG_GPU_S`=2 s), console to
  `session-<stamp>.console.log`. Same fixed profile as the default launch: High preset, 800×450
  internal, screen Bayer-8, MSAA 4, maps visibility, full transport, pizza audio; window 1280×720.
- **Driver**: `-Autopilot` sets `KESSEL_CANYON_AUTOPILOT=1` (new, `src/canyon_autopilot.rs`): the same
  rule-based `CanyonPilot` crew as `canyon-ai-run` drives the *windowed* run through ordinary
  inputs (drive, mouse, clicks, station actions, the physical reload), so the frames are the real
  presented frames. Nothing about the run is staged: full defence, return fire on, no god mode.
  The process exits after the after-action card (0 completed, 1 knocked out, 3 stalled 240 s, 4
  time limit 2400 s).
- **Memory**: `tools\sample_process_memory.ps1` (working set, private bytes, board VRAM and
  utilisation from `nvidia-smi`, every 5 s).
- **Analysis**: `python tools/analyze_session_log.py <run>.jsonl.gz --console … --memory …`;
  `python tools/summarize_gfxbench.py <dir>` for the benchmark. Percentiles across a run are
  percentiles of the per-second statistics the log keeps (the log has no per-frame record);
  the overall mean frame time is exact.
- **Not done: (c) the 10-minute human run.** This session had no human at the keyboard (it was
  started by another session; the owner did not join), so no human-style run was made. The
  launcher line for it is `tools\launch_windows.ps1 -Renderer raymarch -Logged` (fullscreen,
  vsync on, the default profile).

## Per-run performance

Full per-run summaries (every table the analyzer prints, including the per-minute series and
all 22 GPU passes) are `summary.md` beside each run's gzipped session log and console log.
Each section below: overall frame statistics, frame time over the run, the worst five seconds
with what the gameplay snapshot says was happening, CPU simulation by phase, the top GPU
passes from the fenced frames, memory and the run's gameplay line.

#### Run D — seed 29, final pilot da1789e (crew lost at 84.3 %, 28:56)

`artifacts/milestone-1-20261027/runD_s29_crew_lost84/` — 1717 s of play, 97763 frames.

| mean frame | median-second fps | 5th-pct-second fps | p50 of p50 | p95 of p95 | p99 of p99 | worst frame | s with p99 > 33 ms | sim mean | submit mean | remainder |
|---|---|---|---|---|---|---|---|---|---|---|
| 17.60 ms (56.8 fps) | 59.0 | 39.7 | 16.67 | 28.9 | 46.9 | 633 | 140 | 9.07 | 4.57 | 3.96 |

Over time (5-minute buckets of the per-minute table; fps is the frame-weighted mean):

| minutes | fps | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | s with an enemy ≤ 420 m |
|---|---|---|---|---|---|---|---|---|---|
| 0–5 | 58.0 | 19.8 | 31.1 | 42 | 633 | 8.9 | 4.9 | 0.56 | 230 |
| 5–10 | 60.0 | 16.8 | 20.2 | 27 | 36 | 6.0 | 4.3 | 0.10 | 0 |
| 10–15 | 57.0 | 23.3 | 29.3 | 144 | 512 | 8.7 | 5.1 | 0.52 | 176 |
| 15–20 | 46.5 | 24.5 | 53.9 | 148 | 150 | 16.8 | 4.8 | 0.56 | 292 |
| 20–25 | 60.0 | 16.7 | 20.0 | 24 | 39 | 6.8 | 4.0 | 0.10 | 0 |
| 25–29 | 60.0 | 16.7 | 20.8 | 25 | 68 | 6.7 | 4.2 | 0.51 | 121 |

Worst five seconds:

| play s | fps | frame mean / p99 / max | sim p99 | submit p99 | what was happening |
|---|---|---|---|---|---|
| 805 | 31.1 | 32 / 512 / 512 ms | 503.9 | 8.2 | enemy 271 m; 11 live AFV; rm events 1, sparks 4, parcels 4, lights 9; lights offered 6; storm 0.58; station Gunner; optic; beat 4 'The factory yard' |
| 1077 | 38.7 | 25 / 150 / 150 ms | 144.4 | 5.0 | enemy 23 m; 6 live AFV; rm events 3, sparks 0, parcels 10, lights 7; lights offered 7; storm 0.58; station Loader |
| 1082 | 37.9 | 26 / 148 / 148 ms | 143.1 | 8.8 | enemy 23 m; 6 live AFV; rm events 6, sparks 3, parcels 34, lights 8; lights offered 7; storm 0.58; station Loader |
| 1071 | 35.3 | 28 / 148 / 148 ms | 142.5 | 8.4 | enemy 23 m; 6 live AFV; rm events 2, sparks 4, parcels 1709, lights 9; lights offered 7; storm 0.58; station Loader |
| 1088 | 41.5 | 24 / 146 / 146 ms | 141.0 | 5.3 | enemy 23 m; 6 live AFV; rm events 7, sparks 0, parcels 10, lights 6; lights offered 6; storm 0.58; station Loader |

CPU simulation per frame by phase (ms):

| nearest enemy | seconds | enemy_ai.orders | enemy_ai.stealth | enemy_ai.at_guns | enemy_ai | fx | infantry | projectiles | vehicle_contacts | other | cpu_sim |
|---|---|---|---|---|---|---|---|---|---|---|---|
| all | 1717 | 3.56 | 0.03 | 0.09 | 3.71 | 2.00 | 0.77 | 0.03 | 0.03 | 2.53 | 9.07 |
| enemy > 420 m | 898 | 0.84 | 0.01 | 0.11 | 0.98 | 2.02 | 0.77 | 0.00 | 0.02 | 2.59 | 6.40 |
| enemy 150-420 m | 644 | 6.05 | 0.05 | 0.07 | 6.21 | 1.96 | 0.76 | 0.03 | 0.03 | 2.44 | 11.44 |
| enemy < 150 m | 175 | 8.31 | 0.06 | 0.09 | 8.48 | 2.01 | 0.83 | 0.12 | 0.03 | 2.58 | 14.05 |

GPU (fenced frames: 859; pass sum median 7.06 ms, mean 7.36 ms), top 8 by median:

| pass | median | mean | p95 | max |
|---|---|---|---|---|
| `rm-sky` | 2.303 | 2.838 | 4.123 | 11.4 |
| `rm-gbuffer` | 1.124 | 1.173 | 1.911 | 2.2 |
| `rm-shadow-far` | 0.867 | 0.925 | 1.439 | 2.5 |
| `rm-point-face` | 0.741 | 0.874 | 1.531 | 3.0 |
| `rm-sky-cover` | 0.401 | 0.435 | 0.730 | 1.2 |
| `rm-shadow-near` | 0.381 | 0.425 | 0.743 | 1.2 |
| `rm-shade` | 0.304 | 0.314 | 0.415 | 0.5 |
| `rm-volume` | 0.127 | 0.164 | 0.550 | 1.1 |

Memory: working set median 2735 MB, peak 7033 MB; private bytes median 7626 MB, peak 15407 MB; board VRAM (all processes) median 12045 MB, peak 20342 MB.

| gameplay | |
|---|---|
| outcome | {'outcome': 'knocked_out', 'play_s': 1736.4, 'progress': 0.8409, 'reloads': 29} |
| first enemy vehicle ≤ 420 m / first main-gun shot / first hit taken | 29.5 s / 33.6 s / 185.1 s |
| main-gun shots / hits dealt / kills | 30 / 23 / 16 |
| hits taken / penetrations / modules out | 12 / 0 / ['HullMg'] |
| death | THE RUN IS OVER — CREW LOST |
| reloads | 29; median 6.1 s, max 7.2 s |
| mission beats started | 1 'Read the ground' at 0 s; 2 'Phase line Anton: the hamlet' at 4 s; 3 'The gun line at the fork' at 90 s; 4 'The factory yard' at 673 s; 5 'The final basin' at 1553 s |
| stalls (≥ 20 s under 0.3 m/s, no enemy within 420 m) | [(617, 639), (1498, 1538), (1582, 1612)] |
| goal distance at the end | 728 m |

#### Run A — seed 29, pilot 7804cf8 (stalled at 57.9 %: escape heading dithered among hulls)

`artifacts/milestone-1-20261027/runA_s29_stalled58/` — 1152 s of play, 67824 frames.

| mean frame | median-second fps | 5th-pct-second fps | p50 of p50 | p95 of p95 | p99 of p99 | worst frame | s with p99 > 33 ms | sim mean | submit mean | remainder |
|---|---|---|---|---|---|---|---|---|---|---|
| 17.02 ms (58.8 fps) | 59.0 | 50.1 | 16.62 | 24.8 | 39.1 | 588 | 34 | 8.60 | 4.73 | 3.68 |

Over time (5-minute buckets of the per-minute table; fps is the frame-weighted mean):

| minutes | fps | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | s with an enemy ≤ 420 m |
|---|---|---|---|---|---|---|---|---|---|
| 0–5 | 58.2 | 19.0 | 27.8 | 39 | 588 | 8.6 | 4.8 | 0.56 | 231 |
| 5–10 | 60.0 | 16.7 | 20.5 | 27 | 44 | 6.0 | 4.4 | 0.10 | 5 |
| 10–15 | 58.1 | 16.7 | 31.2 | 54 | 274 | 8.8 | 5.0 | 0.52 | 158 |
| 15–20 | 58.9 | 16.5 | 29.4 | 36 | 40 | 11.2 | 4.7 | 0.58 | 261 |

Worst five seconds:

| play s | fps | frame mean / p99 / max | sim p99 | submit p99 | what was happening |
|---|---|---|---|---|---|
| 808 | 39.6 | 25 / 274 / 274 ms | 268.9 | 7.5 | enemy 168 m; 10 live AFV; rm events 1, sparks 2, parcels 2, lights 7; lights offered 6; storm 0.58; station Gunner; optic |
| 895 | 30.3 | 32 / 164 / 164 ms | 154.3 | 13.5 | enemy 29 m; 7 live AFV; rm events 1, sparks 0, parcels 5, lights 6; lights offered 6; storm 0.58; station Gunner; optic |
| 896 | 40.8 | 25 / 54 / 54 ms | 41.7 | 12.7 | enemy 136 m; 6 live AFV; rm events 2, sparks 0, parcels 11, lights 8; lights offered 7; storm 0.58; station Loader |
| 892 | 37.6 | 26 / 47 / 47 ms | 20.4 | 15.8 | enemy 29 m; 7 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Gunner; optic |
| 742 | 32.7 | 29 / 46 / 46 ms | 37.0 | 9.1 | enemy 443 m; 12 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Commander; beat 4 'The factory yard' |

CPU simulation per frame by phase (ms):

| nearest enemy | seconds | enemy_ai.orders | enemy_ai.stealth | enemy_ai.at_guns | enemy_ai | fx | infantry | projectiles | vehicle_contacts | other | cpu_sim |
|---|---|---|---|---|---|---|---|---|---|---|---|
| all | 1152 | 3.41 | 0.03 | 0.10 | 3.58 | 1.87 | 0.79 | 0.02 | 0.02 | 2.32 | 8.60 |
| enemy > 420 m | 497 | 1.02 | 0.01 | 0.12 | 1.18 | 1.78 | 0.80 | 0.00 | 0.02 | 2.07 | 5.87 |
| enemy 150-420 m | 313 | 5.37 | 0.04 | 0.07 | 5.52 | 1.62 | 0.78 | 0.02 | 0.02 | 2.16 | 10.14 |
| enemy < 150 m | 342 | 5.10 | 0.05 | 0.10 | 5.28 | 2.22 | 0.77 | 0.03 | 0.03 | 2.82 | 11.16 |

GPU (fenced frames: 576; pass sum median 7.43 ms, mean 7.73 ms), top 8 by median:

| pass | median | mean | p95 | max |
|---|---|---|---|---|
| `rm-sky` | 3.401 | 3.037 | 4.178 | 10.8 |
| `rm-gbuffer` | 1.264 | 1.263 | 1.881 | 2.5 |
| `rm-point-face` | 0.908 | 0.908 | 1.360 | 3.7 |
| `rm-shadow-far` | 0.896 | 0.959 | 1.420 | 2.7 |
| `rm-sky-cover` | 0.450 | 0.456 | 0.699 | 2.7 |
| `rm-shadow-near` | 0.423 | 0.435 | 0.653 | 1.1 |
| `rm-shade` | 0.302 | 0.311 | 0.425 | 0.6 |
| `rm-volume` | 0.124 | 0.154 | 0.455 | 1.3 |

Memory: working set median 2633 MB, peak 6916 MB; private bytes median 7554 MB, peak 15336 MB; board VRAM (all processes) median 12044 MB, peak 17336 MB.

| gameplay | |
|---|---|
| outcome | {'outcome': 'pilot_stalled', 'play_s': 1164.0, 'progress': 0.5791, 'reloads': 17} |
| first enemy vehicle ≤ 420 m / first main-gun shot / first hit taken | 29.5 s / 33.6 s / 185.1 s |
| main-gun shots / hits dealt / kills | 17 / 13 / 13 |
| hits taken / penetrations / modules out | 5 / 0 / [] |
| death | none |
| reloads | 17; median 6.1 s, max 7.1 s |
| mission beats started | 1 'Read the ground' at 0 s; 2 'Phase line Anton: the hamlet' at 4 s; 3 'The gun line at the fork' at 90 s; 4 'The factory yard' at 677 s |
| stalls (≥ 20 s under 0.3 m/s, no enemy within 420 m) | [] |
| goal distance at the end | 1408 m |

#### Run B — seed 7, pilot 7804cf8 (knocked out at 44.9 %: one penetration, fuel fire, cook-off)

`artifacts/milestone-1-20261027/runB_s7_knocked_out/` — 1101 s of play, 62521 frames.

| mean frame | median-second fps | 5th-pct-second fps | p50 of p50 | p95 of p95 | p99 of p99 | worst frame | s with p99 > 33 ms | sim mean | submit mean | remainder |
|---|---|---|---|---|---|---|---|---|---|---|
| 17.63 ms (56.7 fps) | 59.1 | 44.6 | 16.71 | 27.7 | 44.8 | 605 | 61 | 8.60 | 4.48 | 4.55 |

Over time (5-minute buckets of the per-minute table; fps is the frame-weighted mean):

| minutes | fps | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | s with an enemy ≤ 420 m |
|---|---|---|---|---|---|---|---|---|---|
| 0–5 | 59.8 | 16.7 | 20.9 | 30 | 605 | 6.8 | 4.7 | 0.35 | 210 |
| 5–10 | 60.0 | 16.7 | 21.9 | 30 | 41 | 5.8 | 4.2 | 0.26 | 4 |
| 10–15 | 56.8 | 20.8 | 33.9 | 60 | 61 | 7.7 | 4.6 | 0.58 | 167 |
| 15–19 | 47.4 | 20.8 | 88.4 | 126 | 126 | 17.0 | 4.5 | 0.40 | 210 |

Worst five seconds:

| play s | fps | frame mean / p99 / max | sim p99 | submit p99 | what was happening |
|---|---|---|---|---|---|
| 1109 | 12.7 | 79 / 126 / 126 ms | 109.2 | 11.3 | enemy 80 m; 9 live AFV; rm events 0, sparks 1, parcels 15873, lights 18; lights offered 18; storm 0.10; station Commander; beat 4 'The factory yard' |
| 1111 | 12.0 | 84 / 94 / 94 ms | 81.4 | 12.9 | enemy 78 m; 9 live AFV; rm events 0, sparks 0, parcels 15873, lights 18; lights offered 18; storm 0.10; station Commander; beat 4 'The factory yard' |
| 1112 | 10.7 | 85 / 94 / 94 ms | 76.4 | 12.9 | enemy 78 m; 9 live AFV; rm events 0, sparks 0, parcels 15873, lights 17; lights offered 17; storm 0.10; station Commander; beat 4 'The factory yard' |
| 1110 | 11.3 | 81 / 87 / 87 ms | 75.1 | 11.7 | enemy 78 m; 9 live AFV; rm events 0, sparks 1, parcels 15873, lights 18; lights offered 18; storm 0.10; station Commander; beat 4 'The factory yard' |
| 875 | 32.5 | 31 / 61 / 61 ms | 51.8 | 8.7 | enemy 300 m; 7 live AFV; rm events 5, sparks 4, parcels 45, lights 7; lights offered 7; storm 0.58; station Loader |

CPU simulation per frame by phase (ms):

| nearest enemy | seconds | enemy_ai.orders | enemy_ai.stealth | enemy_ai.at_guns | enemy_ai | fx | infantry | projectiles | vehicle_contacts | other | cpu_sim |
|---|---|---|---|---|---|---|---|---|---|---|---|
| all | 1101 | 3.48 | 0.02 | 0.11 | 3.65 | 1.83 | 0.84 | 0.12 | 0.02 | 2.13 | 8.60 |
| enemy > 420 m | 510 | 0.59 | 0.01 | 0.09 | 0.72 | 1.80 | 0.82 | 0.00 | 0.01 | 2.13 | 5.51 |
| enemy 150-420 m | 512 | 5.82 | 0.04 | 0.12 | 6.02 | 1.90 | 0.86 | 0.01 | 0.02 | 2.15 | 10.97 |
| enemy < 150 m | 79 | 6.99 | 0.02 | 0.08 | 7.13 | 1.61 | 0.87 | 1.56 | 0.02 | 1.92 | 13.25 |

GPU (fenced frames: 551; pass sum median 6.99 ms, mean 7.28 ms), top 8 by median:

| pass | median | mean | p95 | max |
|---|---|---|---|---|
| `rm-sky` | 3.326 | 2.994 | 4.272 | 11.2 |
| `rm-gbuffer` | 1.076 | 1.081 | 1.927 | 2.4 |
| `rm-shadow-far` | 0.871 | 0.908 | 1.480 | 2.0 |
| `rm-point-face` | 0.587 | 0.721 | 1.306 | 3.0 |
| `rm-shadow-near` | 0.387 | 0.418 | 0.670 | 1.1 |
| `rm-sky-cover` | 0.380 | 0.423 | 0.675 | 1.0 |
| `rm-shade` | 0.323 | 0.334 | 0.433 | 0.6 |
| `rm-volume` | 0.136 | 0.183 | 0.616 | 1.0 |

Memory: working set median 2680 MB, peak 6821 MB; private bytes median 7595 MB, peak 15104 MB; board VRAM (all processes) median 12035 MB, peak 20273 MB.

| gameplay | |
|---|---|
| outcome | {'outcome': 'knocked_out', 'play_s': 1112.3, 'progress': 0.4488, 'reloads': 30} |
| first enemy vehicle ≤ 420 m / first main-gun shot / first hit taken | 22.5 s / 30.5 s / 53.6 s |
| main-gun shots / hits dealt / kills | 31 / 7 / 7 |
| hits taken / penetrations / modules out | 2 / 1 / ['Engine', 'Fuel'] |
| death | THE RUN IS OVER — FUEL FIRE — BURNED OUT |
| reloads | 30; median 6.1 s, max 25.3 s |
| mission beats started | 1 'Read the ground' at 0 s; 2 'Phase line Anton: the hamlet' at 4 s; 3 'The gun line at the fork' at 91 s; 4 'The factory yard' at 1102 s |
| stalls (≥ 20 s under 0.3 m/s, no enemy within 420 m) | [(642, 715)] |
| goal distance at the end | 1767 m |

#### Run C — seed 101, pilot 5ffd352 (stalled at 62.6 %: live T-34 unseen 19 m up the street; first 2.5 min shared with two aborted parallel runs)

`artifacts/milestone-1-20261027/runC_s101_stalled63/` — 1138 s of play, 60825 frames.

| mean frame | median-second fps | 5th-pct-second fps | p50 of p50 | p95 of p95 | p99 of p99 | worst frame | s with p99 > 33 ms | sim mean | submit mean | remainder |
|---|---|---|---|---|---|---|---|---|---|---|
| 18.99 ms (52.7 fps) | 58.8 | 36.7 | 16.71 | 70.5 | 388.5 | 11692 | 254 | 9.70 | 4.83 | 4.46 |

Over time (5-minute buckets of the per-minute table; fps is the frame-weighted mean):

| minutes | fps | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | s with an enemy ≤ 420 m |
|---|---|---|---|---|---|---|---|---|---|
| 0–5 | 45.8 | 64.1 | 461.0 | 570 | 11692 | 8.2 | 4.9 | 0.16 | 221 |
| 5–10 | 60.0 | 16.7 | 21.4 | 30 | 34 | 6.8 | 4.5 | 0.44 | 48 |
| 10–15 | 57.0 | 20.2 | 30.8 | 46 | 198 | 9.0 | 5.0 | 0.58 | 153 |
| 15–20 | 49.0 | 20.4 | 34.2 | 43 | 45 | 15.2 | 4.9 | 0.16 | 255 |

Worst five seconds:

| play s | fps | frame mean / p99 / max | sim p99 | submit p99 | what was happening |
|---|---|---|---|---|---|
| 1 | 9.9 | 1161 / 11692 / 11692 ms | 29.6 | 14.3 | enemy 483 m; 11 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Commander; beat 1 'Read the ground' |
| 29 | 8.9 | 173 / 570 / 570 ms | 12.2 | 8.0 | enemy 275 m; 11 live AFV; rm events 0, sparks 0, parcels 0, lights 7; lights offered 7; storm 0.40; station Gunner; optic; beat 2 'Phase line Anton: the hamlet' |
| 27 | 8.3 | 179 / 552 / 552 ms | 17.6 | 7.4 | enemy 280 m; 11 live AFV; rm events 0, sparks 0, parcels 0, lights 7; lights offered 7; storm 0.42; station Gunner; optic; beat 2 'Phase line Anton: the hamlet' |
| 24 | 10.9 | 130 / 461 / 461 ms | 28.4 | 4.5 | enemy 311 m; 11 live AFV; rm events 0, sparks 0, parcels 0, lights 7; lights offered 7; storm 0.47; station Commander; beat 2 'Phase line Anton: the hamlet' |
| 39 | 17.5 | 80 / 455 / 455 ms | 21.3 | 4.3 | enemy 248 m; 11 live AFV; rm events 1, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.26; station Loader; beat 2 'Phase line Anton: the hamlet' |

CPU simulation per frame by phase (ms):

| nearest enemy | seconds | enemy_ai.orders | enemy_ai.stealth | enemy_ai.at_guns | enemy_ai | fx | infantry | projectiles | vehicle_contacts | other | cpu_sim |
|---|---|---|---|---|---|---|---|---|---|---|---|
| all | 1138 | 4.23 | 0.02 | 0.09 | 4.38 | 1.89 | 0.95 | 0.06 | 0.02 | 2.38 | 9.70 |
| enemy > 420 m | 461 | 0.79 | 0.01 | 0.09 | 0.92 | 1.86 | 0.90 | 0.00 | 0.02 | 2.18 | 5.89 |
| enemy 150-420 m | 313 | 4.37 | 0.04 | 0.08 | 4.52 | 1.78 | 1.03 | 0.20 | 0.02 | 2.35 | 9.94 |
| enemy < 150 m | 364 | 8.46 | 0.04 | 0.11 | 8.63 | 2.02 | 0.95 | 0.00 | 0.02 | 2.66 | 14.31 |

GPU (fenced frames: 568; pass sum median 7.17 ms, mean 10.14 ms), top 8 by median:

| pass | median | mean | p95 | max |
|---|---|---|---|---|
| `rm-sky` | 2.341 | 2.884 | 4.133 | 11.2 |
| `rm-gbuffer` | 1.122 | 2.239 | 1.948 | 183.4 |
| `rm-point-face` | 0.999 | 1.453 | 1.622 | 116.4 |
| `rm-shadow-far` | 0.811 | 1.277 | 1.809 | 49.9 |
| `rm-sky-cover` | 0.461 | 0.979 | 0.807 | 105.5 |
| `rm-shadow-near` | 0.412 | 0.602 | 0.903 | 76.8 |
| `rm-shade` | 0.302 | 0.312 | 0.428 | 0.7 |
| `rm-volume` | 0.125 | 0.193 | 0.557 | 18.4 |

Memory: working set median 2874 MB, peak 7911 MB; private bytes median 7656 MB, peak 15325 MB; board VRAM (all processes) median 12036 MB, peak 23032 MB.

| gameplay | |
|---|---|
| outcome | {'outcome': 'pilot_stalled', 'play_s': 1151.7, 'progress': 0.6262, 'reloads': 17} |
| first enemy vehicle ≤ 420 m / first main-gun shot / first hit taken | 12.6 s / 30.9 s / 32.6 s |
| main-gun shots / hits dealt / kills | 17 / 11 / 11 |
| hits taken / penetrations / modules out | 4 / 0 / [] |
| death | none |
| reloads | 17; median 6.1 s, max 7.1 s |
| mission beats started | 1 'Read the ground' at 0 s; 2 'Phase line Anton: the hamlet' at 4 s; 3 'The gun line at the fork' at 84 s; 4 'The factory yard' at 681 s |
| stalls (≥ 20 s under 0.3 m/s, no enemy within 420 m) | [(624, 647)] |
| goal distance at the end | 1371 m |

#### Run A0 — seed 29, pilot b0813a9 (stalled at 67.3 %: boxed by three dead T-34s)

`artifacts/milestone-1-20261027/runA0_s29_stalled67/` — 1495 s of play, 85706 frames.

| mean frame | median-second fps | 5th-pct-second fps | p50 of p50 | p95 of p95 | p99 of p99 | worst frame | s with p99 > 33 ms | sim mean | submit mean | remainder |
|---|---|---|---|---|---|---|---|---|---|---|
| 17.47 ms (57.2 fps) | 59.0 | 44.3 | 16.66 | 27.4 | 47.6 | 603 | 88 | 9.06 | 5.25 | 3.16 |

Over time (5-minute buckets of the per-minute table; fps is the frame-weighted mean):

| minutes | fps | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | s with an enemy ≤ 420 m |
|---|---|---|---|---|---|---|---|---|---|
| 0–5 | 57.7 | 18.1 | 29.0 | 75 | 603 | 8.8 | 5.6 | 0.56 | 239 |
| 5–10 | 59.2 | 16.9 | 29.8 | 40 | 55 | 6.8 | 5.4 | 0.10 | 2 |
| 10–15 | 56.2 | 20.2 | 30.3 | 51 | 61 | 9.6 | 5.2 | 0.52 | 139 |
| 15–20 | 53.3 | 20.4 | 32.4 | 44 | 52 | 12.4 | 5.4 | 0.56 | 252 |
| 20–25 | 59.6 | 16.7 | 24.2 | 162 | 165 | 7.8 | 4.7 | 0.10 | 69 |
| 25–26 | 59.8 | 16.6 | 23.2 | 25 | 32 | 8.4 | 6.0 | 0.10 | 0 |

Worst five seconds:

| play s | fps | frame mean / p99 / max | sim p99 | submit p99 | what was happening |
|---|---|---|---|---|---|
| 212 | 42.7 | 23 / 277 / 277 ms | 268.4 | 9.4 | enemy 61 m; 7 live AFV; rm events 3, sparks 3, parcels 631, lights 8; lights offered 6; storm 0.58; station Gunner; optic |
| 1250 | 45.6 | 22 / 165 / 165 ms | 157.8 | 8.3 | enemy 13 m; 4 live AFV; rm events 2, sparks 0, parcels 5, lights 6; lights offered 6; storm 0.10; station Loader |
| 1261 | 43.8 | 23 / 162 / 162 ms | 154.5 | 13.1 | enemy 13 m; 4 live AFV; rm events 6, sparks 2, parcels 35, lights 8; lights offered 7; storm 0.10; station Loader |
| 1266 | 46.8 | 21 / 155 / 155 ms | 150.3 | 7.9 | enemy 13 m; 4 live AFV; rm events 4, sparks 0, parcels 5, lights 7; lights offered 7; storm 0.10; station Loader |
| 1244 | 50.7 | 19 / 144 / 144 ms | 138.7 | 5.0 | enemy 13 m; 4 live AFV; rm events 2, sparks 1, parcels 5, lights 7; lights offered 7; storm 0.10; station Loader |

CPU simulation per frame by phase (ms):

| nearest enemy | seconds | enemy_ai.orders | enemy_ai.stealth | enemy_ai.at_guns | enemy_ai | fx | infantry | projectiles | vehicle_contacts | other | cpu_sim |
|---|---|---|---|---|---|---|---|---|---|---|---|
| all | 1495 | 3.00 | 0.02 | 0.11 | 3.16 | 2.12 | 0.87 | 0.02 | 0.03 | 2.85 | 9.06 |
| enemy > 420 m | 794 | 0.99 | 0.01 | 0.12 | 1.16 | 2.13 | 0.87 | 0.00 | 0.02 | 2.88 | 7.09 |
| enemy 150-420 m | 394 | 5.03 | 0.03 | 0.07 | 5.16 | 2.01 | 0.88 | 0.01 | 0.03 | 2.81 | 10.91 |
| enemy < 150 m | 307 | 5.61 | 0.04 | 0.10 | 5.77 | 2.24 | 0.83 | 0.07 | 0.03 | 2.82 | 11.77 |

GPU (fenced frames: 747; pass sum median 7.68 ms, mean 7.97 ms), top 8 by median:

| pass | median | mean | p95 | max |
|---|---|---|---|---|
| `rm-sky` | 3.396 | 3.063 | 4.087 | 10.9 |
| `rm-gbuffer` | 1.146 | 1.192 | 1.831 | 2.7 |
| `rm-point-face` | 0.993 | 1.150 | 2.008 | 3.9 |
| `rm-shadow-far` | 0.900 | 0.952 | 1.434 | 2.5 |
| `rm-sky-cover` | 0.451 | 0.483 | 0.721 | 3.9 |
| `rm-shadow-near` | 0.436 | 0.461 | 0.733 | 1.1 |
| `rm-shade` | 0.300 | 0.311 | 0.429 | 0.6 |
| `rm-volume` | 0.125 | 0.155 | 0.457 | 1.2 |

Memory: working set median 2700 MB, peak 6915 MB; private bytes median 7615 MB, peak 15329 MB; board VRAM (all processes) median 16113 MB, peak 20662 MB.

| gameplay | |
|---|---|
| outcome | {'outcome': 'pilot_stalled', 'play_s': 1511.3, 'progress': 0.6726, 'reloads': 27} |
| first enemy vehicle ≤ 420 m / first main-gun shot / first hit taken | 29.5 s / 33.6 s / 212.2 s |
| main-gun shots / hits dealt / kills | 27 / 22 / 16 |
| hits taken / penetrations / modules out | 7 / 0 / [] |
| death | none |
| reloads | 27; median 6.1 s, max 7.1 s |
| mission beats started | 1 'Read the ground' at 0 s; 2 'Phase line Anton: the hamlet' at 4 s; 3 'The gun line at the fork' at 90 s; 4 'The factory yard' at 687 s |
| stalls (≥ 20 s under 0.3 m/s, no enemy within 420 m) | [(1471, 1493)] |
| goal distance at the end | 1357 m |

#### Run 1 — seed 29, pilot b6705fe (stalled at 57.6 %: ready rack empty, reload froze); heavy sibling GPU load

`artifacts/milestone-1-20261027/run1_s29_stalled/` — 1260 s of play, 49960 frames.

| mean frame | median-second fps | 5th-pct-second fps | p50 of p50 | p95 of p95 | p99 of p99 | worst frame | s with p99 > 33 ms | sim mean | submit mean | remainder |
|---|---|---|---|---|---|---|---|---|---|---|
| 25.66 ms (39.0 fps) | 39.1 | 17.9 | 16.45 | 115.4 | 306.6 | 855 | 1140 | 13.88 | 5.48 | 6.30 |

Over time (5-minute buckets of the per-minute table; fps is the frame-weighted mean):

| minutes | fps | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | s with an enemy ≤ 420 m |
|---|---|---|---|---|---|---|---|---|---|
| 0–5 | 45.5 | 24.9 | 115.5 | 256 | 855 | 11.4 | 7.0 | 0.56 | 220 |
| 5–10 | 42.9 | 14.9 | 123.3 | 325 | 335 | 7.0 | 4.2 | 0.10 | 0 |
| 10–15 | 38.1 | 26.2 | 128.7 | 370 | 428 | 12.6 | 5.4 | 0.52 | 120 |
| 15–20 | 29.6 | 50.8 | 199.4 | 345 | 357 | 25.0 | 5.4 | 0.56 | 289 |
| 20–22 | 37.8 | 15.9 | 99.2 | 295 | 295 | 13.0 | 5.1 | 0.10 | 104 |

Worst five seconds:

| play s | fps | frame mean / p99 / max | sim p99 | submit p99 | what was happening |
|---|---|---|---|---|---|
| 863 | 8.8 | 107 / 428 / 428 ms | 121.7 | 8.5 | enemy 218 m; 10 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Commander |
| 217 | 9.4 | 98 / 417 / 417 ms | 81.7 | 15.2 | enemy 451 m; 6 live AFV; rm events 1, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Loader |
| 872 | 19.3 | 65 / 370 / 370 ms | 97.6 | 7.2 | enemy 171 m; 10 live AFV; rm events 2, sparks 2, parcels 13, lights 7; lights offered 7; storm 0.58; station Loader |
| 969 | 27.4 | 45 / 357 / 357 ms | 107.3 | 5.5 | enemy 50 m; 7 live AFV; rm events 2, sparks 2, parcels 15, lights 8; lights offered 7; storm 0.58; station Loader |
| 941 | 8.4 | 112 / 357 / 357 ms | 156.2 | 8.3 | enemy 130 m; 8 live AFV; rm events 1, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Commander |

CPU simulation per frame by phase (ms):

| nearest enemy | seconds | enemy_ai.orders | enemy_ai.stealth | enemy_ai.at_guns | enemy_ai | fx | infantry | projectiles | vehicle_contacts | other | cpu_sim |
|---|---|---|---|---|---|---|---|---|---|---|---|
| all | 1260 | 0.00 | 0.00 | 0.00 | 7.14 | 2.39 | 1.07 | 0.05 | 0.03 | 3.18 | 13.88 |
| enemy > 420 m | 527 | 0.00 | 0.00 | 0.00 | 1.43 | 2.31 | 0.97 | 0.00 | 0.02 | 2.71 | 7.48 |
| enemy 150-420 m | 265 | 0.00 | 0.00 | 0.00 | 8.65 | 2.19 | 1.15 | 0.01 | 0.03 | 3.01 | 15.07 |
| enemy < 150 m | 468 | 0.00 | 0.00 | 0.00 | 12.73 | 2.58 | 1.12 | 0.12 | 0.04 | 3.80 | 20.41 |

GPU (fenced frames: 627; pass sum median 7.56 ms, mean 9.73 ms), top 8 by median:

| pass | median | mean | p95 | max |
|---|---|---|---|---|
| `rm-sky` | 2.199 | 2.859 | 4.138 | 11.2 |
| `rm-gbuffer` | 1.209 | 1.932 | 2.350 | 128.6 |
| `rm-shadow-far` | 0.952 | 1.111 | 2.019 | 22.6 |
| `rm-point-face` | 0.886 | 1.097 | 2.081 | 13.0 |
| `rm-sky-cover` | 0.452 | 0.777 | 1.096 | 26.4 |
| `rm-shadow-near` | 0.446 | 1.207 | 1.091 | 247.9 |
| `rm-shade` | 0.289 | 0.306 | 0.428 | 0.6 |
| `rm-volume` | 0.137 | 0.229 | 0.543 | 38.1 |

Memory: working set median 2855 MB, peak 7623 MB; private bytes median 7610 MB, peak 15359 MB; board VRAM (all processes) median 22892 MB, peak 23072 MB.

| gameplay | |
|---|---|
| outcome | {'outcome': 'pilot_stalled', 'play_s': 1307.7, 'progress': 0.5758, 'reloads': 28} |
| first enemy vehicle ≤ 420 m / first main-gun shot / first hit taken | 29.5 s / 33.6 s / 909.0 s |
| main-gun shots / hits dealt / kills | 29 / 20 / 14 |
| hits taken / penetrations / modules out | 7 / 0 / [] |
| death | none |
| reloads | 28; median 6.2 s, max 7.7 s |
| mission beats started | 1 'Read the ground' at 0 s; 2 'Phase line Anton: the hamlet' at 4 s; 3 'The gun line at the fork' at 90 s; 4 'The factory yard' at 717 s |
| stalls (≥ 20 s under 0.3 m/s, no enemy within 420 m) | [(604, 666)] |
| goal distance at the end | 1400 m |

#### Run 0 — seed 29, merged pilot 3f1c693 before the hull bypass (vsync on; jammed on dead T-34s in the hamlet, froze on focus loss at 310 s); heavy sibling GPU load

`artifacts/milestone-1-20261027/run0_s29_prefix/` — 305 s of play, 11150 frames.

| mean frame | median-second fps | 5th-pct-second fps | p50 of p50 | p95 of p95 | p99 of p99 | worst frame | s with p99 > 33 ms | sim mean | submit mean | remainder |
|---|---|---|---|---|---|---|---|---|---|---|
| 27.67 ms (36.1 fps) | 34.0 | 15.5 | 18.67 | 197.6 | 487.0 | 1389 | 259 | 13.32 | 6.59 | 7.76 |

Over time (5-minute buckets of the per-minute table; fps is the frame-weighted mean):

| minutes | fps | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | s with an enemy ≤ 420 m |
|---|---|---|---|---|---|---|---|---|---|
| 0–5 | 35.9 | 24.7 | 199.1 | 487 | 1389 | 13.9 | 6.7 | 0.57 | 237 |
| 5–6 | 37.4 | 16.8 | 289.1 | 524 | 524 | 7.8 | 5.0 | 0.20 | 3 |

Worst five seconds:

| play s | fps | frame mean / p99 / max | sim p99 | submit p99 | what was happening |
|---|---|---|---|---|---|
| 312 | 1.2 | 95 / 524 / 524 ms | 15.8 | 6.9 | enemy 1108 m; 5 live AFV; rm events 1, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.11; station Commander |
| 288 | 4.8 | 228 / 520 / 520 ms | 56.5 | 4.6 | enemy 68 m; 6 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.39; station Commander |
| 313 | 9.9 | 106 / 512 / 512 ms | 27.2 | 7.3 | enemy 1109 m; 5 live AFV; rm events 1, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.11; station Commander |
| 268 | 1.5 | 250 / 487 / 487 ms | 167.6 | 6.2 | enemy 66 m; 6 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Commander |
| 267 | 4.6 | 257 / 433 / 433 ms | 86.4 | 8.0 | enemy 66 m; 6 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Commander |

GPU (fenced frames: 155; pass sum median 7.72 ms, mean 12.06 ms), top 8 by median:

| pass | median | mean | p95 | max |
|---|---|---|---|---|
| `rm-sky` | 2.107 | 2.704 | 4.232 | 10.7 |
| `rm-gbuffer` | 1.541 | 2.420 | 2.679 | 72.3 |
| `rm-point-face` | 1.294 | 2.677 | 3.032 | 148.9 |
| `rm-shadow-far` | 0.999 | 1.859 | 2.318 | 57.6 |
| `rm-sky-cover` | 0.477 | 0.930 | 1.038 | 30.7 |
| `rm-shadow-near` | 0.464 | 0.718 | 1.542 | 17.4 |
| `rm-shade` | 0.314 | 0.331 | 0.435 | 1.0 |
| `rm-volume` | 0.125 | 0.176 | 0.496 | 1.1 |

Memory: working set median 3824 MB, peak 7101 MB; private bytes median 7474 MB, peak 15116 MB; board VRAM (all processes) median 22662 MB, peak 22922 MB.

| gameplay | |
|---|---|
| outcome | None |
| first enemy vehicle ≤ 420 m / first main-gun shot / first hit taken | 29.8 s / 32.9 s / — s |
| main-gun shots / hits dealt / kills | 11 / 7 / 6 |
| hits taken / penetrations / modules out | 0 / 0 / [] |
| death | none |
| reloads | 11; median 6.1 s, max 7.3 s |
| mission beats started | 1 'Read the ground' at 0 s; 2 'Phase line Anton: the hamlet' at 4 s; 3 'The gun line at the fork' at 90 s |
| stalls (≥ 20 s under 0.3 m/s, no enemy within 420 m) | [] |
| goal distance at the end | 2457 m |

## `gfxbench 240` per internal-resolution preset

`gfxbench 240 240 <png> high` on the raymarch backend, `KESSEL_GPU_PROFILE=1`, one run per entry of
`renderer::INTERNAL_RES_PRESETS` (all twelve), High preset. The bench scene is the deterministic
22 m exterior orbit over the Act One city (24 k instances), not the canyon. Wall time is
`render_offscreen` + fence per frame (CPU encode included); the pass columns are the `[gpuprof]`
per-frame means. Sweep 2 (`artifacts/milestone-1-20261027/gfxbench_sweep2/`) is the complete one;
the "GPU before" column is `nvidia-smi` sampled just before each point — **the GPU was never
idle** (KSP and the renderer session's `rm_capture.exe` were running), so read these as
contended upper bounds. Sweep 1 agreed within 0.5–2 ms at 320×180–640×360 and was ruined from
800×450 up by a sibling's capture and my own rebuild (kept in `gfxbench_sweep1/` for the record).

| internal | wall avg ms (fps) | p50 | p99 | GPU pass sum | rm-gbuffer | rm-sky | rm-sky-cover | rm-shadow-far | rm-shadow-near | rm-point-face ×12 | rm-shade | rm-volume | GPU before |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 320×180 | 18.83 (53.1) | 17.94 | 27.52 | 12.02 | 4.25 | 3.48 | 1.90 | 1.65 | 0.32 | 0.14 | 0.06 | 0.08 | 96 % |
| 320×240 | 19.07 (52.5) | 17.80 | 36.03 | 11.01 | 3.38 | 3.48 | 1.68 | 1.73 | 0.32 | 0.14 | 0.07 | 0.09 | 100 % |
| 480×272 | 20.52 (48.7) | 19.44 | 29.54 | 12.33 | 4.22 | 3.48 | 2.14 | 1.60 | 0.32 | 0.14 | 0.08 | 0.14 | 70 % |
| 576×324 | 20.62 (48.5) | 19.63 | 28.92 | 13.30 | 4.14 | 3.44 | 2.08 | 1.55 | 0.32 | 0.14 | 0.10 | 0.20 | 84 % |
| 640×360 | 22.73 (44.0) | 19.82 | 37.56 | 13.45 | 5.01 | 3.47 | 2.30 | 1.67 | 0.32 | 0.14 | 0.12 | 0.24 | 81 % |
| **800×450 (default)** | **23.63 (42.3)** | 22.68 | 40.40 | **14.79** | 5.08 | 3.49 | 2.14 | 2.16 | 0.32 | 0.14 | 0.16 | 0.36 | 100 % |
| 800×480 | 26.72 (37.4) | 26.35 | 39.85 | 16.60 | 5.79 | 3.47 | 2.21 | 2.50 | 0.32 | 0.14 | 0.17 | 0.43 | 55 % |
| 960×540 | 22.00 (45.5) | 20.93 | 31.02 | 14.24 | 4.01 | 3.50 | 2.04 | 1.65 | 0.32 | 0.14 | 0.23 | 2.02 | 90 % |
| 1280×720 | 23.63 (42.3) | 21.49 | 34.98 | 15.85 | 4.09 | 3.49 | 2.14 | 1.78 | 0.32 | 0.14 | 0.38 | 3.15 | 73 % |
| 1600×900 | 26.73 (37.4) | 25.13 | 36.07 | 17.64 | 3.99 | 3.49 | 1.64 | 1.62 | 0.32 | 0.14 | 0.60 | 5.18 | 90 % |
| 1920×1080 | 39.71 (25.2) | 38.17 | 55.78 | 27.74 | 5.86 | 3.46 | 2.21 | 2.46 | 0.32 | 0.14 | 0.86 | 11.12 | 83 % |
| 2560×1440 | 40.49 (24.7) | 38.54 | 62.44 | 30.32 | 4.16 | 3.48 | 1.74 | 1.72 | 0.32 | 0.14 | 1.58 | 13.98 | 96 % |

Reading it:

- **Up to 1600×900 the frame is resolution-independent**: G-buffer (geometry-bound, 24 k
  instances), sky cache (fixed 768×288), sky cover and the sun cascades are ~11–12 ms of GPU at
  every resolution, and wall time sits 6–10 ms above the pass sum (CPU encode + the fence).
  Lowering the internal grid below 800×450 buys at most ~3 ms; the city costs what it costs.
- **The froxel volume is the only pass that scales**: 0.36 ms at 800×450, 3.2 ms at 1280×720,
  11.1 ms at 1920×1080, 14.0 ms at 2560×1440 (its grid is a quarter of the internal resolution ×
  96 slices). Above 1600×900 the volume, not the G-buffer, is the frame. A preset above 900p
  should cap the froxel grid rather than tie it to the internal resolution.
- In the canyon (session logs) the same passes are cheaper — median fenced pass sum 7.0–7.7 ms —
  because the canyon draws ~35 k instances of mostly small props over open terrain and the city
  bench is a worst case for `rm-gbuffer` and `rm-sky-cover`.

## Optimization targets, ranked

Ranked by what they cost a frame on this build, measured, with the evidence line for each. CPU
items first: the 4090 was shared all session, but these are this process's own threads, and the
biggest of them is larger than the whole fenced GPU frame.

1. **Street-order planning for the T-34 crews (`winter_encounter_ai::Encounter::orders` →
   `street_nav::Leg::plan`) — 5.6–8.5 ms per frame in contact, the largest single cost in the
   game.** `enemy_ai` is 97 % this one call; stealth/line-of-sight is 0.02–0.04 ms and the AT guns
   0.1 ms. Evidence: run C `enemy < 150 m (364 s) | enemy_ai.orders 8.46 | enemy_ai 8.63 | cpu_sim 14.31`;
   run B `enemy < 150 m … enemy_ai.orders 6.99`; run A0 `… 5.61`; with every enemy beyond 420 m it
   is 0.8–1.0 ms. Mechanism (code read, not yet timed separately): `drive_to` re-plans whenever a
   crew's goal has moved more than 6 m or the crew is off its leg, and in contact the goal is the
   moving Tiger; each `StreetNav::path` call is an A* over the 306,218-cell street grid that
   allocates two full-grid `Vec<u32>` (2.4 MB) before it starts. Fix candidates: reuse the
   g/came scratch per encounter, rate-limit re-plans per crew (e.g. ≥ 0.5 s) and scale the 6 m
   goal tolerance with distance to the goal, bound the search window; then time `Leg::plan`
   alone to confirm.
2. **Shell-impact hitches inside `advance_projectiles` — single simulation steps of 140–500 ms.**
   `projectiles` is ~0.05 ms per frame on average, but the worst seconds of the cleanest run are
   all one projectile step: run D 805 s `sim max 503.9 ms`, phase `projectiles 13.82` ms/frame
   over that second (47 updates, so one call of ~0.6 s); 1071 s `sim max 142.5`, `projectiles
   3.72`; 1077 s `sim max 144.4`, `projectiles 3.32` (a T-34 at 23 m, the Tiger firing and being
   hit). That is a frozen picture on the moment of a hit — the worst place for one. The impact
   path (plate solve, behind-armour fragments, armour scarring/dents, wreck and fire spawning) needs
   a timer per stage; whatever builds geometry or allocates there should move off the frame.
3. **FX update (`Game::update_fx`) — 1.8–2.4 ms per frame, constant, even with nothing burning.**
   Evidence: run C `enemy > 420 m (461 s) | fx 1.86`; run 1 `all | fx 2.39`. It does not track
   the raymarch work counters (parcels 0, sparks 0 in most seconds), so it is per-frame pool
   iteration, not effect load. Next: time its sub-steps.
4. **Unattributed simulation ("other", the rest of `Game::update`) — 2.2–3.0 ms per frame.**
   Evidence: every run's phase table, e.g. run B `all (1101 s) … other 2.13 | cpu_sim 8.60`,
   run A0 `all (1495 s) … other 2.85`. Interior/station/presentation
   `drive_*` calls and `update_scene_environment` are the untimed remainder; one more timer pass
   will split it.
5. **`rm-sky` (by-direction sky cache) — 2.2–2.3 ms median per frame, ~30 % of the GPU frame, and
   resolution-independent.** Evidence: run C `rm-sky | median 2.341`; gfxbench `rm-sky` 3.44–3.52
   ms at every internal resolution from 320×180 to 2560×1440. The cache is 768×288 refreshed one of
   16 phases per frame; a static camera and sun under a static sky do not need a phase every frame,
   and the blizzard storms are the only fast-changing input.
6. **`rm-gbuffer` — 1.1–1.5 ms in the canyon, 4.1–4.7 ms in the city, flat across resolution.**
   Evidence: gfxbench `rm-gbuffer` 4.48 (320×180), 4.11 (640×360); canyon runs median 1.12–1.54.
   Geometry-bound (24–36 k live instances, `live instances | 34546`), not fill-bound: instance
   culling / distance LOD before resolution.
7. **Point-light cube faces (`rm-point-face`, 12 per frame) — 0.7–1.3 ms median.** Evidence: run C
   `rm-point-face | median 0.999`; `shadowed_lights` is 2 in every second of every run. The two
   shadowed lights are cabin lamps that rarely move relative to their casters: cache the faces
   and redraw on change.
8. **Far sun cascade (`rm-shadow-far`) — 0.8–1.0 ms (city 1.7) and sky cover (`rm-sky-cover`)
   0.45 ms (city 1.8–2.0).** Evidence: run C `rm-shadow-far | median 0.811`, gfxbench
   `rm-sky-cover` 1.79–2.04. Both re-render mostly static terrain every frame: half-rate or
   scrolled far cascade, cached sky-cover depth with dynamic casters added.
9. **CPU render submission — 4.5–5.5 ms mean per frame** (`cpu_render_submit_ms`; run B 4.48,
   run A0 5.25), of which the raymarch backend's own prepare+encode is ~1.8 ms
   (`prepare_ms 0.60`, `encode_ms 1.15` medians). The rest is the shared renderer's scene walk.
10. **Load-time memory: private bytes peak ~15.3 GB at load, ~7.6 GB steady; working set ~2.7–2.9 GB
   median, 6.8–7.9 GB peak.** Evidence: every run's memory table (run C `private bytes … peak
   15325 MB`). The load peak is twice the steady state: transient decode/upload buffers held
   until the world is built. With three instances (or siblings) on the board, VRAM ran out and the
   game panicked instead of degrading (item 11).
11. **Robustness under pressure (not a frame cost, a crash).** (a) VRAM exhaustion: a buffer
    (`private articulated actor vertices`) came back invalid and `Buffer::get_mapped_range`
    panicked (`aborted/runA_parallel_vram_panic.console.log.gz`); (b) a surface that cannot be
    acquired (`surface error: Other`, 806 times) ended a run with status 1
    (`aborted/run1b_surface_error.console.log.gz`); (c) the physical backend panics at startup on
    this branch (bloom `fs_composite_glare`). None of these degrade gracefully.
12. **Start-of-run hitches: worst frames 0.6–1.4 s in the first seconds of every run** (run B
    605 ms, run A0 603 ms, run 0 1389 ms; run C's 11.7 s includes the window being restored and
    resized by the tiling step) — streaming/first-use work after the world is "ready".

What is *not* a target on this evidence: the froxel volume, integrate, shade, glass, reflections,
bloom/glare and display passes together are under 0.6 ms median; vehicle contacts, projectiles,
wrecks/fire/haze and the canyon director/mission are each under 0.2 ms per frame.

## Gameplay data from the snapshots

All windowed runs below are mortal (full defence, return fire on, no god mode); the two headless
runs are the invincible traversal proofs (`KESSEL_CANYON_AI_INVINCIBLE=1`, disclosed in their
receipts) and are listed only for navigation and beat timing. Times are the Canyon clock.

| run | outcome | reached | first AFV ≤ 420 m | first shot | first hit taken | shots / hits / kills | hits taken (pens) | reloads: n, median, max | time lost to stalls |
|---|---|---|---|---|---|---|---|---|---|
| **D — seed 29 (final pilot)** | **crew lost 28:56** — killed in the instant of a 76 mm hull-front bounce at 29 m (99.3 mm vs 102.5 mm effective, `Bounce`, then `killed: CrewLost`, 0 penetrations all run) | **84.3 % (4,151 of 4,926 m)** | 29.5 s | 33.6 s | 185 s | 30 / 23 / 16 | 12 (0) | 29, 6.1 s, 7.2 s; then 3 rack slots out of reach → reload abandoned 3× and the last ~2 min driven with an empty gun | no terminal stall (standstills 617–639, 1498–1538, 1582–1612 s: duels and reload cycles) |
| A — seed 29 | stalled 19:24 | 57.9 % | 29.5 s | 33.6 s | 185 s | 17 / 13 / 13 | 5 (0) | 17, 6.1 s, 7.1 s | final 240 s pivoting among hulls |
| B — seed 7 | **knocked out 18:32** — one penetration took engine and fuel; fuel fire reached the ammunition ("FIRE reached the ammunition — cook-off, Tiger burned out") | 44.9 % (2,213 of 4,926 m) | 22.5 s | 30.5 s | 53.6 s | 31 / 7 / 7 | 2 (1) | 30, 6.1 s, 25.3 s | — |
| C — seed 101 | stalled 19:12 | 62.6 % | 12.6 s | 30.9 s | 32.6 s | 17 / 11 / 11 | 4 (0) | 17, 6.1 s, 7.1 s | final 240 s: live T-34 out of sight 19 m up the street + dead hull 14 m |
| A0 — seed 29 (before escape) | stalled 25:11 | 67.3 % | 29.5 s | 33.6 s | 212 s | 27 / 22 / 16 | 7 (0) | 27, 6.1 s, 7.1 s | final 240 s boxed by three dead T-34s |
| 1 — seed 29 (before reload fix) | stalled 21:48 | 57.6 % | 29.5 s | 33.6 s | 909 s | 29 / 20 / 14 | 7 (0) | 28, 6.2 s, 7.7 s | final 240 s frozen at the breech: ready rack empty |
| headless s29 (invincible) | **completed 30:07** | 100 % | — | — | — | 26 / 20 / 16 | 7 (3) | 26 | — |
| headless s137 (invincible) | **completed 23:46** | 100 % | — | — | — | 19 / 14 / 13 | 2 (0) | 19 | — |

Mission beats (start → end, s; "—" = not reached):

| run | 1 Read the ground | 2 Phase line Anton (hamlet) | 3 Gun line at the fork | 4 Factory yard | 5 Final basin | 6 Ravine head |
|---|---|---|---|---|---|---|
| D | 0 → 4 | 4 → 90 | 90 → 107 | 673 → 825 | 1553 → (crew lost at 1736) | — |
| A | 0 → 4 | 4 → 90 | 90 → 107 | 677 → 757 | — | — |
| B | 0 → 4 | 4 → 91 | 91 → 114 | 1102 → (died) | — | — |
| C | 0 → 4 | 4 → 84 | 84 → 123 | 681 → 772 | — | — |
| A0 | 0 → 4 | 4 → 90 | 90 → 107 | 687 → 766 | — | — |
| headless s29 | 0 → 4 | 4 → 89 | 89 → 107 | 732 → 805 | 1435 → 1688 | 1688 → 1807 |
| headless s137 | 0 → 4 | 4 → 89 | 89 → 106 | 673 → 766 | 1137 → 1270 | 1270 → 1426 |

Between beat 3's end (~107–123 s) and beat 4's start (~680 s) the session snapshot reports no
active beat: that is the long march from the fork to the factory yard, where the mission sheet
holds no objective. Beat 4 → 5 is the next ~6–11 minutes of road.

What the numbers say about play:

- **Contact comes fast and early**: a T-34 inside 420 m at 12–30 s in every run, the first round
  fired at ~31–34 s. Hits taken are low (2–7 per run) and almost always bounce: 1 penetration in
  ~6,500 s of mortal windowed play across five runs — and that one penetration ended run B through
  engine → fuel fire → cook-off. That is the "one pen is catastrophic" balance the run-loop session
  flagged; it is the owner's call under the physical-truth rules.
- **Reload is a steady 6.1–6.2 s median** (7.1–7.7 s max) on the physical ritual; the 25.3 s outlier
  in run B is a retried cycle after the stage machine timed out.
- **Kill rate is high** (7–16 kills, 0.4–0.8 per minute) — the defence fields 11–16 T-34s per run.
- **Two deaths, both from the damage model's sharp edges, not from attrition**: B burned out from
  a single penetration (engine → fuel fire → cook-off), D's lone crewman died on a hit that did
  not penetrate. The second is worth a look under the physical-truth rules: a non-penetrating
  hull-front strike killing the crew needs a named mechanism (spall from the near-limit bounce?)
  in the ledger, or it is a defect.
- **The loader runs dry of reachable rounds after ~28 shots**: in runs 1 and D the remaining AP
  rounds (34–36 left) sat in hull bins no prompt reaches from the loader's post, so a long run ends
  unarmed. Either the bins need a reachable route (turret position / basket walk) or the crew
  needs a way to restock the ready racks.
- **Before the final pilot, every mortal run that did not die ended in a navigation stall between 57 % and 67 %**, always
  among enemy hulls the Tiger itself knocked out (or one still alive and unseen) in the streets of
  the factory district; never against a wreckage-layer prop alone. The stops of 20–70 s at
  (−40, −635) in runs 1, B and C are not stalls but standing duels with a target the vehicle-range
  heuristic does not count (engage/reload cycles at speed 0).
- Only run D reached beat 5 (the final basin, at 25:53) in mortal play; the full course (4.9 km) is
  24–30 minutes for this crew. No mortal run reached the ravine head.

## Open items

1. **A clean measurement.** Re-run `gfxbench` ×12 and one autopilot run with no other game,
   capture or KSP process on the GPU (`nvidia-smi` idle). Everything above is contended.
2. **Mortal full-course completion by the autopilot.** Navigation with the wreckage layer is
   proven end to end (two invincible headless completions); the final pilot's mortal run reached
   84 % and died, earlier pilots stalled among enemy hulls at 57–67 %. Each pilot change on this
   branch (hull bypass, slot skip and reload abandon, escape, live hulls, escape commit) fixed the
   stall it was written for; the factory district may still produce new deadlock shapes, and the
   seed-101 fix was not re-run. A planner that routes on the street nav grid with live and dead
   hulls stamped into it would replace the reactive ladder.
3. **Two damage-model questions for the owner** (gameplay, not pilot): run D's crewman died on a
   non-penetrating 76 mm hull-front bounce (99.3 mm vs 102.5 mm effective) — the mechanism needs
   a ledger line or it is a defect; run B burned out from its only penetration.
4. **Ammunition the loader cannot reach**: after ~28 rounds the remaining AP rounds are in hull
   bins with no prompt from the loader's post; long runs end with an empty gun.
5. **The 10-minute human run in the native window** — not done (no human in this session).
6. **Physical-backend startup panic** (`fs_composite_glare`) — fixed in the renderer session's
   working tree, not yet on any pushed branch.
7. **Sibling hygiene**: windowed runs were killed by an image-name `taskkill` from another session
   (one graceful at 19:01, one forced at 19:13); integration runs now use their own image names.
8. Branches not in this build: `crew-solo`, `scatter`, `terrain-detail`, `armor-scars` (their
   sessions were still working and nothing was on origin).
