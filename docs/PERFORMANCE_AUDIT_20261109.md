# Performance audit, part 1: the recorded frame (9 November 2026)

Status: **read from a recorded playtest and from the source; nothing was benchmarked today**
(the game was open for the owner's run-through, so no bench may run). Part 2 is the measured
split with the game closed. Target: locked 60 fps at 1024x576, 99th-percentile frame under 15 ms.

## Evidence

The owner's Canyon run of 7 November (`artifacts/runtime/demo-20261107-230552.session.jsonl`,
752 s, 20,961 frames, 1024x576, locked look, that day's build). Read with `tools/sim_calls.py`
and the per-second frame records.

| Per frame | Typical second | Bad second (1 in 10) | Worst |
|---|---|---|---|
| Whole frame | 36.2 ms | 44.4 ms | 492 ms |
| Simulation (CPU) | 9.8 ms | 21.0 ms | 451 ms |
| Render calls (CPU) | 16.1 ms | 19.3 ms | 43.7 ms |
| Rest (GPU wait and present, not split) | 10.2 ms | 14.5 ms | |

The run averaged 28 fps; 9,818 frames (47 %) took longer than 33.4 ms. In the three minutes of
fighting (300 to 480 s) simulation alone averaged 38 to 53 ms a frame.

What one frame submits, typical second:

| Work | Count per frame |
|---|---|
| Shadow depth passes | 29 |
| Shadow draw calls | about 7,500 (4,470 instanced, 3,100 mesh) |
| Camera scene renders (main view plus port and sight relays) | 4.2 |
| Camera draw calls | about 1,960 |
| Full-screen passes | 38 |
| Bloom passes | 24 |
| Live instances, in batches | 37,000 in 4,924 |

## What that says

1. **The frame is three serial pieces and each one alone is over or near the budget.**
   Simulation, then render-call encoding, then the GPU, all on the main thread in sequence:
   10 + 16 + 10 ms. No single fix reaches 16.7 ms.
2. **The largest steady cost is CPU draw submission, not pixels and not triangles.** About
   9,500 draw calls a frame, each with its own push constants and buffer binds
   (`renderer_raymarch.rs` `draw_scene`). Three quarters of them are shadow draws.
3. **Shadow views redraw the scene every frame.** The main view draws the near cascade, and
   every frame all six cube faces of each of the two shadowed point lights (12 passes,
   `encode`, `rm-point-face`), plus the far cascade and sky cover when not reused. The cabin
   lamps already keep a static depth cache with a moving-parts overlay (`cabin_shadows.rs`);
   the raymarch point faces do not.
4. **The Tiger is drawn part by part.** 1,518 parts, 4.13 M triangles: about 3,100 shadow mesh
   draws a frame are its parts going into the depth passes. The gen2 export has fewer triangles
   (1.79 M) but **more meshes (2,489)**, so brought in as it is, it would lower the GPU cost and
   raise the CPU cost that is the present limit. It needs merging by material and rigid group
   before it is a fair test.
5. **Simulation has a base of about 10 ms and long single calls.** Worst calls in the run:
   `resolve_authoritative_vehicle_impact` 238 ms, `update_wrecks` 75 ms, `ai_tank_update`
   41 ms, `render_upload_uniforms` 85 ms (mean 2.7 ms, every frame). Steady phases per update:
   effects 2.4 ms, wrecks/fire/haze 1.8 ms (68 ms worst), infantry 1.2 ms, enemy AI 0.9 ms.
6. **The GPU share is not known.** Live play records no GPU pass timings (`"gpu": null`), so
   the 10 ms "rest" is unsplit. The engine document's figure of about 7 ms for the G-buffer is
   from an earlier bench.

## What this means for voxel buildings

The worry is right that today's frame has no room. It is not right that voxels are what fills
it: the limit is draw-call count and one thread, and a voxel building meshed in chunks adds few
draws. The chunk store on `codex/engine-overhaul-20261107` (`src/voxel_chunks.rs`, not in this
build) republished two edited chunks in 0.2 to 0.5 ms across four scenes of 682,407 cells in
total, on the CPU, without rendering. Unknown until measured: the cost of rebuilding the ray
acceleration structure for an edited chunk, and collision against chunks for a tank driving
through.

## Proposed order of work

1. **Measure** (game closed): `gfxbench` with GPU pass timings on Canyon and the bridge map,
   standing, moving and in a fight; split the 16 ms of render calls by pass and the 10 ms rest
   by GPU pass.
2. **Shadows:** static-depth cache plus moving overlay for the point-light faces, per-face
   culling, slower cadence for faces whose light and casters did not move. Cadence is a preset
   quantity under the physical-truth rule; the emitters and occluders stay the same.
3. **Batching:** the Tiger's rigid parts merged per material and frame into few buffers; one
   draw per group instead of one per part. The same merge is the gen2 import's first step.
4. **Simulation off the frame's critical path:** the long single calls (impact resolution,
   wreck rest tests) on workers with published results; then encoding in parallel with the
   next update.
5. **Relays:** port and sight views refreshed only when looked at or stale.

Each step gets a before and after bench on the same fixture; anything that could change the
picture gets matched stills.

## Destruction tests, after step 1

The ladder already written in `ENGINE_OVERHAUL_20261107.md`: idle Tiger, one strike, repeated
strikes, then one voxel building under local destruction. For the building:

- one house as a voxel volume (materials: timber, brick, plaster, glass), chunked 16³;
- armour-piercing shot through both walls (hole, shot carries on), high-explosive shot
  (crater, blast removal), the Tiger driven through a wall and into the house;
- pieces cut loose from the ground fall as bodies; rubble that stops or bogs the tank;
- recorded per edit: remesh, upload, acceleration rebuild, collision update, frame time.

Needed first: the chunk store merged into this build, and a loader for the gen2 Tiger (the game
reads none of it yet; the import plan's Garage step).

## Addendum: the owner's run of 9 November (`demo-20261109-141955`)

Read from the run's log and session record after the game was closed. 160 s, 7 fps mean.

- Before contact: frame 22 to 34 ms, simulation 4 to 6 ms, render calls 12 to 14 ms.
- A 76 mm round through the Tiger's hull side at 3 m: `resolve_player_penetration` 3.1 s in one
  call and `commit_fea_penetrations` 3.5 s (the update waits for the plate and round solves),
  then `drive_penetration_fx` 1.3 s in one update.
- After the Tiger's ammunition went: simulation about 35 ms an update (effects 22 ms, fire
  14 ms) at five catch-up updates a frame, and render calls 225 to 257 ms a frame. Five crew
  bodies had been torn into about 21,000 pieces; the log notes more than 8,192 cubes of one
  colour in view.
- The thrown-turret flight cost (fixed in `wreck_turret.rs`, unbenchmarked) is one part of
  this, not the largest in this run.
- **No death was declared.** The log has "Tiger destroyed" and no "killed:" line, so the death
  shots never ran and the sight view rode the thrown turret (`Tank::turret_model` returns the
  thrown pose). On 2 November the same event was followed by "killed: AmmoDetonation". Cause
  not found yet.
- Enemy kills across the recorded runs of 2 to 9 November: 13 penetrations, 8 turret throws,
  3 abandoned, 1 burning wreck. Each throw is the whole load of 77 rounds at 53 m/s.

## Part 2, first measurement: one catastrophic kill on Canyon (9 November, game closed)

`gfxbench high 1100` with `KESSEL_BENCH_LIVE_FRAME=1 KESSEL_BENCH_KILL=60`, the launcher's
environment, 1024x576, locked look; the killed T-34 is 956 m from the Tiger. The bench steps
at a fixed 1/60 s, so the live game's catch-up multiplier is not in these numbers.
`KESSEL_GPU_PROFILE=1` hangs this fixture after loading (main thread polling), so the GPU
frame is still not split by pass.

| Mean per frame | Before the kill | After, without the turret fix | After, with it |
|---|---|---|---|
| Simulation | 23.1 ms | 44.3 ms | 35.8 ms |
| GPU frame | 15.9 ms | 26.7 ms | 26.6 ms |
| Whole frame | 39.0 ms | 71.1 ms | 62.4 ms |

Simulation after the kill, ms per update, with the fix:

| Cost | ms |
|---|---|
| Infantry (`mixed_observe_act` 17.9) | 20.3 |
| Effects (`update_fx`) | 8.9 |
| Interior fire (`drive_interior_fire`, smoke clearance 3.4 of it) | 8.7 |
| `render_upload_uniforms` | 7.6 |
| `render_collect_parcels` | 5.3 |
| Wrecks (the thrown turret; 11.6 without the fix) | 3.2 |

The thrown turret settled at the same time and place in both runs (15.1 s, 183.37 m). The
infantry cost is there before any shot is fired and was not in the 7 November record (1.2 ms):
it arrived with the swarm of 9 November. After one kill the scene holds 262,140 embers and
72,427 merged grains, and the GPU frame rises by 11 ms.

## Infantry: 20.3 ms to 2.7 ms an update (9 November, same kill bench)

Cause, found with per-call probes: each soldier's observation casts eight 6 m obstacle
feelers and one sight ray, and `PlayerSightCover::first_hit` swept every one of them against
the Tiger's source surfaces (3.2 M triangles in the hull frame alone, 80 posed frames).
With 125 men round the tank that was 1,098 sweeps an update at 20 microseconds each, 21.6 ms;
the world's own cover index cost 0.16 ms for the same rays.

Changes (`infantry_combat.rs`, `small_arms_runtime.rs`, `mixed_combat_runtime.rs`,
`surface_fire.rs`):

- The feelers read the occupancy that stops a soldier's steps (the world and the vehicles'
  boxes) through a new `CoverSet::first_obstacle`; sight still sweeps the source surfaces.
  **This is a named simplification**: near the Tiger a soldier's obstacle sense now sees the
  hull's box, as his movement always did, not the exact plates.
- The 80 frame inverses are taken once an update, not once a ray (`sweep_posed`), and a ray
  whose box misses the Tiger's box is not swept (`world_bounds`). Exact.

| Mean per frame, same bench | Before | After |
|---|---|---|
| Infantry phase | 20.3 ms | 2.7 ms |
| Simulation before the kill | 24.0 ms | 6.6 ms |
| Simulation after the kill | 35.8 ms | 18.9 ms |
| Whole frame after the kill | 62.4 ms | 46.2 ms |

Unit tests for infantry, swarm, small arms, mixed combat, foot AI and source surfaces pass
(161 tests). Not yet seen in live play. What is left after the kill is effects and fire
(9.4 ms), `render_upload_uniforms` 7.9 ms, `render_collect_parcels` 5.5 ms, wrecks 3.3 ms, and
the GPU frame at 27 ms; the effects are with the `fx-overhaul` session
(`/home/alex/panzer/fx-overhaul-20261109`, branch `claude/fx-overhaul-20261109`).

## Riflemen fire on the Tiger; the bench fixture's pile (9 November, evening)

**The kill bench above never spread the infantry.** Without `KESSEL_PLAYTEST` the bench skips
the playtest setup and all 156 riflemen stand on one point beside the Tiger (56 shots in one
run left from the same muzzle position). The infantry figures above (20.3 to 2.7 ms) are that
pile's; the cause and the fix hold wherever men are inside the Tiger's box, but the size of the
gain in play is not measured. `KESSEL_BENCH_RING=NEAR_M,FAR_M` now spreads the men over a ring
round the Tiger. With `KESSEL_PLAYTEST=1` the men are at their course posts and none is in
range of the spawn.

Owner's direction: "infantry should pelt tank with small arms fire when they can". Before, a
rifleman sensed the Tiger only inside the 45 m at which he senses a man, and fired only when
his policy chose to and his squad was not moving. Now (`infantry_combat.rs`,
`mixed_combat_runtime.rs`, `swarm_runtime.rs`):

- a vehicle's point is sensed to the rifle's modelled reach, 120 m (`VEHICLE_SIGHT_RANGE`,
  authored);
- a Soviet rifleman who sees the Tiger and nothing nearer aims and fires whenever the rifle is
  ready, halting 0.3 s for the shot, unless he is bounding, boarding, throwing or routed, or a
  comrade stands in his line of fire (`comrade_in_line_of_fire`);
- the world sight line to the Tiger is sampled with the aim point every 0.2 s for the swarm,
  not on every step (**named simplification**).

Ring bench, 156 men at 25 to 110 m, 18 s, same binary settings:

| | Before | After |
|---|---|---|
| Rifle shots | 39 | 207 |
| Strikes on the Tiger's armour | 27 | 214 |
| Infantry phase, ms per update | 5.8 | 6.2 (11.3 before the sight line was cached) |
| Riflemen killed by small arms | 17 | 19 |

Open: the 17 to 19 riflemen killed by small arms in both runs (their own side's bullets or
ricochets; not traced). 207 shots from 156 men in 18 s is about one man in seven firing at
the bolt's rate; what holds the others (sight line, facing, intent) is not broken down. GPU
times of these runs are not usable: the effects session was rendering on the same card.

### T-34 coax on the mounted Tiger (same evening)

T-34 crews fired their machine guns only at men on foot. Now (`Game::enemy_coax_on_tiger`,
`game.rs`) a turreted tank that is tracking the mounted Tiger with its gun laid fires coax
bursts along the gun while the main gun is being loaded: 8 rounds every 3 s inside 400 m with
a clear line (burst, period and range authored; the DT's ammunition is not counted). Headless
Canyon run (`canyon-ai-run`, seed 211, 180 s): 120 coax rounds fired, 133 contacts on the
Tiger's armour. The run's outcome differs from the 2 November receipt for the same seed
(Tiger knocked out with one kill, against alive with two); the extra projectiles shift the
seeds of later shots, so that is not evidence either way.
