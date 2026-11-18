# Performance audit and quality presets, 18 November 2026

The owner's ask: a full performance audit and gameplay bug test of the latest build (main at
`d15fda6`), the singleplayer solo crew bulletproof across the main-menu modes, and an adjustable
graphics setting with three presets: **Low** for a Raspberry Pi 5 at 30 to 60 fps, **Medium** for
an Intel Core Ultra 5 laptop at 30 fps, **High** a flat 60 fps in play.

## What blocked part of it

The game's private asset root, `/windows/shop/panzerrechnersystem` (`KESSEL_ASSET_ROOT` in
`tools/launch_demo_20261101.sh`), is no longer on this machine: `/windows/shop` was last changed on
12 November and every drive (3.6 million files) was searched. Without it neither the launcher nor a
headless harness can build the Canyon v2 run or the Bridge, so **no mission was played or
smoke-tested** and the gameplay bug test of the menu modes did not run. Everything below was
measured on a partial asset root: the 3 November Entfernungsmesser package (`dist/`, the Tiger,
crew, sounds, terrain kit and the original Canyon run, `gaea-canyon-run`) plus the repository's
tracked files.

## Quality presets (this change)

| Preset | Grid | Sun shadows | Light sampling | Raymarch budgets | Optic relays a frame | Sized for |
|---|---|---|---|---|---|---|
| Low | 320x180 (6x on 1080p, 16x on 5K) | 512 | gameplay | 32 froxel slices, 512/512 cascades, 64 px lamp cube, 256 sky cache, 1 sample a pixel | largest + 1 rotating, culled | Raspberry Pi 5 class |
| Medium | 640x360 (3x on 1080p, 8x on 5K) | 1024 | gameplay | the old Low (laptop) table | largest + 1 rotating, culled | integrated laptop graphics |
| High | 1024x576 (5x on 5K) | 2048 | full | unchanged | all | desktop GPU |

Every preset renders the same scene and runs the same simulation (CLAUDE.md, second principle,
rule 2); each keeps a shadowed lamp. Ultra stays as an explicit review setting.

- Graphics page, Display tab, first row: **Quality preset**; it sets grid, shadows and sampling
  together, the rows below still fine-tune. Saved with the preferences (`"preset"` in
  `graphics.v1.json`; older files read as High).
- Title menu: the row reads **Graphics: High**; left/right steps the preset, Enter opens the page.
  F11 still cycles it.
- Default: discrete GPU High, integrated GPU Medium, a single-board GPU (V3D/VideoCore, Mali,
  Adreno, PowerVR) or software adapter Low.
- Launcher: `--quality low|medium|high` (persists); without it the saved choice is kept. `--res`
  still forces a grid. A preferences file from before the presets migrates to High at 1024x576,
  which is what the launcher forced on every start until now.
- Low and Medium cull optic captures per instance and per terrain page (exact: only what lies
  wholly outside the capture frustum is skipped). On the 4090 at Low this took the gunner's optic
  pass from 3.2 to 2.2 ms of GPU but cost about 0.7 ms of CPU, so High keeps the old draw.

## Fixes to the measuring tools (this change)

- `KESSEL_GPU_PROFILE=1` hung forever after load (noted 9 November). Cause: Vulkan's query copy
  waits until every query in its range is written, and a labelled pass that never ran left one
  unwritten. Every slot is now written once before each frame (`GpuProfile::prime`); an unrun
  pass reads about 0 ms. Per-pass GPU timings work again, with and without the live frame.
- `gfxbench` ignored `KESSEL_GRAPHICS_PRESET` and always ran High (noted 10 November). It now
  honours it (the argument, else the variable, else the adapter).
- `KESSEL_BENCH_TRAVERSE=SECONDS`: the traverse key held, reversing every SECONDS.

## Measurements (RTX 4090, i9-12900K, original Canyon run, live frame: simulation plus render)

Frame time in ms, mean over 240 frames (p99 within 2 to 4 ms of it), standing at the start, no
combat:

| Seat | High 1024x576 | Medium 640x360 | Low 320x180 |
|---|---|---|---|
| Driver | 17.2 | 12.3 | 10.9 |
| Gunner | 28.6 | 19.7 | 17.3 |
| Commander | 30.7 | 21.3 | 18.8 |
| Commander, head out | 30.8 | 21.5 | 18.9 |
| Loader | 24.7 | 18.4 | 16.4 |

Simulation is 2.6 to 3.3 ms of each. Before the presets existed the same seats on the old High
budgets at 320x180 were 1.2 to 3.6 ms slower than the Low column.

GPU passes, gunner at High (sum about 20 ms): raymarch G-buffer 4.6 (two views), optic relay
captures 3.5 (four views), shade 2.2, sky 1.7, environment probe 1.3, volume 1.3, sky-visibility
dynamic 1.0, exposure 0.9, near shadow 0.9. Commander at Low (sum 8.3 ms): G-buffer 1.9, optic
captures 1.3, sky-visibility dynamic 1.0, environment probe 0.5, shadows 0.9, sky cover 0.4.

The owner's own sessions (session records in `artifacts/runtime/`):

| Run | Grid | Median fps | Seconds under 55 fps | Gunner seat | Notes |
|---|---|---|---|---|---|
| 9 Nov 20:03 | 800x450 and 1024x576 | 36.5 | 83 % | 31.6 fps, render submit 20.9 ms | 13 % of seconds under 20 fps |
| 9 Nov 21:27 | 1024x576 | 31.3 | 98 % | 29.5 fps, render submit 20.2 ms | 17 % under 20 fps |
| 10 Nov 01:47 | 640x360 | 56.4 | 47 % | 39.9 fps, render submit 20.3 ms | no combat; simulation 5 ms |

## Findings

1. **High is not a flat 60 on this machine.** Only the driver's seat is near 16.7 ms; the turret
   seats are 25 to 31 ms with nothing happening, and the owner's runs show the gunner's seat at 30
   to 40 fps in play. Lowering the grid does not fix it: most of the cost does not scale with
   pixels.
2. **The fixed cost is geometry and per-frame passes, not resolution.** From 1024x576 to 320x180
   (a tenth of the pixels) the gunner's frame only drops from 28.6 to 17.3 ms. The G-buffer pass
   is 1.9 ms at 320x180, which is triangles: the playable Tiger is 4.13 M triangles. The four optic
   captures of the gunner's seat submitted 16 M scene and 8 M terrain triangles a frame, some into
   a 32x18 image; culling cuts that to 3 M (now on for Low and Medium).
3. **The turret seats render the world twice** (the cabin view and the sight or port view); every
   raymarch pass shows two passes a frame there. That is what the player sees, not waste, but it
   doubles the fixed cost.
4. **CPU render preparation is single-threaded and serial with the simulation**: at Low the GPU
   work is about 8 ms but the frame 19 ms (simulation 2.8, encoding and submit 2.7, the rest
   uniform uploads and culling). In the owner's live gunner sessions "render submit" was 20 ms,
   with 190,000 sky-visibility static redraws a second against 2,700 at the loader's seat. This
   bench did not reproduce those redraws standing or traversing; their trigger in play (driving,
   the day clock or the Canyon v2 scene) is open and is the first thing to chase with the assets
   back.
5. **Damage solves**: the layered armour lattice costs 172 ms median, 454 ms p95, 507 ms worst of
   CPU per hit (`fea-bench 5`). It runs on worker threads with deferred commit in live play;
   fragment tracing on a perforation was 110 to 133 ms on the main thread on 9 November and was not
   re-measured (needs a live combat run).

## What each target needs

- **High, a flat 60 in play on this 4090**: not met (turret seats 25 to 31 ms standing). Needs a
  reduced-triangle Tiger for the shadow, point-light and optic passes (generated from the source
  with a measured error, CLAUDE.md rule 3), the sky-visibility redraw found and stopped, render
  preparation moved off the simulation thread, and a cheaper second view in the turret seats.
- **Medium, 30 fps on an Intel Core Ultra 5**: not measured on that hardware and not likely met
  as is. Arc graphics in that chip is roughly 8 to 18 times slower than a 4090 by bandwidth and
  throughput; the 3 November package measured an Intel UHD 770 at 6 to 7 fps looking at the tank on
  the old Low budgets at 480x270. A Core Ultra 5 is several times a UHD 770, so expect the driver's
  seat near 30 and the turret seats well under it until the fixed costs above come down.
- **Low, 30 to 60 fps on a Raspberry Pi 5**: out of reach for this renderer. Its CPU cores are about
  three times slower than this machine's, so simulation and render preparation alone would be
  near 30 ms, and its GPU is two to three orders of magnitude below a 4090 against 8 ms of 4090 GPU
  work at Low. A Pi needs a separate raster path and low-poly assets. The Pi build itself
  (aarch64 Linux, Vulkan through Mesa's v3dv) was not attempted.

## Not done

- The gameplay bug test of Bridge and Canyon Run with the living crew and solo: blocked on the
  asset root. With it back: `playtest-smoke` and `canyon-ai-run` (both menu courses, crewed and
  `KESSEL_ONE_MAN=1`), then the launcher.
- The unit tests: 2,763 pass; 27 fail, every one on a missing or older asset file
  (`act1/heights.npy`, the PSX kits, `kv1.glb`, canyon rocks, review fixtures). With
  `KESSEL_ASSET_ROOT` pointing nowhere, 124 fail the same way.
