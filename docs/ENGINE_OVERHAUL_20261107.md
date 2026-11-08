# Engine overhaul: baseline and first slice

The current engine misses the 16.67 ms interactive deadline. This branch is a starting point for
replacement of the expensive execution paths, not a declaration that the problem is fixed.
The integration checkpoint preserves the existing native and web implementations separately
from the experimental work below. `docs/INTEGRATION_AUDIT_20261107.md` records excluded experiments.

## Evidence and failure mechanism

The recorded November 5 live session had 243 active frames above 33.33 ms combined CPU work.
Those frames averaged 118.06 ms simulation plus 22.25 ms render preparation. Interior fire
averaged 56.02 ms, including 46.18 ms smoke clearance; blood averaged 25.09 ms. These are inclusive
phase costs, so child phases cannot be added to their parents. See `EDITOR_FRAME_COSTS_20261105.md`.
The old logger clamps its `dt_ms` to 100 ms, so it cannot certify actual worst frame intervals.

The range also constructs a full playtest world before disabling opponents. The November 7
bridge transition probe built 30,433 TLAS instances, 8,781 LOS occluders, 60 infantry actors and
79 cabin collision frames from 3,787,792 source triangles. Scene complexity is not equivalent
to the visible one-tank workload. This is an architectural issue to fix, not a reason to relax
the requirement. Repeated catch-up updates amplify the expensive per-particle work.

Interactive FEA budgets are already lower than reference rendering: 1,500 plate particles and
three projectile cells across versus 6,000/eight. But sequential material work still occupies
the main thread, and per-effect count caps are not execution-time budgets. Disabling visual
coverage/lifetime is not the proposed remedy; reduce numerical refinement first.

## Implemented first slice

`src/voxel_chunks.rs` and the standalone `voxel_chunk_probe` binary provide an experimental
destruction store independent of `Game`, the renderer, tank meshes and FEA:

- Empty space has no cell allocation; occupied 16³ chunks carry material IDs.
- Swept queries read current voxel state over the supplied elapsed segment. No future impact
  is resolved at launch, and collision does not wait for a render-mesh update.
- Local changes invalidate the containing chunk and only affected face neighbors.
- Greedy meshing emits exposed equal-material rectangles. Removing a chunk publishes an
  empty mesh so its previously displayed geometry can be retired.
- A FIFO queue coalesces edits and publishes a caller-limited number of chunks per update.
  This is a work-count bound, not a hard millisecond guarantee.
- The four new primary-account concept models are occupied-volume fixtures, with actual
  source cell sizes and material palettes. Their exported GLBs are art previews, not colliders.

The lab's spherical eraser is an authored editing tool, **not** a calibrated ballistic, thermal
or structural solver. It does not model momentum, fracture, detachment, debris or FEA. The
current gameplay engine does not yet consume the new chunk store. Traversal of very long empty
segments still needs chunk-level skipping/streaming limits before production use.

The native range now logs uncapped `interval_ms`, clamped simulation `dt_ms`, surface-acquisition
cost, CPU presentation-call cost and total frame work separately (`panzer.range-frame.v2`).
The HUD FPS estimate also uses the uncapped interval. A proposed GPU-timestamp hookup in the
range probe stalled in its first trial and was removed before commit. The existing standalone
GPU profiling support is not yet validated in this range path. Normal play performs no new
synchronous timestamp readback. CPU submission time and a synchronized image-readback benchmark
still are not measurements of actual display delivery.

### Reproduce the local-destruction checks

```sh
cargo test --release --bin voxel_chunk_probe
cargo run --release --bin voxel_chunk_probe -- \
  --report target/voxel-chunk-probe.json --obj-dir target/voxel-chunk-meshes --budget 2
```

The independent run receipt is `docs/benchmarks/voxel-chunks-20261107.json`. It records all four
source identities, material mappings, geometry counts, local edits and exploratory timings.
All 682,407 input cells were retained before edits. Only 2/2/4/12 chunks were republished across
the four scenes, with no unrelated chunk publication. Maximum two-chunk publication batches
were 0.191/0.198/0.224/0.518 ms in that run. These exclude rendering, physical solvers, fixture
parsing and initial meshing, and are **not gameplay frame times**. Initial loading and publication
are separately reported. Greedy meshes preserve the exposed cell-face area and material IDs.

## Next integration order and gates

1. **Explicit scene scope.** Construct RANGE without campaign populations and reserved campaign
   wreck machinery. Preserve the chosen environment and Tiger. BATTLE opts into combat systems;
   WORLD opts into authoring. Test mode transitions instead of hiding actors after full creation.
2. **Simulation ownership.** Move expensive sequential material/contact advancement off the
   presentation thread into persistent workers; publish immutable completed states. Track
   simulation debt explicitly. Never skip contacts or calculate future crew damage to catch up.
   Separate interactive numerical refinement from offline reference quality at construction.
3. **Shared contact acceleration.** Persistent spatial indices for static and articulated
   geometry; rebuild only changed pieces. Batch particle queries. Keep near-surface collision
   correctness while reducing refinement of visual transport work.
4. **Incremental scene publication.** Connect dirty voxel chunks to persistent GPU mesh pages,
   local collision updates and structural islands. Only affected chunks are remeshed/uploaded;
   distant dormant chunks consume storage, not simulation work.
5. **Renderer budgets.** Use GPU timestamps to budget transport, lights, volumes and visible
   optics. Update auxiliary views only when visible/stale. Bound uploads and allocation churn.
   Retain the intended colours and effects; validate matched-camera images after each change.

Each step must retain a rollback point and causal collision/reset tests. Test live windows at
the same internal resolution, scene, shot, seed, quality, power state and software build. Record
raw intervals, CPU phases, GPU passes, simulation debt, queue backlog and allocation peaks.
Report median/p95/p99/worst and every 33.33 ms miss; averages do not establish locked 60 FPS.
The workload ladder is an idle Tiger, one penetrating strike with all effects, repeated strikes,
then a voxel building under local destruction, and finally larger combined-arms scenes.

## Validation boundary on November 7

The native integration tests and WORLD save/reload probe, browser interaction checks, and
voxel-core tests establish bounded functional claims. The workstation also had another
graphics application active during this session. CPU lab timings are exploratory and no clean
post-change live FPS comparison is claimed. The game-dev performance CLI was unavailable;
repository-native probes were used, not sealed game-dev run bundles.
