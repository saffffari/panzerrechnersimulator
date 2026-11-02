# Live-frame timing audit — 31 October 2026

This is a source-and-log audit, not a new performance measurement. The parent ran all builds and GPU comparisons. The subagent did not run Cargo or a GPU process. Source pointers are current at the build-41 instrumentation checkpoint.

## Latest measured result: build 41

The parent completed the added-timestamp diagnostic on the RTX 4090, Ultra, 640 × 360, ordinary single-player Commander view, 60 warmup frames and 180 measured frames. `KESSEL_BENCH_LIVE_FRAME=1` includes simulation and the ordinary RTT/shared-shadow path. It fences the queue after every measured frame. Both experimental geometry range flags were disabled.

- Total mean: **71.2403 ms**, 14.0370 FPS; total p99: 80.7038 ms.
- Simulation mean: **3.7851 ms**; render-and-wait mean: **67.4534 ms**.
- The sum of all 33 printed GPU pass means is **49.267 ms**.
- Render-and-wait minus that sum is **18.1864 ms of unclassified time**, not a measurement of CPU overhead.
- The environment probe totals **1.890 ms**: directions 1.701, finish 0.133 and cloud grid 0.056 ms. It is not the primary explanation for the frame-time gap; the serial finish alone is only 0.133 ms.
- Scene acceleration builds are **0.762 ms on the GPU**. Optical MSAA depth resolve is **0.013 ms across four passes**. Neither is a hidden multi-millisecond dominant cost in this fixture.
- Reported CPU sub-timers: TLAS rewrite/record 0.33 ms, encoder finalization 4.81 ms, queue submission 0.81 ms. These incomplete host-side spans may contain driver back-pressure and cannot simply be added to all GPU intervals.
- Last-sample raymarch `prepare_ms=0.7348` and `encode_ms=2.4637` are not means over 180 frames.

Evidence: `artifacts/renderer-performance-20261031/b41_timing_breakdown.json` and `b41_profile_ultra640.log`. The older build-39 result was total 70.9242 ms, render 67.1360 ms and printed GPU sum 46.822 ms, with 20.3140 ms unclassified before the added timers. These are separate runs; their difference is not a measurement of the instrumentation's overhead or of a performance improvement.

## Every reported build-41 GPU pass

| Label | Mean ms/frame | Passes/frame |
| --- | ---: | ---: |
| `raster-optic-exterior` | 15.275 | 4.0 |
| `rm-gbuffer` | 9.468 | 1.0 |
| `raster-optic-sky` | 8.027 | 4.0 |
| `rm-point-face` | 2.661 | 12.0 |
| `rm-sky` | 2.382 | 1.0 |
| `sky-visibility-dynamic` | 1.818 | 1.0 |
| `environment-probe-directions` | 1.701 | 1.0 |
| `rm-sky-cover` | 1.501 | 1.0 |
| `exposure` | 1.443 | 1.0 |
| `rm-integrate` | 1.337 | 1.0 |
| `scene-acceleration-builds` | 0.762 | 1.0 |
| `rm-shade` | 0.687 | 1.0 |
| `raster-optic-current-weather` | 0.655 | 4.0 |
| `rm-shadow-far` | 0.395 | 1.0 |
| `rm-shadow-near` | 0.373 | 1.0 |
| `rm-volume` | 0.221 | 1.0 |
| `environment-probe-finish` | 0.133 | 1.0 |
| `rm-cloud-shadow` | 0.084 | 1.0 |
| `sky-visibility-static` | 0.079 | 0.1 |
| `rm-reflect` | 0.068 | 1.0 |
| `environment-probe-cloud-grid` | 0.056 | 1.0 |
| `rm-glass` | 0.045 | 1.0 |
| `bloom-glare` | 0.016 | 1.0 |
| `raster-optic-depth-resolve` | 0.013 | 4.0 |
| `bloom-composite` | 0.011 | 1.0 |
| `bloom-bright` | 0.010 | 1.0 |
| `bloom-glare-composite` | 0.010 | 1.0 |
| `bloom-glare-down` | 0.009 | 5.0 |
| `crush` | 0.007 | 1.0 |
| `raymarch-optic-relays` | 0.006 | 1.0 |
| `rm-points` | 0.006 | 1.0 |
| `bloom-blur-h` | 0.004 | 1.0 |
| `bloom-blur-v` | 0.004 | 1.0 |

These intervals are not a complete critical-path decomposition. Timestamp boundaries omit some commands, synchronization and transfers; independent GPU work may overlap, and host work may overlap GPU execution. Rounded pass means also do not reconstruct every measured frame.

The build-41 optical exterior, sky, weather, relay composite and depth resolve means sum to **23.976 ms**. Including the 1.890 ms environment probe gives **25.866 ms of listed optical-related GPU work**. Subtracting that from total time leaves 45.3743 ms (about 22.0 FPS), but this is only budget arithmetic: some related host work could disappear too, and intervals do not establish the whole GPU critical path. It neither proves an attainable FPS nor authorizes removing required optical views. The large non-optical geometry pass (`rm-gbuffer`, 9.468 ms), other GPU work and unclassified remainder still need attention. This stationary fixture cannot establish a prolonged-combat minimum FPS.

## Previously untimed work, now measured in build 41

`src/environment_probe.rs::EnvironmentProbe::encode`, called by `src/renderer_raymarch.rs::encode` when relays are present, previously recorded three dispatches in one untimed compute pass, followed by a storage-to-uniform copy:

1. `probe_directions`: 32 direction workgroups of 16 lanes. Upper-hemisphere cells evaluate cone samples of the shared physical sky transport. Each participating lane independently evaluates `probe_solar` at the same origin before its distinct sky query.
2. `probe_finish`: one lane accumulates the directional quadrature and evaluates 12 ground-disc solar samples serially before deriving beam/diffuse/ground fields.
3. `probe_cloud_grid`: 6 × 6 workgroups of 8 × 8 lanes, producing the 48 × 48 local cloud field; the center also computes a beam-disc cloud mean.

These are GPU lighting calculations. Their omission from the old pass table was not evidence of CPU cost. Build 41 now measures all three; their combined 1.890 ms makes probe reuse a secondary opportunity, not the leading plan for reaching 30 or 60 FPS. The primary raymarch sky cache has a different angular representation and update scheme; replacing objective-origin cloud transport with that cache would not automatically preserve optical truth.

Other missing spans included acceleration-structure build commands (the 0.33 ms timer measures host recording, not GPU building) and MSAA nearest-depth resolves for optical captures. Texture/buffer copies, transitions, and any inter-pass scheduling gaps still need careful interpretation. Particle/tracer and scatter paths may contain additional work in other fixtures; the present receipt has zero tracer and spark records, and the current Act-I build does not activate the optional terrain detail scatter field.

The prepared instrumentation patch was applied by the parent for build 41. It adds optional timestamps inside the existing probe pass, without splitting or reordering it; aggregate fallback on adapters without in-pass timestamps; an encoder span for scene acceleration builds; and labels for optical/main depth resolve. It changes no physical samples or dispatch dimensions. Records: `artifacts/renderer-performance-20261031/missing_gpu_timestamps.prepared.patch` and `.prepared.json`.

## What fenced gfxbench measures versus the window

`src/main.rs::run_gfxbench` updates with the same fixed 1/60-second input step and, with LIVE_FRAME, enables the solo crew and deferred RTT scheduling. Its render timer begins before shared shadow/RTT setup, then includes the render submission and a full `device.poll(wait_indefinitely())`. GPU timestamp readback occurs afterward and is excluded from that timer, but it further serializes the benchmark's wall-clock loop. Source loading, warmup and the final reference PNG are also outside its reported frame means.

`Renderer::render_offscreen` and `render_to_surface` share uniform preparation, track stamps, acceleration updates, temporal preparation and the internal render graph. Encoder finalization exists in both; the 4.81 ms observed host finalization is not benchmark-only book-keeping. The window also performs its final blit and present. Normal window rendering does **not** fence every frame: the surface requests a maximum frame latency of two, and the next frame's host preparation can overlap prior GPU work. Surface acquisition/presentation can still apply back-pressure or vsync. An opt-in session GPU sample can fence an individual sampled frame; it is not the default every-frame loop.

All relevant submissions use the same wgpu queue. There is no explicit independent asynchronous-compute queue. Some GPU scheduling overlap is driver-dependent; required resource dependencies, per-port scratch reuse, and optical geometry/depth/sky ordering remain. Two frames in flight therefore do not imply two complete GPU frames execute independently or halve the frame interval.

The existing `cpuprof` two-frame option demonstrates the queue-marker technique, but is **not a matched replacement for this gfxbench result**: it constructs a seeded campaign attempt and uses nonzero drive/traverse input with a different warmup and encounter setup. Its numbers must not be used as the missing half of the Commander A/B pair.

## Prepared matched throughput experiment

`artifacts/renderer-performance-20261031/same_fixture_pipelined_gfxbench.prepared.patch` changes only `run_gfxbench` and is prepared, not measured. It requires both `KESSEL_BENCH_LIVE_FRAME=1` and `KESSEL_BENCH_PIPELINED=1`, and rejects `KESSEL_GPU_PROFILE=1` before source/device construction to avoid shared query-buffer/readback synchronization.

It preserves the existing fixture, input sequence, update step, camera/optical views, RTT and shadow scheduling. Completion markers bound the queue to two frames. Warmup is drained before the timer begins; the final drain is included, and each measured marker must be accounted for exactly once. Its distinct receipt reports elapsed time, completed-frame count and completed headless throughput only. It deliberately omits per-frame latency, p99 and GPU-pass claims. The original fenced report remains unchanged. A final reference capture is outside the timed throughput interval.

For the matched comparison, keep all fixture/preset/resolution/candidate flags identical, disable GPU profiling in **both** runs, and vary only `KESSEL_BENCH_PIPELINED`. This does not replace the separately profiled run or a later real-window/long-combat check.

## Next measured target after the queued-throughput comparison

First complete the same-build fenced versus two-frame-throughput pair with GPU profiling disabled in both. That will establish how much the compulsory per-frame fence affects completed throughput. It will not by itself identify the remaining 18.1864 ms or provide input/display latency. Keep the regular profiled diagnostic as a separate result.

The next diagnostic target should be the **optical exterior pass (15.275 ms across four views)**, alongside a whole-render GPU envelope and bounded host-stage timings to classify the remainder. Split optical exterior timings by actual port and correlate them with submitted source work; the existing total conceals which objective dominates. Distinguish geometry/vertex and shaded-fragment pressure before choosing an optimization. The 9.468 ms main geometry pass is the next substantial source-geometry cost to compare. If the queued run instead reveals a large host limit, prioritize command preparation/finalization and draw submission measurements before rewriting a shader. This is a measurement recommendation, not an implemented optimization or a promised FPS gain.

Do **not** pursue probe caching as the primary solution based on build 41. Directions cost 1.701 ms, finish 0.133 and cloud grid 0.056. Exact reuse of identical solar inputs or parallel beam queries may be valid later, but even their impossible zero-cost limit does not explain the target gap. The exposure pass (1.443 ms) and volume pass (0.221 ms) are also secondary in this fixture.

The two measured geometry-range candidates preserved the image but did not improve total time and remain disabled by default. An exact visibility-first optical shading design or source-order indirect multi-draw may eventually address larger work; both need a measured bottleneck and separate correctness treatment for alpha discard, per-sample coverage, original equal-depth winners and source IDs. Replacing turning periscopes with straight windows, reducing nonzero radiance, or silently degrading optical transport is outside this recommendation. No new optimization was implemented by this audit update.

## Measurement validity update (2026-10-31)

The owner subsequently identified another GPU-intensive application (Kerbal Space Program) using approximately 98% GPU outside the game. The earlier absolute FPS/latency figures below were not collected with a demonstrated exclusive GPU workload and must not be presented as the game's isolated hardware capacity. Pass identities, instrumentation gaps and source analysis remain useful diagnostic evidence. Candidate speed differences require a fresh controlled same-build baseline before being promoted; no current FPS target is established by those earlier runs.
