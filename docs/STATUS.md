# Kessel — current status (2026-08-31)

Kessel is a playable three-act prototype developed from one `main` checkout on Apple Silicon macOS.
The Mac is the development machine and source of truth; Hal/Linux is the Vulkan validation and
batch-simulation machine. The Mac build uses forward raster lighting and does not create or package
the Vulkan ray-query resources. The former heightfield/OBB and screen-depth contact-light proxy is
disabled in shipping paths; Laptop therefore degrades through samples, resolution, and raster
shadowing instead of substituting fake geometry.

## 2026-08-31 Vulkan release reconciliation

- The current Mac renderer keeps its newer physical work: visible-normal GGX sampling, weighted
  local-emitter selection, the bounded delta-Eddington endpoint closure, current per-act
  atmosphere transport, and the always-created empty visibility bind used by portable smoke.
- The Hal-tested compiler-shape containment is restored around those mechanisms. Mutually
  exclusive environment/miss-hit branches feed one inlined physical-transport call; exact glass
  reflection/transmission and the cosine/VNDF one-bounce pair each iterate one inlined continuation
  using host-written invariant counts (`cam.transport_passes = 2/2`). This changes shader compiler
  shape, not rays, estimator weights, materials, or transport laws.
- The portable smoke draw always binds group 1 even when it is the empty non-ray-query layout. The
  Linux launcher is restored, and tests that referenced retired penetration/spall code are gated
  with the retired implementation instead of breaking the current test build.
- The preceding containment pattern compiled and rendered on Hal/RTX 4090 with driver 610.57.04;
  this reconciled source candidate still requires the mandatory fresh Hal High-frame gate because
  it also contains the newer VNDF, emitter-sampling, and atmosphere changes.

## 2026-08-29 integration receipt

- `campaign-ai-run` now drives a real Act-I attempt through generated navigation, live combat,
  timed hatch/climb movement, three on-foot folio searches, remounting, and the generated inset
  exit. Seed `0xc0de194245a10001` completed authoritatively in 674.78 simulated seconds with three
  dismounts, 14 vehicle kills, 4,501.5 m of tank travel, and 144.7 m on foot. JSONL retains both
  successful and failed attempts for regression and balance analysis. Schema v2 separates
  outgoing shots/hits/penetrations/kills from incoming hits/penetrations, records exact fuel and
  spare-part deltas, keeps crouched on-foot trajectories, and labels real rifle/coax-MG events.
  The validated run closes its ammunition equation exactly: 50 initial rounds − 15 shots = 35.
- Physical fuel cans use the reviewed jerry-can GLB. E / controller Cross picks up or places with
  zero impulse; at the Tiger's rear it transfers only the litres the tank can accept and preserves
  any remainder. Q / Circle throws the held can, with the empty steel can travelling substantially
  farther than a full 20 L can. Empty cans persist in the world.
- Laptop and 4090 are explicit session-local graphics presets. Laptop uses 320×180, a 256-sample
  terrain budget, 512 px shadows and forward lighting. 4090 uses 640×360, a 512-sample terrain
  budget, 2048 px raster shadows for forward-only passes, and deferred hardware ray-query direct
  lighting. Screen-space AO, screen-space diffuse bounce, ambient floors, aperture proxy lights,
  analytic viewport cones and camera-dependent interior fill are not shipping lighting paths.
  Four-player split screen always selects the 4090 preset; occupied visible station viewports share
  one shadow result instead of each submitting a full shadow scene.
- Act I's blizzard is brighter and snow-white but denser in depth: six world-space snow shells,
  spindrift, shelter masking and a stronger atmospheric veil trade empty clarity for readable
  whiteout. The procedural destruction test wall is no longer spawned in normal play.
- Enemy tanks use parts-derived yawed hull footprints for separation, collision response and
  predictive avoidance, including enemy↔enemy and Tiger↔enemy contacts. This prevents T-34s from
  driving through or stacking on one another.
- Hal's isolated RTX 4090 batch completed ten frozen Act-I seeds: 3 successes, 3 timeouts, 3 Tiger
  KIAs and 1 on-foot KIA. All JSONL v2 records pass combat, resource, map and trajectory invariants.
  The batch captured 734 on-foot samples and 16 spatially labelled tank-coax-MG attacks, while also
  reproducing two early route deadlocks and one late recovery oscillation. Successful runs consumed
  11–16 rounds and 57–76 L of fuel; none acquired optional loot or used spare parts, so the batch
  supports consumption bounds but deliberately does not claim a cache-placement recommendation.
- The reproduced route failures now have deterministic regressions. Objective-relative progress,
  neutral pivoting and alternating widening escape routes clear the former seed-4/6 town deadlock;
  seed 9 now completes all three maps, five waves and the exit in 582.97 s. Seed 6 reaches all three
  maps before exposing a separate combat-aim/ground-shot failure, and physically immobilized Tigers
  now terminate after a bounded recovery budget instead of consuming the run cap as a route timeout.
- Hal is reachable over OpenSSH and the RTX 4090 currently has about 23 GB free. The Vulkan feature
  probe validates hardware ray queries and vertex-return support; a full High scene render and
  four-view benchmark remain the integration gates for the corrected renderer.
- Current source receipt: **432 passed, 0 failed, 1 host-audio test ignored** in the full game test
  binary, plus clean formatting, release compilation and diff checks.

## Playable now

- Acts 1–3 carry one Tiger, its damage, stores, discoveries, and route state through the campaign.
- Solo, split-screen seats, and host-authoritative LAN crew play use the same embodied commander,
  driver, gunner, loader, and radio/bow-MG roles.
- The interior, cupola/hatches, optics, manual loading, tank movement, on-foot scouting and
  scavenging, paper map, stealth, armor/spall/module damage, fire, repair, crush, shell destruction,
  T-34 AI and encounter/platoon logic are working systems.
- Armor strikes now leave compact persistent local dents/holes, weaken repeat-hit resistance, drive
  directional acoustics/concussion/spall and throw the camera away from the shell source. Field
  repair consumes a spare and clamps three visible track shoes across the breach. The turret-floor
  disc is a real articulated PBR mesh parented to live traverse.
- Two tested but not-yet-wired foundations now define deterministic six/ten-tank doctrine roles and
  invariant round/case ownership. They are architecture for the next integration slices, not live
  campaign smoke tactics, ten-tank battles, hangfires or reload replication.
- The authored campaign worlds and encounter placement remain thin. Kessel has the mechanical loop,
  but not yet the dense, dangerous, explorable warzone it is meant to become.

## Mac renderer and atmosphere

- Winter has a dense, depth-aware, crisp-pixel blizzard with world-fixed flakes, gusts, spindrift,
  interior shelter masking, and usable visibility through open hatches and optics.
- Every act has deterministic dawn/day/dusk/night profiles. Overcast, haze, smoke and restrained
  visibility replace the old clear-sky, glorious-weather look.
- Internal-resolution presets (640×360 through 1280×720) and independent clean/light/standard/strong
  retro-filter presets make honest A/B captures possible. Current image comparisons favor 800×450
  visually, but 640×360 remains default until a self-describing Mac frame-time baseline and target
  are recorded.
- Game and editor share terrain and manifests. Placements compile into 128 m pages with shared GPU
  prototypes and packed instance buffers.
- A renderer-owned spatial-cell index culls page work for local camera and shadow queries instead of
  scanning the whole world. The current maps contain 12,438 / 20,474 / 36,438 placements, although
  Act I is still a small city patch in a large field and Acts II–III are still tree-heavy.

## Playable building and world foundation

- A live two-storey Pizza building is enterable on foot and contains four rooms, real doorways,
  windows and stairs, interior lights, weather shelter, shelves and loot.
- Shell and tank-crush damage can remove supported structure and produce grounded rubble. Rendering,
  collision, cover/line-of-sight, lighting and shelter respond to the surviving structure.
- The modular compiler/runtime has stable instance IDs, floor/room/portal data, support topology,
  compact damage and loot state, plus incremental render/nav/collision dirty sets.
- The one featured current-act building is keyed by act plus stable instance/layout identity in the
  in-memory campaign checkpoint. Load/retry restores it, successful completion commits it, and a
  blocked/lost completion cannot overwrite it. This is not a disk-save system. Mutable state is
  exact module integrity, 1/256-quantized sub-point damage, and atomic shelf/container state. All
  current doorways are permanently open; door-leaf gameplay and partial per-item loot are not live.
- A compact bounded copy of that state is host-authoritative over LAN and is included for late
  joiners. Gameplay protocol v3 uses a host epoch and fresh client challenge to reject stale and
  duplicate packets without reopening old sequence spaces across wrap or host restart; discovery
  remains format/version v1.
- This is one proved vertical slice, not a finished city system. Most landmarks remain sealed or
  placeholder geometry, and the editor still needs first-class floorplan, room, stair, door, loot
  and structural authoring.

## Runtime assets and Mac package

- The current reviewed model closure is exactly **139 GLBs / 233,434,384 bytes**: **120 Pizza**
  models / 79,558,992 bytes, **16 current Tiger/T-34** models / 152,018,608 bytes, one authored P08
  bake / 1,125,872 bytes, and two verified CC0 static crew poses / 730,912 bytes.
- With the 21-file non-GLB payload, the exact locked resource input is **633,083,808 bytes**. Raw
  authoring libraries, source archives, screenshots, video and retired models are not bundled.
- The last fully packaged owner-development checkpoint is `dist/macos-f8496b9-final/`: its app occupies
  **627,716 KiB (about 613 MiB)** and its compressed DMG is **534,839,582 bytes (about 510 MiB)**.
  The DMG SHA-256 is
  `2d57a97bf0e686e2868b0bc146f5efd2bced11992bc5f7904cb13257911d3d72`.
- The ignored artifact presently in `dist/macos/` is an internally valid but stale owner/developer
  snapshot. It embeds the older **147-GLB / 491,705,164-byte** model closure and must not be presented
  as a build of current `main`.
- Current Mac source receipt: **261 passed, 0 failed, 1 real-audio-device test ignored** under the
  full locked suite; the locked release build also passes. The packaged app and DMG independently
  pass resource hashes, ad-hoc hardened-runtime signatures, architecture/minimum-OS checks and disk
  image verification. With repository asset overrides removed and its working directory outside
  the repository, the bundled executable loaded Garage and all three acts from `Contents/Resources`
  and rendered one Garage frame plus 12 Metal dawn/day/dusk/night frames. The atmosphere grid is
  exact 2x-nearest from 640x360; the Garage frame is exact 3x-nearest.
  That package predates the resolution/retro controls, expanded impact response, armor wounds/track
  repairs and articulated turret floor; a fresh package must be cut before those are called shipped.

Friend/public distribution is still blocked by the 16 tank models' unrecorded provenance, unverified
Mapzen-derived Garage/Act I redistribution rights, review and bundling of applicable Rust dependency
licences, and a re-check of Pizza Doggy terms and purchase evidence. A standard friend build also
still needs Developer ID signing/notarization, local-network permission/firewall and cross-machine
session QA, controller and 2/3/5-crew testing, and a clean-account downloaded-DMG launch test.

## Immediate production order

1. **Record the baseline.** Capture a self-describing Mac frame-time receipt—commit, hardware, build,
   scene, warmup/timed frames and avg/p50/p99—before performance or resolution claims.
2. **Integrate reload truth.** Replace the fragmented live reload/round/case state with the tested
   invariant aggregate and prove conservation through interruption, retry and late join.
3. **Persist resumable work.** Version LAN resumable-task state and add an atomic versioned disk
   run-save before multiplying mutable props.
4. **Scale the vertical slice.** Turn the proved building into a dense 500×500 m warzone block with
   varied multi-floor interiors, destruction, scavenging, debris, cover, forest and streets. Keep
   compact building state bounded and authoritative as instance count grows.
5. **Worldbuilder and terrain.** Add floorplan/room/door/stair/loot tools, structural tagging and collapse
   preview, density brushes, spline roads/trenches/patrols, semantic zones, encounter sockets and
   validation overlays. Replace the coarse ground treatment with chunked/LOD terrain, layered tiled
   materials, vegetation LOD and interest-managed collision/destruction/room streaming.
6. **Finish the gun and signature slice.** Complete main-gun firing/reload sensory work and the shared
   engine/track signature model before broadening the component simulation.
7. **Doctrine, co-op and release.** Wire six-tank fix/mask/flank doctrine into real perception,
   movement and smoke in the dense route; keep ten-tank drills blocked on the hard six-enemy pool.
   Stress mutable-task synchronization and late joins at world scale; exercise 2/3/5-seat LAN and
   split screen; then clear distribution gates, rebuild,
   sign, notarize, staple and verify the DMG on a clean non-developer Mac.

## Useful verification commands

```sh
cargo test --locked --no-fail-fast
cargo run --release -- atmosphere-grid /tmp/kessel-atmosphere
cargo run --release -- campaign-verify /tmp/kessel-proof
tools/package_macos.sh --dry-run --offline --release-scope owner-dev
```
