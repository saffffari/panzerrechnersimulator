# Integration receipt — 7 November 2026

Native baseline: merge commit `e4d438a` on `codex/integration-20261107`, parents `9fc6553`
and web tip `52b7d10`. This includes the current web worktree's source and native-model package,
not its dependency/build directories or stale QA reports. Older experimental worktrees remain
untouched; see `INTEGRATION_AUDIT_20261107.md` for the explicit exclusion list.

The subsequent `codex/engine-overhaul-20261107` branch contains the original concept models,
experimental sparse-voxel core and uncapped range frame telemetry. It has not replaced the
production gameplay engine. GitHub main and the separately installed editor-stable build were
not updated.

## Verified

- 39 focused native release tests passed: contacts, blood timing, temporal progression,
  mantlet partition, range controls, terrain-only serialization, manifest reload,
  full display-state transfer and map-failure recovery.
- Web: 27 tests, TypeScript/production build, and 15 real Chromium interaction checks passed;
  no page exceptions. The inspected screenshot shows the authored Tiger and bottom-bar controls.
- A clean non-incremental native release build succeeded. The prior interrupted incremental
  build left undefined hidden symbols at link; a clean code-generation pass resolved that
  build-cache failure without changing application logic.
- Fresh native bridge probe completed RANGE → WORLD → RANGE in the same window. WORLD's
  authoring smoke saved/reloaded its temporary edits and protected the source map files.
- Four native yaw/pitch/recoil poses passed fixed shield/cradle, moving barrel and articulated
  contact-frame assertions.
- The penetrating BR-365 functional probe advanced two simulation seconds and passed blood
  deposition, no-cache firing, pause, resume and reset checks. It recorded 13,339 crew contacts.
- Sparse core: 10 tests passed, including negative-boundary DDA and randomized greedy-face
  equivalence. All four source fixtures passed local-edit and publication checks.
- Concept packages passed independent format readback and Blender import checks. All four
  final geometry-derived overview renders were visually inspected.

## Remaining limits

- These checks do not certify 60 FPS. The offscreen functional strike measured 51.5 ms median,
  67.0 ms p95, 96.1 ms worst in a contended run with synchronized readback and a concurrent
  compile. It is not comparable hardware FPS evidence or a clean improvement measurement.
- The temporary range GPU-timestamp hookup stalled and was removed before the overhaul commit.
  No new synchronous GPU profiling is active in ordinary play.
- The mantlet pose capture still shows colour/rainbow artefacts. Transform correctness is
  verified; visual correctness of the native renderer is not declared solved. Athena's
  read-only pixel/code review of this specific capture found the expected coarse 7/7/4 RGB
  levels and a strong eight-pixel repeat, consistent with look 6's Bayer8 display path.
  Headless initialization does not inherit saved editor dither preferences. The next test is
  `debug_capture_frozen_dither_pair` after convergence, holding HDR/eye state and quantization
  fixed; this observation does not diagnose every earlier transient colour report.
- The bridge map is an installed local asset payload. A source checkout alone does not install
  the full existing game asset warehouse or this editable map copy.
- The browser remains an approximate raster/physics port. The sparse-voxel lab remains separate
  from gameplay, structural physics, FEA and renderer upload integration.
- The complete revised film batch, campaign-scale destruction and all world-editor workflows
  were not validated by this integration pass.

Local evidence: `target/integration-20261107/` contains build/test logs, browser QA, WORLD receipts,
probe logs, the exploratory chunk report and exported edited meshes. Native screenshots and
concept renders are in `/windows/shop/panzerrechnersystem/captures/screenshots/`. Captures are
not source-controlled; the compact chunk report is preserved in `docs/benchmarks/`.
