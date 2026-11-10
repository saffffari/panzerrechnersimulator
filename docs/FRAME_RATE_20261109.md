# Frame rate in play: where the time goes (9 November 2026)

Source: the owner's 25-minute Canyon run, `artifacts/runtime/demo-20261109-212702.session.jsonl`
(1,467 one-second lines). Baseline: presented frame mean 37.3 ms (27 fps), 640 seconds over 33 ms;
simulation 18.4 ms and render submission 16.9 ms a frame. GPU not sampled in that run.

## The frame is serial

`main.rs` redraw: `acquire()` the swapchain image, then `Game::update_realtime` (one update for
frames under 50 ms; more, each at most 50 ms, for slower frames), then the render encode and
`queue.submit`. Simulation and submission run one after the other on the same thread, and the
frame is their sum (18.4 + 16.9 + about 2 ms of other work = 37.3). `encode_ms` starts after the
acquire, so it is CPU encoding, not waiting for the GPU.

Estimate for overlapping them (simulation of frame N+1 on a worker while frame N is encoded): the
frame becomes the longer of the two, about 18-20 ms in this run (50-55 fps), not 60. `Game` and
`Renderer` are one mutable world today (the update writes the renderer's slots and instance pools
directly), so this is a large restructure; both halves also have to come down on their own.

## By station (seconds in each, means)

| Station | Seconds | Frame | Simulation | Submission | Uniform uploads per frame |
|---|---|---|---|---|---|
| Gunner, eyepiece composite | 926 | 38.6 | 16.0 | 20.2 | 2.0 (3.8 ms each) |
| Commander, inside | 191 | 27.2 | 13.9 | 11.3 | 2.5 |
| Commander, head out | 119 | 55.4 | **41.7** | 12.9 | 1.0 |
| Loader | 99 | 39.2 | 25.7 | 12.0 | 1.1 |
| Driver | 128 | 25.0 | 15.2 | 9.1 | 1.0 |

- **The gunner's eyepiece renders the scene twice a frame** (the optic pass, then the cabin round
  the eyepiece): two `upload_uniforms` (3.8 ms each) and two encodes. That is most of the 20 ms
  submission at the gun, where the owner spends 63 % of the run.
- **Head out, simulation is 41.7 ms**, of which `world_foley.traces` is 10.2 ms an update on
  average and up to 36 ms: the acoustic ray traces. The ray loop is time-boxed (1.5 ms), but a
  finished trace synthesised its binaural response (8 bands x 2 ears of filtered noise over up to
  a second of samples) on the simulation thread, and outdoors the field's key changes often.

## The one-second stalls (170-240 ms of simulation)

At t = 1336-1338, 1670-1685, 1920, 1998 s the worst single updates were `world_foley.traces`
(18-36 ms), `update_fx` / `drive_interior_fire` / `fire_world_sources` (9-16 ms: burning wrecks'
surface fires), `wrecks` (9-11 ms), `drain_audio` (18-32 ms) and `trees_scatter_push` (9-28 ms),
several in the same update. These seconds held 4-5 frames.

## Changes in this commit

1. **Traced acoustic responses are synthesised on worker threads in live play** (at most four at
   once; harness runs keep the synchronous path). The response is the same computation; it is
   installed a few milliseconds later, the nearest cached field standing in meanwhile as before.
   Expected to remove most of the head-out 10 ms and the 18-36 ms stalls; **not yet measured**: the
   owner's live run held the game for the whole of this session's window, so no measured run
   could be made.
2. **Finer timers** (sim_phase probes, in the session log's `calls`; always on, as the existing
   probes are): `render_track_stamps`, `render_rt_update`, `render_accum_prepare`,
   `render_internal_encode`, `render_blit`, `render_queue_submit`, `render_sun_shadows_encode`,
   `render_cabin_sky_shadows_encode`, `render_raster_optics_encode`; `acoustic_field_for_ear`,
   `acoustic_scene_build`, `acoustic_trace_rays`. The next logged run splits submission and the
   acoustic segment by these.

## Next, in order of size (named, not yet done)

| Item | Cost in the baseline | Why it is next |
|---|---|---|
| Second full render for the gunner's eyepiece | about 8-10 ms of submission at the gun | Share the per-frame uploads (scene visibility, RT actor lookup, surface pack) between the two passes of one frame; measure with the new timers first. |
| `upload_uniforms` 3.4-4.2 ms a call | 3.4-8 ms a frame | Its sub-probes (`upload_scene_visibility`, `upload_rt_actor_lookup`, `upload_rt_surface_pack`) were not in the build the owner ran; the next log names the part. |
| Burning wrecks' surface fire presentation | 9-16 ms in the stalls, about 2 ms mean | Same smoke clearance as the cabin fire; the parallel clearance (commit 6435376) covers the cabin; wrecks use `fire_wreck_patches` (5 ms worst). |
| `trees_scatter_push` | 1.9 ms mean, 28 ms worst | Tree and scatter contact against every tank each update. |
| `drive_casings` | 1.1 ms mean | Spent brass. |
| Serial simulation and submission | the sum instead of the larger | The restructure above, after the halves come down. |
