# Old renderer review — 2026-10-01

This isolated checkout starts at `c6bdb335be8fc471bf6232198f67202ae4b904fe`. The active portable source-triangle visibility/medium overhaul remains in the separate `canyon-v2-20260930` checkout. These films use the older raster renderer and its existing documented rendering limitations.

The native capture harness uses the real game maps, Tiger and T-34 source models, authoritative fixed-step simulation, occupied crew viewpoints, ordinary live gameplay audio, and real armor/damage outcomes. Initial placements, weather, automated crew input and actor readiness are disclosed in each receipt. No forced hit, kill or invulnerability is used.

The first baseline, `captures/old_renderer_blizzard_cupola_v1_20261001.mp4`, is 960×540 at 30fps for 16 seconds (480 frames). Its receipt passed all eight observation checks and ffmpeg decoded both streams without errors. It records one Tiger shot, two T-34 shots and the commander's death at 7.6667 seconds. This is preserved baseline audio; the later sound, sealed-breech-light and compact portal-bloom changes are absent.

Subsequent review staging puts the six-T-34 blizzard encounter at a base 250m with readiness staggered from 6.0 seconds in 2.5-second steps. Actual AI approach, aiming and outcomes remain live. This is a single disclosed staging change, not an outcome or random-seed search.

The display comparison ports only the fixed64×64 void-and-cluster threshold tile and frozen display-stage helper. Bayer4 remains the default. Paired frames reuse identical HDR, exposure, camera, particles, simulation and audio; only the display threshold differs. They do not compare different light transport algorithms. The baseline particle pipeline's bindings are preserved.

The initial ordinary and paired videos render natively at960×540. New winter-only review uses640×360 with default ordered Bayer4 at the user’s request. Encoder dimensions and output receipts now follow the actual internal render size. The recorder can still support older1920×1080 callers, but these outputs bypass that upscale. Per-case JSON receipts distinguish observation checks from subjective visual or listening approval.


## Winter-only correction

The early distant-blizzard clips used the Act0 training arena’s legacy DEM, not the authored Canyon V2 map. Its Winter season flag also passed through a neutral campaign profile that incorrectly mixed spring ground into the terrain. The corrected ground mood follows the map’s winter season; source rock/snow material masks remain unchanged.

`winter_canyon` and `winter_canyon_inside` explicitly select `assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json`, build Act1 through `Game::build_playtest_act`, and use the resolved authored spawn and180° course heading. The exact source world, terrain material and authored grass patches remain present. The six review T-34s occupy fixed geometry-checked250–300m candidate lanes; this is a disclosed combat review on that source map, not a full seeded Canyon course run. Ordinary return fire is enabled.

`KESSEL_COMBAT_STILL_ONLY=1` records the initial occupied view and source/placement receipt, then exits before a video. The first640×360 capture is `captures/old_renderer_winter_canyon_640_bayer_still_v1_20261001_first.png`; six clear250/285m lanes were available at the authored start. Blizzard override2.5 produces a heavy whiteout (opening fog0.03673, snow2.4088); distant targets are largely obscured. That still is a composition/terrain observation, not a completed motion or combat test.

The later winter build includes all eight interior-fire emitters in the existing cube-shadow bank (25slots total), without changing emitted power. Shadow-bank tests12/12, raster shader/ABI tests14/14 and the winter-ground seasonal regression1/1 passed. This does not resolve every older renderer limitation.


## Manual elevation and display comparison

New winter captures now set a desired gun lay through the ordinary mechanical drives; the earlier baseline films retain their disclosed direct gun posing. The manual elevation source uses the retained `WHEEL1_POS` transformed with the actual turret, measured pitch motion, and its own positioned loop. Initial/reset poses and discontinuous jumps are silent; stopped or destroyed mechanisms retire the voice. Manual elevation does not depend on the engine running. Its75dBA at1m full-speed reference is an authored estimate, with distance, enclosure, obstruction, headset and the shared hearing stage applied. The loop is synthesized, not a historical recording. Four elevation/vehicle/fire-loop regressions passed.

New combat receipts disclose the86dBA at1m fire reference (previous74), ignition estimates106dB peak/92dBA at1m, and manual elevation reference. These are source-level changes; captured audio receives no offline boost or replacement. Incoming-hit events also retain the actual damage-sound category and resolved detonation/catastrophe flags.

The `obra-dinn-video` flag requests a matched native640 comparison: current Bayer4 versus camera-centered, world-oriented sphere Bayer8 with2×2 threshold samples. It uses the same frozen HDR/camera/exposure/particles and single live audio stream. The cupola makes a12° ordinary head-yaw arc during1–3s, then returns before firing; neither comparison changes gun motion. This is an algorithm-inspired colored comparison, not a claim of reproducing Obra Dinn’s complete renderer or palette. The default remains Bayer4 pending user review.
