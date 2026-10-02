# Cabin weather correction in the old raster renderer

The manual-driver review exposed an exterior-weather leak. `stylized_smoke.wgsl`
integrated the world weather from the eye to interior geometry and lit that medium
with full outdoor sky/sun visibility. Lowering the separate cabin haze did not
remove this path. Camera depth reconstruction was already correct: the inverse
view projection reconstructs camera-relative vectors.

The old renderer now reuses MAIN's `cabin_medium` representation. A 2 cm XZ grid
stores source floor/roof Y intersections from the Tiger GLBs in independently
articulated hull, turret and cupola frames. Explicit interfaces at source rims
partition the connected air between frames. These interfaces never block light.
World-segment traversal excludes these cabin intervals from outdoor-weather
extinction. A ray leaving an opening retains all exterior weather beyond it.
Cabin haze occupies the same source domain; its old raster coefficient remains
0.010, with the existing game strength and dust controls unchanged.

Main-view volume, optical captures, fine HDR effects and weather-particle
uniforms share the source domain and pose. The existing opaque geometry depth
still terminates camera paths. Fine flame/soot generation, particle identities,
plume shapes, density and pixel-coverage laws are unchanged. Local-light medium
attenuation follows the same domain split. World fog's sky/sun source now uses
the existing raster geometry-visibility atlas at its optical-depth centroid.
The old raster visibility approximations remain; this does not introduce MAIN's
software visibility BVH.

Snow in the modeled cabin is rejected by each flake's world position. The
legacy camera-near building shelter estimate is skipped for an observer inside
this source cabin, so snow immediately outside its apertures remains present.
The generic building shelter estimate remains an unrelated approximation.

Limits: the medium is a sampled source enclosure, not an exact air/steel Boolean
or a ventilation solver. It includes internal solid objects and armor thickness;
visible geometry still stops the light/view rays. Nearest 2 cm columns can have
boundary sampling error. Medium source lighting retains one optical-depth-centroid
visibility estimate. Opening a hatch does not instantly replace all compartment
air with outdoor snow. The domain source is cached for the process's fixed asset
root. No lamp intensity, exposure, bloom, fire or audio controls were retuned.

Validation is recorded in `artifacts/runtime/old_cabin_domain_*_20261001.log`.
A matching manual-driver capture is still required for visual acceptance.

CPU validation: `cargo check --bin panzerrechnersystem` passed. The relevant
shader, particle, source-domain, fine-effect, optical-weather and uniform-layout
checks passed43/43. Two broader legacy suites passed27 additional checks and
retained3 unrelated failures: `physical_truth_contract` expects the removed
literal `BAYER[bi]` and forbids the existing raster `glass_pipe`;
`volumetric_temporal_contract` also expects the old Bayer literal. No checks were
relaxed to hide these failures. The frozen unit-test executable was written at
19:40:14 PDT. No GPU or gameplay capture was run for this port checkpoint.


## Cabin correction accepted in native v4 captures

`winter_manual_drive800_v4_20261001.mp4` passes5/5 at800x450,30fps,240frames/8s.
The setup, initial state, all480 simulation samples and audio WAV are identical to manual v2.
Root and the capture agent inspected the matched4.5s view: the broad gray cabin veil is gone,
with local torch illumination and clear geometry/shadows. Torch-lit nearby surfaces remain
very bright; this confirms the weather leak correction without settling aesthetic exposure.
The review page includes matched before/after images.

`smoke_candidate_blizzard800_v4_20261001.mp4` passes4/4 at800x450,30fps,360frames/12s,
with the same2.5 storm fixture as the prior smoke capture. Root and the capture agent inspected
near/deck views: connected fine smoke and warm fire are preserved. Zero missing identities or
saturation over360frames; maximum grain weight0.0797966. Flame synthesis is unchanged, but
geometry-visible medium lighting changes some pixels (not a pixel-identical image claim).
Both complete videos decode successfully; adjacent verification JSON records details.

Final reviewed OLD binary SHA256:
`2501feb81e4e6300ff1334e6f80e684be7406136a880c56ea8964241e2c7a7b0`.
Release build68s;302 Rust/WGSL source hashes in `artifacts/runtime/playtest_review_build_v4_20261001.json`.
Cabin port: cargo check and43/43 relevant domain/shader/particle/optic/ABI tests pass.
Three unchanged legacy literal assertions in broader suites remain documented in
`docs/OLD_CABIN_WEATHER_DOMAIN_20261001.md`; this is not an all-tests-green claim.
