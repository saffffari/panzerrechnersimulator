# Panzerrechnersystem — current status (2026-09-29)

Panzerrechnersystem is a playable three-act prototype moving permanently to one authoritative CachyOS Linux
checkout on the i9-12900K / 128 GB / NVMe Gen5 / RTX 4090 workstation. Linux, Vulkan and the
4090/High path now define correctness, image quality, performance budgets and shipping scope. The
Mac/Metal build and Laptop preset are preserved historical/fallback paths only; they no longer
constrain effects, simulation density, renderer architecture or defaults. The intentionally low
internal pixel grid remains Panzerrechnersystem's visual language and creates extra compute headroom—it is not a
low-end accommodation. See `docs/LINUX_4090_HANDOFF.md` for transfer and acceptance gates.

The integrated executable still contains the earlier Halbe finale and lacks the newly locked
crew-trauma/rescue campaign. The canonical destination is now the Reichstag last stand, with a
nearly impossible, intentionally ambiguous escape easter egg. These are production decisions and
remaining work, not claims about the current build.

## 2026-09-29 urban intro district

- The intro is urban combat (owner, 28 September). A city-centre district of January 1943 now
  surrounds the Act I spawn in the workshop `assets/worldgen/workshops/act1-urban-intro-20260928/`,
  generated deterministically by `tools/worldgen/urban_intro_district.py` and played with
  `tools/launch_winter_session.sh --urban` (the default session is still the winter courtyard).
  See [`URBAN_INTRO_20260928.md`](URBAN_INTRO_20260928.md).
- 203 generated buildings (merchant houses, 1930s blocks, timber houses, workshops; intact, burned,
  shelled or partly collapsed) are live runtime instances beside the courtyard house: 23,226
  modules, destructible through the ordinary damage path, enterable on foot by their external steps,
  with dog-leg stairs to their upper floors, rubble mounds from failed modules, per-instance saves
  and LAN sync. Their dimensions come from cited research, their forms were checked against the 180
  reference photographs (`docs/research/`).
- Not yet: the 45 mm gun sites conflict with the district and the guns do not engage there;
  cellars are sealed under the terrain; rubble is render-only; the authored stormy dusk leaves the
  city nearly black; AI infantry still have no navigation.

## 2026-09-09 pause and checkpoint update

- Solo **Esc** now opens a pause menu with Resume, Save checkpoint, Load checkpoint, Settings,
  and a confirmed Exit game action. The simulation and game audio stop while this menu is open.
- Active Kampagne runs can write one manual disk checkpoint. Loading restores the saved sector's
  entrance state and the same numbered attempt/route, retaining saved story discoveries and the
  day/night choice. It does not restore current position, mid-sector combat or objective progress,
  or map annotations. An explicit manual load can restart a saved Act III entrance; ordinary
  finale defeat remains terminal. There is no autosave and Playtest cannot create these saves.
- Versioned saves are bounded, validated, and atomically replaced. Invalid or incompatible saves
  leave the current game intact. Campaign/route/world rules and authored data are compatibility
  checked; unrelated renderer, UI, and audio edits are excluded. The normal Linux location is
  `~/.local/state/panzerrechnersystem/campaign-checkpoint.v1.json`; an absolute `XDG_STATE_HOME`
  overrides the state directory. See [`CONTROLS.md`](CONTROLS.md#pause-save-and-load).
- This update describes the implemented path. CPU round-trip tests cover entrance restoration,
  same-attempt retries, Act III manual reload, invalid data, and disk replacement. A fresh human
  campaign playthrough is still needed to assess the full first-play experience.

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
  Split screen preserves the selected Laptop or 4090 preset while every cell stays at 320×180;
  occupied visible station viewports share one shadow result instead of each submitting a full
  shadow scene.
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
- A prior Hal audit reached the machine and found about 23 GB of VRAM free. Its Vulkan feature probe
  validated hardware ray queries and vertex-return support; this is historical evidence, not the
  post-transfer acceptance receipt. The restored checkout still has to log the RTX 4090, Vulkan,
  `ray_query=true`, `ray_vertex_return=true` and the ray-query/deferred pipeline enabled, then compile
  and render a complete High scene plus the four-view benchmark. Stop Ollama/llama-server first so
  their roughly 18 GB reservation
  cannot disguise a renderer or allocation failure.
- Current integrated Mac source receipt: formatting, all-target compilation and the locked release
  build pass. Across all targets **703 tests pass** and one real-host-audio test is ignored. A
  restricted sandbox run reports three loopback-socket `PermissionDenied` failures only; the full
  22-test LAN subset passes when sockets are available. Linux must reproduce the suite rather than
  inheriting this receipt.

## Playable now

- Acts 1–3 carry one Tiger, its damage, stores, discoveries, and route state through the campaign.
- Solo, split-screen seats, and host-authoritative LAN crew play use the same embodied commander,
  driver, gunner, loader, and radio/bow-MG roles.
- Local split-screen has an explicit Keyboard + controllers / Controllers-only selector. The latter
  assigns pads 1–4 to Commander/Gunner/Driver/Loader with no phantom keyboard roster, supports a
  one-pad Commander drive/lay/fire fallback, and routes map/control overlays through that pad.
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
- The authored campaign worlds and encounter placement remain thin. Panzerrechnersystem has the mechanical loop,
  but not yet the dense, dangerous, explorable warzone it is meant to become.

## Authoritative renderer and atmosphere

- Winter has a dense, depth-aware, crisp-pixel blizzard with world-fixed flakes, gusts, spindrift,
  interior shelter masking, and usable visibility through open hatches and optics.
- Every act has deterministic dawn/day/dusk/night profiles. Overcast, haze, smoke and restrained
  visibility replace the old clear-sky, glorious-weather look.
- Internal-resolution presets (640×360 through 1280×720) and independent clean/light/standard/strong
  retro-filter presets make honest A/B captures possible. Current image comparisons favor 800×450
  visually, while 640×360 remains the default and reference grid until an RTX 4090/Vulkan quality
  and frame-time receipt justifies a change. The next budget increase should buy better transport,
  volumetrics, particles, destruction and simulation before merely adding pixels.
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
  campaign checkpoint. Retry and explicit disk-checkpoint load restore its entrance state,
  successful completion commits it, and a blocked/lost completion cannot overwrite it. Mutable state is
  exact module integrity, 1/256-quantized sub-point damage, and atomic shelf/container state. All
  current doorways are permanently open; door-leaf gameplay and partial per-item loot are not live.
- A compact bounded copy of that state is host-authoritative over LAN and is included for late
  joiners. Gameplay protocol v3 uses a host epoch and fresh client challenge to reject stale and
  duplicate packets without reopening old sequence spaces across wrap or host restart; discovery
  remains format/version v1.
- This is one proved vertical slice, not a finished city system. Most landmarks remain sealed or
  placeholder geometry, and the editor still needs first-class floorplan, room, stair, door, loot
  and structural authoring.
- Since 2026-09-29 the same runtime builds every `building_instances` placement of a manifest
  revision from the building grammar (four typologies in four conditions), with collision proxies
  generated from the kit's source meshes, external steps, dog-leg stairs, instanced rendering and
  spatial grids; the urban intro district holds 203 of them (see the section above).

## Runtime assets and preserved Mac package

- The authoritative reviewed model closure is exactly **155 GLBs / 241,657,292 bytes**: **120
  Pizza** models / 79,558,992 bytes and **35 non-Pizza** models / 162,098,300 bytes. The latter are
  16 Tiger/T-34 models, 15 static Panzertruppe identity/pose meshes, the physical Panzertruppe
  binoculars, Enigma machine, authored P08 and skeleton casualty/anatomy reference. The Linux
  restoration must reproduce this through `python3 tools/runtime_asset_cook.py validate`.
- The regenerated non-GLB catalog declares **22 files / 399,651,573 bytes**. Together the two
  catalogs describe **177 runtime files / 641,308,865 bytes**. Packaging and future handoff receipts
  must still read `file_count` and `total_bytes` from the validated catalogs rather than carrying
  these numbers forward after assets change. Raw authoring libraries, source archives, screenshots,
  video and retired models remain outside the runtime cook even when the private owner-transfer
  archive preserves them.
- The last fully packaged owner-development checkpoint is `dist/macos-f8496b9-final/`: its app occupies
  **627,716 KiB (about 613 MiB)** and its compressed DMG is **534,839,582 bytes (about 510 MiB)**.
  The DMG SHA-256 is
  `2d57a97bf0e686e2868b0bc146f5efd2bced11992bc5f7904cb13257911d3d72`.
- The ignored artifact presently in `dist/macos/` is an internally valid but stale owner/developer
  snapshot. It embeds the older **147-GLB / 491,705,164-byte** model closure and must not be presented
  as a build of current `main`.
- Preserved Mac source receipt: **674 passed, 0 failed, 1 real-audio-device test ignored** under the
  full locked suite; the locked release build also passes. The packaged app and DMG independently
  pass resource hashes, ad-hoc hardened-runtime signatures, architecture/minimum-OS checks and disk
  image verification. With repository asset overrides removed and its working directory outside
  the repository, the bundled executable loaded Garage and all three acts from `Contents/Resources`
  and rendered one Garage frame plus 12 Metal dawn/day/dusk/night frames. The atmosphere grid is
  exact 2x-nearest from 640x360; the Garage frame is exact 3x-nearest.
  That package predates the resolution/retro controls, expanded impact response, armor wounds/track
  repairs and articulated turret floor; a fresh package must be cut before those are called shipped.

That package and receipt are archival migration evidence, not the current release target. New work
is accepted on Linux/Vulkan. Friend/public distribution is still blocked by the 16 tank models' and authored P08 bake's
unrecorded provenance, unverified Mapzen-derived Garage/Act I redistribution rights, review and
bundling of applicable Rust dependency licences, and a re-check of Pizza Doggy terms and purchase
evidence. A Linux friend build also needs an explicit packaging format, NVIDIA/other-GPU support
policy, dependency licence bundle, firewall/LAN instructions, controller and 2/3/5-crew testing,
and a clean-account install-and-launch test. The old Developer ID, notarization and DMG gates apply
only if a Mac release is deliberately revived.

## Locked campaign direction (not yet implemented)

- Single-player is the design authority. The run is a continuous four-to-six-hour ironman journey
  across three loads, with a rolling irreversible save. Commander death ends the run; there is no
  reload that erases a casualty.
- The starting crew is the entire roster. No replacements arrive. Crew are not a set of isolated
  biographies, affinity meters or side stories: the player is the crew as one interdependent body.
  Attachment must arise from presence, competence, shared procedures, rescue attempts and the
  permanent operational silence left by a missing station.
- A dead or incapacitated crewman removes his labor. The player must physically move to cover the
  station, accepting the blind time and abandoned duties that movement creates. Loader assistance
  remains configurable (Assisted / Hybrid by default / Manual), but difficulty settings may not
  restore a dead comrade.
- Penetration fragments create anatomically located wounds and organ/limb consequences. Hidden blood
  volume, bleed rate, pressure, shock and consciousness drive diegetic behavior rather than a HUD
  countdown. Direct pressure, limb tourniquets and wound packing can stabilize eligible trauma;
  catastrophic head, heart and central torso damage can be beyond field treatment.
- The starting kit is period-researched and scarce. Fixed, concealed Soviet aid posts hold the only
  major resupply/definitive-care opportunities. Armed troops defend them; wounded patients and
  medical staff remain noncombatants, and careless fire can destroy supplies or care capacity. The
  player may have to leave a disabled Tiger under fire, clear a route, recover supplies, return and
  finish treatment. The many physical steps, continuing battle and blood loss make rescue possible
  in theory and usually impossible in practice.
- The driver and radio front hatches plus turret-side escape hatch must become physical, openable,
  damageable and jammable. A casualty can block a station or route. Amputation and later return with
  a crude role-specific prosthesis is an explicit gameplay abstraction, justified only across
  inter-act rehabilitation time and carrying a lasting performance penalty.
- A future local language model may produce terse crew-condition reports through the physical
  Enigma/message interface. The deterministic simulation owns truth; the model receives only what
  the crew could perceive, may report uncertainty or silence, and can never invent damage, reveal
  omniscient information or steer the story.
- FX development follows the same physical event as damage and audio. Shell energy, fragment paths,
  material, pressure, fuel, temperature, oxygen, ventilation, weather and engine state should drive
  sparks, dust, blood, smoke, fire, extinguisher discharge, exhaust and persistent aftermath. Low
  resolution should make this coupling denser and more lavish, not excuse generic billboard bursts.
- Orders, rumors, route evidence and encounters gradually reveal the purpose: reach Berlin and
  defend the Reichstag at all costs. The Reichstag defense is canonical, unwinnable and the campaign's
  intended last stand. Which crew remain alive determines which stations answer, what the tank can
  still do, how long the stand lasts and what its final silence means.
- An exceptionally hidden hope ending may follow only after the Tiger is knocked out at the
  Reichstag. Mobile survivors must bail out, cross collapsing Berlin without being shot or captured,
  find a usable civilian car and escape. A restrained time/season cut may show that car arriving at
  an intact cottage in green pasture with one warm light—no spouse, children, reunion, scorecard or
  exposition. It preserves the crew-as-player by allowing mobile comrades to leave together, remains
  nearly impossible, and is intentionally ambiguous alternate history rather than a claim of a
  documented Tiger or crew escaping those events.

## Immediate production order

1. **Accept the migration.** Verify the archive checksum and manifest, restore the Git bundle and
   private asset payload, then capture a self-describing RTX 4090/Vulkan receipt—commit, driver,
   adapter/features, build, scene, warmup/timed frames and avg/p50/p99. Do not discard the transfer
   archive or Mac checkout until this passes.
2. **Integrate reload truth.** Replace the fragmented live reload/round/case state with the tested
   invariant aggregate and prove conservation through interruption, retry and late join.
3. **Persist resumable work.** Version LAN resumable-task state and add an atomic versioned disk
   run-save before multiplying mutable props.
4. **Scale the vertical slice.** Turn the proved building into a dense 500×500 m warzone block with
   varied multi-floor interiors, destruction, scavenging, debris, cover, forest and streets. Keep
   compact building state bounded and authoritative as instance count grows. Begun 2026-09-29:
   the urban intro district (585 × 600 m, 203 generated buildings); next are its encounter sites,
   tableaux, open cellars and verification (`URBAN_INTRO_20260928.md`).
5. **Worldbuilder and terrain.** Add floorplan/room/door/stair/loot tools, structural tagging and collapse
   preview, density brushes, spline roads/trenches/patrols, semantic zones, encounter sockets and
   validation overlays. Replace the coarse ground treatment with chunked/LOD terrain, layered tiled
   materials, vegetation LOD and interest-managed collision/destruction/room streaming.
6. **Finish the gun and signature slice.** Complete main-gun firing/reload sensory work and the shared
   engine/track signature model before broadening the component simulation.
7. **Crew survival and final campaign.** Build the deterministic anatomy/treatment state, three
   additional hatches, physical rescue loop and truthful Enigma report interface. Replace the
   implemented Halbe codas with the Reichstag build-up, unwinnable defense and near-impossible
   ambiguous escape.
8. **Doctrine, co-op and release.** Wire six-tank fix/mask/flank doctrine into real perception,
   movement and smoke in the dense route; keep ten-tank drills blocked on the hard six-enemy pool.
   Stress mutable-task synchronization and late joins at world scale; exercise 2/3/5-seat LAN and
   split screen; then clear provenance and Linux packaging/distribution gates.

## Useful verification commands

```sh
cargo test --locked --no-fail-fast
KESSEL_GRAPHICS_PRESET=high cargo run --release -- rtprobe linux4090-rtprobe.png
KESSEL_GRAPHICS_PRESET=high cargo run --release -- screenshot linux4090-high.png
KESSEL_GRAPHICS_PRESET=high cargo run --release -- atmosphere-grid /tmp/kessel-linux4090-atmosphere-20260902
KESSEL_GRAPHICS_PRESET=high cargo run --release -- gfxbench koop 240 linux4090-koop.png
cargo run --release -- campaign-verify /tmp/kessel-linux4090-campaign-20260902
cargo run --release -- lan-verify
```
