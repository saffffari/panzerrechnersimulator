# Native winter renderer performance checkpoint

The final candidate renders the matched native800×450 Canyon benchmark in **33.31 ms mean** versus **143.04 ms** before the bounded cache/draw-order changes (4.29× faster). The measured reciprocal mean is 30.03 frames/s. This is 60 fenced render frames following 60 warmup frames; `Game::update` occurs outside the timer. It narrowly reaches the requested 33.33 ms render mean, with **117.29 ms p99** remaining. It does not establish interactive frame rate or acceptance across combat, driving, closed-cabin and exterior views.

| Checkpoint | Mean ms | p50 ms | p99 ms | Minimum ms |
|---|---:|---:|---:|---:|
| Baseline | 143.04 | 174.92 | 178.41 | 38.58 |
| Candidate1: caster cache partition/coverage | 52.93 | 52.67 | 136.28 | 40.51 |
| Candidate2: same opaque geometry before world batches | 40.47 | 40.26 | 123.77 | 28.73 |
| Candidate3: shared per-layer source fingerprints | 33.31 | 32.92 | 117.29 | 21.12 |

Every row uses Vulkan on RTX4090, the actual `gaea-canyon-run-v2/manifest.v1.json`, shader/source-map backend, MSAA4, screen Bayer8 and native800×450. The fixture selects commander and open hatch; its actual camera remains within the cupola looking through ports. It is not a head-out exterior view. There are 31,706 district instances in 4,365 batches/pages, 60 Soviet actors and six T-34s. The intentionally preserved first accidental urban-manifest profile is excluded from this comparison.

Candidate1 moved changing source-instance pools from the static map layer into the dynamic layer, retaining all participating source triangles. A conservative projected-bounds test skips a dynamic draw only when its bounds cannot cover any depth sample, with an eighth-pixel guard and ambiguous projections retained. An explicit articulated-pose revision prevents in-place vertex updates from reusing stale depth. The final frame was byte-identical to baseline.

Candidate2 draws the same visible opaque loose meshes from near to far before instanced world batches, allowing real tank/interior depth to reject hidden world shading. It changes no material, transform, lighting or caster membership. Its final frame changes six of 360,000 pixels versus baseline, maximum channel difference 11/255, mean absolute channel difference 0.0000583/255. No pixel changes by more than 32/255.

Candidate3 hashes each complete static/dynamic source layer once per preparation, then combines that exact source fingerprint with each light pose, emitting-mesh identity and far range. It avoids repeatedly hashing all world matrices for each lamp. Including each emitter in the shared source fingerprint can trigger an extra rebuild but cannot suppress an occluder change. Its final frame is byte-identical to candidate2.

The final GPU timestamp means are 7.141 ms for six optical exterior passes, 6.307 ms for dynamic sky visibility, 6.177 ms for the main raster scene, 2.331 ms for amortized static sky visibility, 0.814 ms for smoke and 0.350 ms for dynamic point depth. GPU timestamps are pass averages, not a complete additive CPU/GPU critical-path decomposition. Rare full static sky work remains in this run; the p99 frame is not eliminated. No additional optimization was attempted after the third measured candidate.

The finite shadow-map approximations are unchanged: 384-pixel local sky tiles and very coarse global coverage (here ±10,496 m), receiver bias, missing reflected indirect light, straight-through transmitting glass, and excluded grass/transient carriers as solid casters. Point maps cover all32 admitted lights with source-derived far bounds. These changes improve execution of that approximation; they do not establish exact transport. The experimental portable source-BVH mode is retained separately.

Evidence:

- `artifacts/runtime/renderer_performance_candidates_20261001.json`: exact four-run values, binary hashes, image/log paths and GPU pass times.
- `artifacts/runtime/shadow_cache_candidate3_image_difference_20261001.json`: matched final image differences.
- `artifacts/runtime/winter_release_sealed_focused_tests_20261001.json`: 69 passed selected CPU tests, one separate GPU test ignored. Point-shadow15, sky4, AA1 and projected-coverage2 regressions are included.
- `artifacts/runtime/map_visibility_composition_20261001.log`: composed shaders18/18 passed for default maps and experimental portable paths.
- Final release SHA256: `3fdd621565275a40e02067c298f86ff5943dfbb0f6f3c61bcc8e3fc3d4de5bd1`.

The renderer source is frozen at this checkpoint. Root-owned moving-scene and audio/combat captures are the next acceptance stage.
