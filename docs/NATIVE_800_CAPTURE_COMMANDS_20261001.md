# Native 800 capture handoff — source-checked, not executed

Run from `/home/alex/panzer/canyon-v2-20260930` after the MAIN binary is built. Linux `create_instance()` selects Vulkan and requests the high-performance adapter. Run these GPU jobs sequentially. Every prefix below must be unused. A timeout is a failed diagnostic, not a completed review. No interactive window is opened.

## First: actual AA resolve GPU test

```bash
cargo test --release --test native_geometry_aa native_geometry_aa_gpu_resolves_real_coverage_and_conservative_depth -- --ignored --exact --nocapture
```

This executes one native pixel with one of four covered source samples, fully covered pixels, clear sky and the production nearest-depth resolve. It does not establish scene aesthetics.

## Same moving winter blizzard: two AA runs, four comparison videos

Each run records eight seconds at 30fps. The occupied commander turns through the same 12-degree head arc during seconds1–3; this is camera motion, not vehicle driving. The two display outputs in each run reuse identical HDR, depth, exposure, camera and simulation. No `preview` option: it would sample at5fps and is unsuitable for judging shimmer.

```bash
timeout --signal=TERM --kill-after=10s 15m env -u KESSEL_COMBAT_STILL_ONLY -u KESSEL_SOURCE_VISIBILITY_REVIEW KESSEL_ASSET_ROOT=/home/alex/panzer/canyon-v2-20260930 KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_INTERNAL_RES=800x450 KESSEL_DITHER_PATTERN=screen_bayer8 KESSEL_GEOMETRY_AA=off ./target/release/panzerrechnersystem combat-scene-suite captures/main800_blizzard_aaoff_v1_20261001 winter_canyon 8 no-dither-video
timeout --signal=TERM --kill-after=10s 15m env -u KESSEL_COMBAT_STILL_ONLY -u KESSEL_SOURCE_VISIBILITY_REVIEW KESSEL_ASSET_ROOT=/home/alex/panzer/canyon-v2-20260930 KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_INTERNAL_RES=800x450 KESSEL_DITHER_PATTERN=screen_bayer8 KESSEL_GEOMETRY_AA=msaa4 ./target/release/panzerrechnersystem combat-scene-suite captures/main800_blizzard_msaa4_v1_20261001 winter_canyon 8 no-dither-video
```

Outputs for each prefix: `_screen_bayer8.mp4`, `_continuous.mp4`, three uncompressed `_screen_bayer8_native_N.png` / `_continuous_native_N.png` pairs, `.json`, and `_soundtrack.wav`. The ordinary `.mp4` also exists. H264 is not an exact-pixel oracle; inspect the native PNGs for palette/coverage and the videos for temporal behavior.

## Quiet incoming rifle fire: 12 seconds, own guns silent

```bash
timeout --signal=TERM --kill-after=10s 20m env -u KESSEL_COMBAT_STILL_ONLY -u KESSEL_SOURCE_VISIBILITY_REVIEW KESSEL_ASSET_ROOT=/home/alex/panzer/canyon-v2-20260930 KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_INTERNAL_RES=800x450 KESSEL_DITHER_PATTERN=screen_bayer8 KESSEL_GEOMETRY_AA=msaa4 ./target/release/panzerrechnersystem combat-scene-suite captures/main800_incoming_v1_20261001 winter_canyon_incoming_small_arms 12
```

Real seeded Soviet AI, ordinary ballistic source contacts and live captured audio; no impacts injected. Other course defenders remain and may mask sound. Rifle ball is unlit, so no tracer requirement is manufactured here. Receipts count geometry submissions, not visible pixels. This stationary head-out view retains ordinary impact feedback and actual lethal damage. A KIA/fade is disclosed, never bypassed.

## Ordinary coax plus lower-snow moving-camera comparison: 12 seconds

```bash
timeout --signal=TERM --kill-after=10s 20m env -u KESSEL_COMBAT_STILL_ONLY -u KESSEL_SOURCE_VISIBILITY_REVIEW KESSEL_ASSET_ROOT=/home/alex/panzer/canyon-v2-20260930 KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_INTERNAL_RES=800x450 KESSEL_DITHER_PATTERN=screen_bayer8 KESSEL_GEOMETRY_AA=msaa4 ./target/release/panzerrechnersystem combat-scene-suite captures/main800_coax_msaa4_v1_20261001 winter_canyon_infantry 12 no-dither-video
```

This uses the ordinary coax trigger toward an actual seeded infantry group, with authored one-in-five MG tracer selection. It has storm override0.35, not clear weather. The blizzard scene has override2.5 and different actor staging, so these two scenes must not be called a controlled weather-only A/B. No exact same-scene clear-day variant exists in the current CLI. To isolate lower-weather geometry AA as well, repeat the coax command with `KESSEL_GEOMETRY_AA=off` and prefix `main800_coax_aaoff_v1_20261001`; the dither pair remains identical-HDR within each run.

## Cabin/optical consistency spot-check (after exterior acceptance)

```bash
timeout --signal=TERM --kill-after=10s 15m env -u KESSEL_COMBAT_STILL_ONLY -u KESSEL_SOURCE_VISIBILITY_REVIEW KESSEL_ASSET_ROOT=/home/alex/panzer/canyon-v2-20260930 KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_INTERNAL_RES=800x450 KESSEL_DITHER_PATTERN=screen_bayer8 KESSEL_GEOMETRY_AA=msaa4 ./target/release/panzerrechnersystem combat-scene-suite captures/main800_inside_msaa4_v1_20261001 winter_canyon_inside 8
```

Stop expansion if MAIN produces unresolved-magenta optics or unacceptable capture cost. Neither alias nor sampling reduction suppresses the diagnostic. Keep that result as failed integration evidence.

## Receipt review

- Require the actual adapter, native/output dimensions800×450, 30fps and expected duration; inspect `geometry_aa.geometry_samples` (1 versus4).
- Require `passed` plus all named checks for gameplay acceptance. A retained failed clip can still diagnose an image defect, but cannot be relabeled accepted combat.
- Compare AA run initial player/clock/seed/lanes and per-second player/camera positions, camera-forward vectors, clock and impact counts. Compare discharge event kind/time/actor while excluding sensor-warmup exposure metadata. Independent deterministic reruns are not the frozen-HDR guarantee: report any state mismatch before visual attribution.
- For incoming, check own cannon/MG counts0, positive Soviet discharges, actual Player armor contacts/sparks and positive stamped arrivals with zero wavefront violations. The bounded512-entry audio and projectile logs may omit older details; lifetime counters and omitted-submission counts remain separate.
- Review audible report/crack/contact ordering against their source position and emitted/arrival timestamps. Audio receipts have no projectile IDs; do not assert an exact ID join.
- Nearest-depth AA remains conservative at mixed geometry/background pixels: transparent glass, entrance panels, particles and media are single sample. Inspect roof/port silhouettes for over-occlusion or dark fringes.

## MAIN compile/test filters

```bash
cargo test --release --test native_geometry_aa --test stylized_raster --test pixel_particle_rendering
cargo test --release --bin panzerrechnersystem renderer::renderer_aa::tests
cargo test --release --bin panzerrechnersystem renderer::renderer_fine_vfx::tests
cargo test --release --bin panzerrechnersystem renderer::cabin_shadows::tests
cargo test --release --bin panzerrechnersystem renderer::cabin_medium::tests
cargo test --release --bin panzerrechnersystem renderer::raster_optics::tests
cargo test --release --bin panzerrechnersystem combat_scene_suite::small_arms_review_tests
```

The first integration group validates the newly filtered material shader, composed shared lighting/media and CPU/GPU ABIs. Fine-VFX uses the MAIN composed visibility prefix. Run broader `physical_truth_contract` and `volumetric_temporal_contract` separately and inspect failures: known earlier literal/source-contract failures cannot be silently interpreted as passing or automatically blamed on this merge. Do not claim a filter matched tests without checking its count.

## Remaining acceptance risks

MAIN retains its portable source-triangle BVH and optical unsupported-path diagnostics. It is not OLD performance with AA added. Native GPU validation must cover actual composed pipelines and main/optical views; Naga success alone does not prove driver acceptance, correct image, or practical frame budget. Four-sample native opaque coverage does not solve alpha-tested subpixel coverage, geometric-normal highlight alias or all transparent edges. Filtered normal-length roughness is an isotropic approximation; visually judge sharpness and highlight changes. The approved cabin density coefficient is0.010 in MAIN regular and fine transport.
