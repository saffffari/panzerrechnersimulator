# Interior and sight frame support — 3 October 2026

The native sight composite renders a 6 km exterior camera and then a 24 m cabin
surround. Previously each view independently derived the source-light domain from
its current camera. That alternated the coarse environment atlas and every point
source's depth range twice per presented frame, even while all scene geometry was
unchanged. The static cache therefore rebuilt instead of being reused.

The deterministic source-scene gunner baseline reproduced the reported slowdown:
272.13 ms median and 295.55 ms p95, with a display-sized 5120 × 2880 final blit and
an 800 × 450 internal scene. GPU timings attributed 143.87 ms/frame to two static
sky-atlas redraws and 47.75 ms/frame to 72 static point-shadow faces. The receipt is
`artifacts/runtime/interior_gunner_precache_20261003.json`. These are fenced
headless workload timings, not compositor/vsync FPS. The baseline includes the
concurrent visual hotfixes but precedes the source-support cache fix.

At `begin_coop_shadow_frame`, the renderer now retains the pre-clip camera domain
for the whole native frame. The domain also includes the full camera far range
around the tank's 24 m protected crew rig, so a different pupil or scope exit has
conservative coverage at its own location. Every view still unions in its currently visible source
geometry. Both environment and point-source maps consult the same domain. At frame
end the retained domain is released; standalone diagnostics retain their own camera
support. No caster, light, shadow direction, map resolution, source power, or
physical visibility test is removed. Interior geometry still uses its short drawing
frustum.

The protected surround now shares the exterior's conservative coarse domain rather
than changing it to the 24 m camera plus loaded geometry. This can change coarse
texel spacing and depth quantization. The fine tank-centred 4.5 m environment
footprint is unchanged. Image acceptance is therefore still necessary; this is not
a claim of byte-identical shadow images.

`winter-bench commander` and `winter-bench gunner` reproduce the actual closed-cabin
and two-pass sight graphs. They pin deployment seed 19421003 unless the caller sets
`KESSEL_CANYON_SEED`; the comparison above explicitly uses 20261003. Optional
`KESSEL_WINTER_BENCH_SURFACE=5120x2880` measures the final display-sized blit, and
`KESSEL_WINTER_BENCH_DRIVE=0.65` drives the real vehicle to expose movement costs.
`KESSEL_GPU_PROFILE=1` enables query timings; new labels cover optical sky, weather,
and panel composition that were absent from earlier GPU reports. Unique
`KESSEL_WINTER_BENCH_TAG` values preserve each native still.

The complete-frame candidate measured 40.12 ms median / 46.34 ms p95 for the
same stationary gunner workload, versus 272.13 / 295.55 ms before the cache fix.
That is a 6.78x improvement in median fenced frame throughput. Static maps recorded
zero redraws after warmup, and shadow passes fell from 148 to 37 per frame. This
measurement precedes the additional pupil-support margin and atlas partition below;
it does not establish native presentation or moving-scene performance.
Receipt: `artifacts/runtime/interior_gunner_framecache_20261003.json`.

## Driving cache partition

The atlas now tracks four independent static footprints: 32 near environment tiles,
32 coarse environment tiles, the near sun tile, and the coarse sun tile. Movement
across a near footprint cell rebuilds only the near environment and near sun.
Changes in sun direction rebuild both sun tiles. A static source revision, including
instance transforms, counts and hidden masks, still invalidates all four groups.
Dirty work remains pending until it is actually encoded.

A depth-only scissored clear resets only dirty tile groups, preserving all other
static depths. Dynamic composition still copies the complete static depth atlas and
redraws all current dynamic casters, because WebGPU depth copies require complete
subresources. This avoids leaving old moving-body shadows in unchanged static tiles.
No dynamic update rate is reduced.

Sky terrain draws use the exact shadow-only spatial triangle pages provided by
`terrain_shadow_pages.rs`; the regular scene and ray geometry retain their original
indices. No terrain triangle is simplified, substituted or dropped on distance.
The old monolithic source was about 2.1 million triangles per direction.

CPU regressions cover frame support at every pupil, next-frame release, independent
near/coarse/sun invalidation, static and dynamic source changes, pending work, and
all 66 tile rectangles. An ignored GPU regression exercises the production partial
clear pipeline and reads every depth texel after clearing each group, checking both
selected far depths and exact retention elsewhere:

```sh
cargo test partial_clear_gpu_preserves_clean_groups_and_clears_selected_group -- --ignored --nocapture
```

The atlas-partition and spatial-page measurements, GPU validation and visual review
are still pending the parent-controlled build and benchmark. The changes above must
not be presented as a measured driving improvement until those receipts exist.
