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

## Round 2 — 2026-10-27

- raymarching @ 7904d3c (indoor panes stop reflecting the sky, shade temporal filter, GPU-free shader validation test) — none — merged in 7ad9bdd
- platoon @ 4b6d5f3 — already in (round 1); nothing new
- claude/canyon-wreckage-20261027 @ 413ee0c, claude/spatial-audio-20261027 @ 272e358 — already in since round 0; nothing new
- terrain-detail @ 58117d0 (partial; its session is still working) — ledger "Updated:" unioned, section on top; RAYMARCHING_ENGINE open items both kept; `src/renderer_raymarch.rs`: **`KESSEL_RM_SKIP` bit collision** (raymarching uses 8192 for screen-space indirect light; terrain-detail took 8192 for the detail terrain) → detail terrain moved to **262144** (code, TERRAIN_DETAIL.md, terrain_showcase doc, ledger); both GPU-free shader test modules kept; raymarching's `Push.extra` filled in terrain-detail's depth draw and declared in `rm_terrain_depth.wgsl` — merged in fbb4e76
- scatter @ 4d3afdf — ledger "Updated:" unioned, section on top; renderer/raymarch/game/main merged automatically, no skip-bit or push overlap — merged in 9ffc949
- armor-scars @ bad2a6c (+3975888) — **push-constant overlap**: raymarching's `extra: vec4<f32>` and armor-scars' `scars: vec4<u32>` at the same offset → `Push { model, params, extra, scars }`, 112 bytes (device floor 128), in Rust, `rm_depth.wgsl`, `rm_gbuffer.wgsl`, `rm_terrain_depth.wgsl`; scar storage buffers + the probe's `COPY_SRC`; scar-pipeline mesh loop keeps the detail-terrain skip; G-buffer pass runs scatter's indirect draw and the near-scar patches; `renderer.rs` both fields; `main.rs` both subcommands; ledger unioned; raymarching's shader test now assembles rm-depth/rm-gbuffer with `rm_scars.wgsl` and covers rm-scar-patch — merged in 9f6e57e
- crew-solo @ 635bf63 — **the branch had turned nine files CRLF** (game.rs alone showed 74,902 changed lines); merged from a local normalization commit (`int/crew-solo-lf`, content identical under `--ignore-cr-at-eol`), after which game.rs is +98/−10; `docs/CONTROLS.md` key table unioned (K, cupola LMB, hold-E treat/drag, driver's-seat LMB; no key collisions); ledger unioned — merged in 6824a75
- **Gate for 6824a75 (+ this log)**: build ok; GPU-free shader validation (raymarch + terrain + scar programs) ok; unit tests 1,926 passed / 90 failed / 61 ignored against `origin/raymarching` 7904d3c 1,862 / 89 / 61 in the same environment — the one extra failure is terrain-detail's new `source_detail_tiles_carry_no_broad_structure`, which reads git-ignored `source_assets/` and passes from the asset checkout; no regressions; smoke **pending GPU window** (owner's hold) — `main` **advanced** to 6824a75 (+ this log)

## Round 2b — 2026-10-27

- crew-solo @ 3dbd422 (its own CRLF→LF fix) — whole-file conflicts only because the merge base 635bf63 is CRLF; 3dbd422's tree is byte-identical to the normalized `int/crew-solo-lf` merged in round 2, so merged with `-s ours` (no content change; crew-solo's history is now an ancestor, later pushes merge normally) — merged in 3855e55; tree identical to bf9c23f, so the round-2 build/test gate stands (build re-run ok); smoke **pending GPU window** — `main` **advanced** to 3855e55 (+ this log)
- Confirmed in `main`: claude/canyon-wreckage-20261027 @ 413ee0c and claude/spatial-audio-20261027 @ 272e358 (merged in round 0).

## Round 3 — 2026-10-28

- raymarching @ 7904d3c — nothing new
- raymarching-2 @ b697e14 (off `main` 8f7481e: `tools/build_render_gallery.py`, `tools/run_ai_fights.ps1`; no Rust or shader change) — none — merged in 3aad760; **`run_ai_fights.ps1` did not parse**: the usage comment's `tools\run_ai_fights.ps1` had been written with `\r` as a carriage return, which ended the comment and left `<checkout with assets>` as code (`The '<' operator is reserved for future use`, line 5) → fixed forward in b54fa01 (PowerShell parser: 0 errors); the gallery builder compiles and built a gallery from this worktree's `captures/` (2 stills)
- **Gate for b54fa01 (+ this log)**: build ok; unit tests 1,926 passed / 90 failed / 61 ignored, failure set identical to round 2 (vs `origin/raymarching` 7904d3c 1,862 / 89 / 61 — the one extra is terrain-detail's `source_detail_tiles_carry_no_broad_structure`, `source_assets/` only); smoke **pending GPU window** (owner's hold) — `main` **advanced** to b54fa01 (+ this log)
- Next: the renderer session's `raymarching` fix (black RTT sights on the raymarch backend, outdoor fog inside the hull, glass grime) gets its own round as soon as it is pushed — the owner is playing `main` and waiting on it.

## Round 4 — 2026-10-28 (renderer fix, merged alone; the owner is playing `main`)

- raymarching @ 8ab7931 (376942f vision blocks and sights relay the outside, no weather in the cabin air; 95de896 relay skip bit **524288** — 262144 stays the detail terrain; 8ab7931 optics start clean, sky cache refreshes in warp tiles, the gunner's eyepiece pass reuses the sight pass's maps) — taken as the renderer session's merge commit **27d2bdb** (`raymarching-main-merge`, built on `main` d9eecb8, fast-forward). Checked against a local merge of `origin/raymarching`: every auto-merged file identical; its three conflict resolutions verified — `docs/RAYMARCHING_ENGINE.md` open items (the "cupola eyepieces are glass panes, no relayed views" item replaced by the relay item; terrain-detail's rut items kept), `src/main.rs` subcommands (`raymarch-cabin` + `terrain-bench`/`terrain-showcase`/`armor-scars` all kept), `src/renderer_raymarch.rs` (skip-bit doc lists 262144 and 524288; the G-buffer mesh loop keeps the detail-terrain skip, the scar binding and `mesh_index`, plus the relayed-panel skip; scatter's GPU cull runs first, the relays as step 0b)
- **Gate for 27d2bdb (+ this log)**: build ok; GPU-free shader validation ok; unit tests 1,927 passed / 90 failed / 61 ignored, failure set identical to round 3 (the one new test passes); smoke **pending GPU window** (owner's hold; the renderer session ran its own short repro captures) — `main` **advanced** to 27d2bdb (+ this log)

## Round 5 — 2026-10-28

- raymarching @ 660bde7 (fa9af06 gunner sight low-passes the hull's jolts, cabin dust rides the hull; b790d61 raymarch-cabin sight-line jolt capture; add17c6 two eyepiece fields, clean glass with ghost and wiping arcs; 1f2ab22 Tiger winter whitewash; 660bde7 sight lamps: loading status and chambered round) — taken through the renderer session's `raymarching-main-merge` @ aba9058, which adds one main-only commit (aba9058: a wounded loader still working blinks the loading lamp's working lens, from crew-solo's performance) — none — build ok — merged in a58e112
- scatter @ 3805dd5 (interaction tiers by estimated mass and size: nothing under 15 cm or 5 kg collides with the hull, light large pieces are cover only, boulders/logs/barricades/pallets resist by their own mass) — none — build ok — merged in d1aee44
- crew-solo @ 2518830 (solo crew legibility: nobody left at the cupola, bodies and blood where men fall; `crew_solo_capture.rs`) — none; stored LF (`git ls-files --eol`) — merged in f2043ac
- **Gate for f2043ac (+ this log)**: build ok; GPU-free shader validation ok; unit tests 1,928 passed / 90 failed / 61 ignored, failure set identical to round 4 (the scatter tiering test and crew-solo's tests pass); smoke **pending GPU window** — `main` **advanced** to f2043ac (+ this log)

## Round 6 — 2026-10-28

- raymarching @ 2e175d5 (TPZ needle drawn 2.5 mm wide; instrument covers get dust and a lamp sheen) — through `raymarching-main-merge` @ a238074 — none — merged in d4f536e
- platoon @ d0d97ba (wingmen fight with the trained Tiger policy in contact, short halt for its shots) — no textual conflict, but **the test build broke**: its new test's `terrain::Heightfield` literal lacks terrain-detail's `detail` field (E0063, `src/platoon.rs:1346`; the release build was fine) → fixed forward in c250280 (`detail: None`, as every other test literal on main) — merged in 91635d4
- **Gate for c250280 (+ this log)**: build ok; GPU-free shader validation ok; unit tests 1,931 passed / 90 failed / 61 ignored, failure set identical to round 5 (13 platoon tests pass); smoke **pending GPU window** — `main` **advanced** to c250280 (+ this log)

## Round 7 — 2026-10-28

- raymarching @ 2e175d5 — nothing new
- platoon @ d0d97ba — already in (round 6)
- missions @ 96b8114 (branched from main 06a7d5f: a button folds the order sheet — Enter/Space/E/click/pad — M rereads it on the map; Enter clears the radio, platoon and intercom lines on the frame; touches `crew_solo.rs`, `platoon.rs`, `ui.rs`, `main.rs`) — none; stored LF — merged in fdd3e63
- **Gate for fdd3e63 (+ this log)**: build ok; test build ok; GPU-free shader validation ok; unit tests 1,934 passed / 90 failed / 61 ignored, failure set identical to round 6 (mission, ui and platoon tests pass); smoke **pending GPU window** — `main` **advanced** to fdd3e63 (+ this log)
- Seen on origin, not yet relayed as working: scatter @ 757f95e (+1)

## Round 8 — 2026-10-28

- raymarching @ 992d707 (the Tiger's exhaust: a winter plume from the engine model in the raymarch medium; new `src/exhaust_plume.rs`) — through `raymarching-main-merge` @ 787715a — none — merged in fe3fd6c
- scatter @ 757f95e (trees fail at the weaker of stem snap and root-plate tip, woody pieces under 400 kg crushed, heavier logs roll; `tank_push.rs`, `trees.rs`) — none; stored LF — merged in 6a9cad2. Its session notes it is unverified in-engine: a treefall run hit a raymarching bloom bug the renderer session is checking.
- **Gate for 6a9cad2 (+ this log)**: build ok; test build ok; GPU-free shader validation ok; unit tests 1,939 passed / 90 failed / 61 ignored, failure set identical to round 7 (scatter, trees, tank_push tests pass); smoke **pending GPU window** — `main` **advanced** to 6a9cad2 (+ this log)

## Round 9 — 2026-10-28 (six branches, parent's order; **not pushed** — the parent pushes after the renderer's last items)

- raymarching-main-merge @ c241223 (raymarching d2332f1: gunner view histories and painted steel at 480×270, small-arms tracers and flashes at a bullet's size with the coax from its mantlet port, night sight without cabin light outside and the TZF reticle at output resolution, zero-reach relay point sources, **OpenVDB explosion volumes** (`src/explosion_volumes.rs`, `tools/bake_explosion_volumes.py`), pixel stars, stepped Sun and Moon discs) — fast-forward from bb2ce6b
- crew-solo @ 60cf65b (empties rack in the bustle recess, hatch toss when full, commander's MMB designation with gunner pre-slew and TPZ second pointer, cupola report checked against the render) — none — merged in bcb17b5
- t34-ai @ 1d0f741 (T-34 crews fix and flank, shoot and move, react to hits; `tank_doctrine.rs`, `enemy_ai_telemetry.rs`; `artifacts/t34-ai-20261028/` before/after evidence, 0.4 MB) — none — merged in 5b5842f
- vehicle-dynamics @ f554cd8 (sourced spec table, sprung hulls, AI tanks on the force-based drivetrain, head sway) — ledger "Updated:" line unioned; `tank.rs`/`game.rs`/`canyon_attempt.rs` merged automatically. **Overlap with t34-ai checked**: t34-ai's crews emit `drive`/`steer` orders and execute them through `Tank::update`, which vehicle-dynamics now runs through each vehicle's own `powertrain` (`powertrain::ai_driver`); nothing in t34-ai writes enemy speed, position or heading directly, so the decision layer is t34-ai's and the physics vehicle-dynamics'. t34-ai's halts and moves were tuned against the old instant acceleration; its tests pass on the merged build, in-engine behaviour unverified (GPU hold) — merged in 61e2e8e
- sound-world @ 2eaa76f (ray-traced room acoustics, 314 foley cues; from the spatial-audio session) — ledger "Updated:" unioned; `src/game.rs` bow-against-cover loop: scatter's indexed loop with the heavy-piece shove kept, sound-world's `struck_cover` / `speed_before_cover` added, `struck_cover` set only on the unyielding path (`foley_hard_stop` is "stopped dead against static mass"; a heavy strewn piece resists and slides with its own collision rumble) — merged in ed7f2ab
- lan-windows @ 5a73db4 (per-adapter discovery via `windows-sys` (already in the lock), join diagnosis, AI crew in empty host seats, launcher host/join + laptop tier, firewall script, portable package) — ledger: header unioned, its section kept below vehicle-dynamics'; `src/game.rs`: crew-solo's `crew_solo_update(input, dt, eye, look)` kept with lan-windows' `else { crew_lan_ai_update(dt) }` for co-op — merged in 69f8901
- **Gate for 69f8901 (+ this log)**: build ok (`--locked`); test build ok; GPU-free shader validation ok; unit tests **1,993 passed / 89 failed / 63 ignored** — no new failures against round 8, one fixed (`renderer::atmosphere_tests::the_eye_is_the_display_and_never_touches_transport`); the branches' own modules pass (suspension 11, vehicle_spec 4, powertrain 11, winter_encounter_ai 14, tank_doctrine 2, lan 124, net_ifaces 5, crew_solo 4, acoustics 79, exhaust 7, mission sheets); smoke **pending GPU window** — local `main` **advanced** to 69f8901 (+ this log); **not pushed** (parent's instruction)
- **Tracked-file check** (`git ls-files -ci --exclude-standard`): three tracked files match an ignore rule, all on main since September and none from this round — `assets/worldgen/revisions/act1-winter-approach/20260913-v1/manifest.v1.json` (1.1 MB, a9d0525), `catalog/maps/canyon_survey_20260930.png` and `canyon_v2_survey_20260930.png` (0.4 / 0.7 MB). This round adds no `dist/`, no asset binaries, no VDB data (the atlas bakes to `assets/vfx/explosion_volumes/`, ignored by `/assets/*`); it adds 35 evidence files under `artifacts/` (0.66 MB: t34-ai, sound-world, lan-windows), as the branches before it did.
- **For the LAN package**: `tools/package_windows.ps1` copies the assets in `tools/windows_payload_traces/worlds_20261028.txt`, traced before the VDB explosions landed; the atlas is not in it, so a package built now runs explosions without the baked volumes (the documented fallback). Re-trace the payload on merged main before packaging.

## Round 10 — 2026-10-28 (graphics-presets; first smoke since the GPU hold; **not pushed**)

- graphics-presets @ 66eceee (local branch from the gallery worktree, two commits on main bb56ff6: 99fcaa4 Low/High/Ultra presets with `graphics::RaymarchQuality` driving the raymarch budgets, High = the old values, the 113 ms/frame cabin `sky-visibility-static` rebuild fixed (`KESSEL_RETAIN_RIGID_SKY_DEPTH` now opt-in), Low refreshes a rolling subset of periscope relays; 66eceee launcher `-Graphics Low|High|Ultra` and `-Bind`) — **the branch committed all 14 files with CRLF** (whole-file diffs, e.g. 5,729 lines in `renderer_raymarch.rs`) → rebuilt as `int/graphics-presets-lf` (5e94bb7, 183e459: same messages and authors, LF content; empty `--ignore-cr-at-eol` diff against 66eceee; real change +231/−99) — none — merged in a8ba9f7
- **Gate for a8ba9f7**: build ok; test build ok; unit tests 1,995 passed / 89 failed / 63 ignored, failure set identical to round 9 (+2 = the new graphics tests)
- **Smoke (GPU free; one headless process at a time, renamed exe `int1_smoke.exe`)**, `artifacts/integration-smoke-20261028/` (result JSONs committed; the screenshot and traces stay local — `*.png` there is ignored):
  - raymarch `screenshot` — exit 0, `[renderer] backend=raymarch`, 2400×1350 from 800×450: the gunner's TZF view with both eyepiece fields, the reticle and the whitewashed barrel; the sight is not black
  - `canyon-ai-run` (Canyon v2, frozen manifest, 300 s cap) — exit 0 and no panic, **but the autopilot stalled**: seeds 137 and 7 pivoted in place at (−771, 625), 250 m in, for the whole run (5 %), seed 29 died there at 124 s (ammunition detonation). **Bisected** (seed 137, 26 s per run): round 0 516b9fd, round 1 4faa0c4 and 9f6e57e (round 2 through armor-scars) drive through (6.9 % at 30 s, 21 % at 300 s); 6824a75 (round-2 crew-solo merge), 8f7481e, 6070b77, bb2ce6b, 5b5842f and a8ba9f7 all stall. Cause: `crew_solo::crew_driver_follow` restarted the driver's 0.30 s reaction on *any* change of the ordered drive/steer; the pilot's steering is analog and changes every frame, so the driver never acted on it and held a stale command through the first bend (4,368 "Driver:" lines in 300 s). It has been in `main` since round 2 — the deferred smoke is what would have caught it — and affects any analog input (a gamepad stick) the same way
  - **fixed forward in b14b7c5**: the reaction and the spoken word come only when the order's intent changes (each axis back / none / forward, 0.05 deadband); within one order the change reaches the levers at the hands' rate as before. Rerun: seeds 29 / 7 / 137 alive at 300 s at 17.2 / 13.9 / 15.1 %, through the hamlet, 6 / 4 / 6 kills, no penetrations taken; 59 driver lines. Slower than before crew-solo (21 %), as the driver's reaction and hand rate are meant to be. Tests after the fix: 1,995 / 89 / 63, identical set; crew_solo tests pass
- local `main` **advanced** to b14b7c5 (+ this log); **not pushed** (parent's instruction)
