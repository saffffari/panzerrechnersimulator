# Optical render work audit, 31 October 2026

This began as a source audit and a bounded next-candidate recommendation. The parent subsequently approved the per-instance candidate below; its source checkpoint is recorded at the end. Builds, GPU execution, and visual acceptance remain with the parent agent.

## Matched evidence

The same-build b37 live-frame commander comparison used Ultra, 640 x 360, and the RTX 4090. The baseline explicitly retained redundant black-eyepiece work; the candidate skipped it only when the existing transmission multiplier was exactly zero.

| Measurement | Baseline | Candidate |
| --- | ---: | ---: |
| Total ordinary live-frame mean | 72.260 ms | 72.228 ms |
| Total FPS | 13.839 | 13.845 |
| Render mean | 68.394 ms | 68.342 ms |
| Simulation mean | 3.863 ms | 3.884 ms |
| Exterior relay GPU time | 15.537 ms | 15.422 ms |
| Relay sky GPU time | 8.322 ms | 8.263 ms |
| Exact-zero entrances | 0 | 0 |

The four prepared ports are 0, 1, and 2 at 320 x 180 with multiplier 1, and port 11 at 128 x 72 with multiplier 0.0070114201. Thus both runs do the same four captures. The approximately 0.04% total-frame difference is not a demonstrated performance gain. Parent inspection also found the saved images pixel-identical. The zero-only optimization is valid but does not address this view's bottleneck.

Evidence: `artifacts/renderer-performance-20261031/b37_baseline_ultra640.log`, `b37_candidate_ultra640.log`, and `captures/liveframe_20261031_b37_{baseline,candidate}_ultra640_commander.png`. These are fenced headless live-frame measurements, not a prolonged combat or desktop-presentation minimum-FPS guarantee.

## What is already avoided

- `src/raster_optics.rs::capture_camera` clips the real entrance polygon to the observer view and fits the objective camera to those rays. Each source objective retains its registered relay rotation and position. A folded periscope cannot be replaced by a straight window without changing the optical model.
- `aperture_extent` uses aspect-preserving resolution buckets with a two-source-samples-per-native-aperture-pixel requirement. The present captures total 182,016 source pixels; they are not four full 640 x 360 images.
- `encode_raster_scene`, the sky pass, depth resolve, and optical weather already set the active viewport and scissor. The full allocated texture clear is not evidence that all allocated pixels are shaded.
- The sky is drawn at far depth with an Equal depth test, after opaque geometry. It already avoids sky shading where opaque geometry covered the source sample.
- Ultra's source geometry uses 4x MSAA. The source sky shader does not request sample-index interpolation, so four coverage samples must not be described as four complete sky-shader executions per pixel.
- Existing particle budgets and static sky-visibility cache improvements remain in the source. The current expensive passes are not evidence that those earlier optimizations were reverted.

## Where repeated work remains

`src/renderer.rs` constructs the forward shader from `shaders/stylized_mesh.wgsl`, not the older `mesh.wgsl`. The actual `environment_light` integrates 32 environment directions; each contributing direction asks source-shadow-map visibility. The surface then evaluates the finite lamps. This is substantial work for every shaded fragment, including fragments later overwritten.

`src/raster_optics.rs::encode_raster_scene` submits visible loose meshes first, sorting terrain after the other loose meshes. It submits every visible instanced page afterward. Therefore an instanced building, tree, or vehicle can overwrite terrain that has already performed the environment-lighting solve. This is a real overdraw opportunity, but its share of the measured 15.5 ms has not been isolated.

The spatial index rejects whole instance pages. Once a page survives, the optical pass draws `0..inst_count`, including source instances outside that objective frustum. Invisible instances use zeroed GPU matrices, but the draw still reaches the vertex stage. The main geometry pass has the same page-level submission pattern. Narrow periscope fields are a reason to measure this conservatively, not proof of a large gain.

`shaders/sky.wgsl` evaluates the shared physical clear-atmosphere transport and then a cloud march at each actual objective origin. The cloud code uses 32 view steps and five light steps, with existing early exits. The atmosphere transport includes altitude quadrature and nested solar-transmittance integration. Four different objective views therefore repeat expensive physical work, even though their numerical viewports are small.

## Measured instance-range candidate

Conservatively reject individual instances outside each optical camera frustum, emitting contiguous surviving ranges in the original order. Keep the existing page query as the first stage. Add per-port counters for candidate and retained instances, source triangles, and emitted ranges. Initially limit the experiment to the optical pass so its cost and result are attributable.

Use the same affine source bounds and full grass-bend envelope as `dynamic_instanced_bounds`, including its outward floating-point margin. Preserve ambiguous/non-finite or near-plane cases. Never introduce an apparent-size threshold. Do not reuse `shadow_has_sample`: it describes a single-sample shadow-map grid, not the perspective 4x-MSAA capture.

This candidate leaves original vertex data, source transforms, material inputs, alpha evaluation, shadow maps, original Less-depth draw order, depth ties, all four coverage samples, and relay sampling unchanged. Splitting an instanced draw must preserve absolute instance indices and stable object IDs; no compaction or remapping is needed. Adjacent retained indices should share a draw range, and a fully retained page should keep its existing single draw.

Meaningful CPU cases: transformed and mirrored instances; wholly outside and boundary-touching bounds; near-plane crossing; large world coordinates; bent grass whose root is outside but tip is inside; hidden/unhidden members; and range identity/order. Compare the same live-frame benchmark and images, plus moving viewpoints and alpha-tested foliage. Reject the candidate if the retained count is nearly unchanged, command-encoding overhead grows more than GPU savings, or any source coverage changes. No FPS improvement is predicted before that measurement. Expected implementation scope is one culling helper, optical submission ranges, counters, and focused regressions.

## Why not make a quick opaque depth prepass instead?

A depth prepass could save more fragment lighting, but a naive prepass is not equivalent here:

1. The existing color pass uses Less, so the first coplanar source wins. Filling nearest depth first and changing color to Equal or LessEqual allows later equal-depth surfaces to win. Simply moving terrain behind instances has the same tie-order risk.
2. Grass alpha must use the same authored texture, sampler, UV interpolation, threshold, and coverage. The view-dependent microscopic-soot discard must agree too. An opaque silhouette proxy would incorrectly occlude the scene and sky.
3. Use the same invariant vertex projection, affine transform, complete grass displacement, viewport, sample count and sample mask. Single-sample prepass depth cannot stand in for four independent coverage samples.
4. The material shader uses centroid receiver interpolation plus pixel-center plane derivatives. A cheap pass must not change the surviving shading invocation's receiver or normal derivatives.
5. Transparent panes and relay entrances must stay out of opaque depth admission. Weather, particles, and the existing nearest-depth resolve must retain their ordering.

An exact nearest-winner ID attachment can retain the original winner and gate shading by both depth and ID, but that is a larger architecture change. A conservatively farther biased prepass followed by the unchanged original Less pass is another possible design; it would require a proof that every covered sample's prepass value is strictly farther than its real depth, including near/far clipping and floating-point/MSAA behavior. Do not assume a constant fragment-depth offset satisfies this for all four samples. Neither design is approved or implemented by this audit.

## Sky reuse safeguards

Reusing main-view or previous-port sky is not currently exact: the objective origin, ray direction, projection, cloud position/time, weather field, and physical environment can differ. A broad camera-stationary cache would freeze moving clouds; one shared cloud panorama would change local parallax. A physically specified atmosphere lookup representation could be a separate larger project, with numerical-error acceptance and independent cloud evaluation, but it should not be presented as removing duplicate identical queries. Reducing capture resolution, MSAA, sky steps, or nonzero eyepiece radiance is a quality/model tradeoff and is outside the equivalent candidate above.

Even the complete removal of both measured optical GPU passes would not by itself establish 60 FPS. The current total live-frame cost includes other GPU work and substantial CPU submission/encoding. Continue matched pass timing and full-frame measurements rather than treating one optical optimization as completion of the performance target.

## Approved instance candidate source checkpoint

Implemented only in `src/raster_optics.rs`; no shader, asset, main-view submission, shadow, material, sample-count, or optical-camera changes. The range iterator allocates no intermediate vector and retains absolute source ordinals. Its envelope uses the existing full moving/grass source bounds, followed by an outward cancellation allowance in the absolute-world plane test. Unknown source bounds or missing CPU transforms keep the instance. Known hidden instances are omitted because their existing GPU matrices produce no geometry.

Historical build-39 comparison: the baseline used both `KESSEL_BENCH_LIVE_FRAME=1` and `KESSEL_BENCH_OPTIC_ALL_INSTANCES=1`, and the candidate omitted the all-instances flag. This default was rejected after measurement; the current explicit opt-in contract is recorded below. The original exact-zero experiment remains independent.

`raster_optic_work_receipt()` now reports `benchmark_submits_all_instances` and an `instance_work` array indexed by port. Each record includes candidate batches/instances/triangles and actual retained instances/triangles/submitted ranges. It describes only the last encoded optical captures and source instanced geometry, excluding loose meshes and separately rendered fine VFX. It is not a fragment-count estimate.

Five new CPU tests cover the explicit diagnostic gate, stable ordinals with hidden gaps, edge/near-plane intersections, a bent grass tip entering from outside the frustum, and independently projected rotated/mirrored source samples at large battlefield coordinates. These tests were written but not run by the subagent; the parent owns native compilation and GPU validation.

Reversible candidate record:

- Before: `artifacts/renderer-performance-20261031/raster_optics.before_instance_frustum.rs`, SHA-256 `b5138818005cfcaf7103333b446c6f8db59763f2fb19e39e8ed017780ed7d6e7`.
- Isolated patch: `artifacts/renderer-performance-20261031/raster_optics.instance_frustum.patch`.
- Candidate source SHA-256: `f3476d459dc312b98efc271f308cbfd9d36035c4a8daf1f13b3e60f3c322082e`.
- `git diff --check -- src/raster_optics.rs` passed. Source remains LF. No performance or visual acceptance is claimed yet.


## Build 39 result: reject instance ranges as the shipping default

The same-executable Ultra 640 x 360 RTX 4090 pair completed 180 measured ordinary live frames after 60 warmup frames. Both saved images were pixel-identical in the parent's comparison.

| Measurement | Baseline | Per-instance candidate |
| --- | ---: | ---: |
| Total mean | 70.9242 ms | 72.1332 ms |
| Total FPS | 14.0996 | 13.8632 |
| Total p99 | 80.3482 ms | 81.3555 ms |
| Last-frame instanced source triangles, all four ports | 42,855,373 | 39,954,549 |
| Last-frame submitted instance ranges | 1,808 | 2,167 |

Source: `artifacts/renderer-performance-20261031/b39_comparison.json` and the matching `b39_{baseline,candidate}_ultra640.log`. The candidate removed about 6.8% of these submitted triangles while increasing ranges about 19.9%; total mean was about 1.7% slower. This pair does not establish an improvement. It also does not isolate whether CPU bounds work, command overhead, or run variation explains every part of the difference.

**Current source defaults to the original instance-page draw.** The rejected experiment now requires both `KESSEL_BENCH_LIVE_FRAME=1` and `KESSEL_BENCH_OPTIC_INSTANCE_CULLING=1`. The old `KESSEL_BENCH_OPTIC_ALL_INSTANCES` switch is no longer used. The gate is independent of graphics preset and is false without both exact opt-in values. The parent's build-39 compilation and all 16 focused CPU tests passed before this new gating change; the next build must validate the new checkpoint.

## Next isolated experiment: original-order loose-terrain ranges

The existing `terrain_shadow_pages` buffer groups triangles in Morton order. Although it retains the same triangles and winding, using that reordered buffer for color would change equal-depth ordering, including alpha-tested overlaps. It is therefore not reused for optical color.

The candidate adds CPU-only bounds over contiguous original triangle spans (up to 256 triangles per leaf, with a binary hierarchy). Metadata is prepared once from the original source only when **both** `KESSEL_BENCH_LIVE_FRAME=1` and `KESSEL_BENCH_OPTIC_TERRAIN_RANGES=1` are present. Only visible, immutable, loose `track_recv` meshes with existing eligible large-terrain metadata use it. Unknown/unavailable metadata falls back to the original full draw; mutable/deformed sources are excluded. Ordinary gameplay builds no new metadata and keeps its original submission.

Each admitted range addresses the original index buffer. Traversal stays in original order, merges adjacent survivors, retains duplicate triangles and winding, and never uses a projected-size or shadow-sample cutoff. Bounds include every source vertex, and each node uses the same conservatively inflated affine/world-frustum admission already tested for perspective capture. Viewport, coverage samples, vertex shader, materials/alpha, source IDs, depth state, tie order, shadows, relay mapping, and sky remain unchanged. No gameplay/contact geometry changes.

The receipt adds `benchmark_instance_culling`, `benchmark_terrain_ranges`, and per-port `terrain_candidate_meshes`, `terrain_paged_meshes`, `terrain_candidate_triangles`, `terrain_retained_triangles`, and `terrain_submitted_ranges` in the existing work record. For the next isolated pair, leave instance culling off in both runs and enable only the terrain switch in the candidate. The old b39 receipt field `benchmark_submits_all_instances` is historical.

Two added CPU regressions check original index order/winding/duplicate overlaps, crossing triangles without an interior vertex, actual omission of distant spans, transformed source sample coverage at battlefield coordinates, and nonfinite-frustum conservative fallback. No GPU test was run by this subagent. The next matched pair must inspect all optical images as well as frame/pass time; range-count overhead can still make this fail, so it is not admitted as a performance improvement.

Reversible originals are `artifacts/renderer-performance-20261031/raster_optics.before_source_terrain_ranges.rs` and `terrain_shadow_pages.before_source_terrain_ranges.rs`. The source checkpoint touches only `src/raster_optics.rs` and `src/terrain_shadow_pages.rs` plus this audit. No source assets, shaders, resolution, MSAA or lighting were changed.


## Build 40 result: keep terrain ranges experimental

The parent completed the matched same-binary Ultra 640 × 360 pair (180 frames, 60 warmup). The source-order terrain candidate produced a pixel-identical reference image. Baseline total was 71.0436 ms / 14.0759 FPS; candidate was 71.7541 ms / 13.9365 FPS. It reduced last-frame terrain submission across four ports from 8,372,232 to 1,504,743 triangles, but increased those terrain draws from 4 to 2,419 ranges. This pair demonstrates no frame-time improvement; the roughly 1% slower result does not establish a statistically isolated cause. It remains disabled unless both explicit benchmark gates are present.

Evidence: `artifacts/renderer-performance-20261031/b40_comparison.json` and matching build-40 logs. The parent reports all seven focused CPU tests passing. Further direct-draw subdivision is not justified by these results; missing GPU lighting spans and matched pipelined throughput are the next measurements, documented in `docs/LIVE_FRAME_TIMING_AUDIT_20261031.md`.
