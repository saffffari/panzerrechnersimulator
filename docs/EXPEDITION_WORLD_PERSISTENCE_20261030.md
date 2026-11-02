# Live expedition world persistence â€” implementation checkpoint

This is a separate format from the old campaign entrance checkpoint and the coordinator-only expedition JSON. Those formats cannot restore a battlefield. The implementation below is present on source; **no full-world GPU roundtrip has yet passed at this checkpoint**. Do not advertise verified Resume until the review receipt passes.

## Contract and entry points

- `expedition_world_save.rs`: `WorldRecipe::capture`, `WorldSave`, eleven required versioned `OwnerBlock`s, fixed-width little-endian bincode 1.3.3, per-owner and outer corruption checksums, bounded input, atomic replacement and a previously valid backup.
- `Game::export_expedition_world(&Renderer, WorldRecipe)` synchronizes the retained coordinator without advancing simulation, checks source identity and captures the owner set.
- `Game::import_expedition_world(&mut Renderer, &WorldSave)` is for a disposable Game rebuilt from the saved manifest/seed only. It checks the exact simulation build and source bytes, reconstructs the original source allocation sequence, relocates the existing camp geometry, restores all mutable owners and rebinds source presentation without a simulation tick.
- The frontend must replace its current Game only after import succeeds. Missing owners, unknown fields, incompatible code/source, invalid identity or differing source allocations are errors, never fresh defaults.
- `expedition_world_review::run(prefix)` builds the actual continuous theatre, explicitly stages a loader wound, turret-ring fault, live solver projectile and half-released workshop mounting, saves atomically, drops the first Game/Renderer, reconstructs a candidate, compares all owner bytes, and compares twelve ordinary continuation ticks. It writes flat images, a `.world.bin`, and a JSON receipt. Fixture damage is not represented as naturally occurring combat.

## Owner coverage

| Required owner | Authority retained |
| --- | --- |
| `game.physical` | Station/mode, walking and climbing transitions, controls and input latches, shell manipulation and reloading, track work, timers/cursors, mechanical items, mission sheet, exact renderer world clock and retained drawing records |
| `vehicles.tanks` | Tiger, all active/inactive enemy and allied records, powertrains, suspension, component faults, armor scars, external damage, fire, wreck state including detached turret rigid bodies, snow/finish wear |
| `crew.runtime` | Stable person/actor registry, assigned roles, active and archived actual Casualty state, procedure cursors, assignment travel, loader work, retained articulated bodies, anatomical injuries, extraction and animation state |
| `transport.recovery` | Halftrack motion, occupancy, tailgate, recovery equipment, passengers, finite cargo, service and tow operation state |
| `workshop.runtime` | Existing finite source parts/ItemIds, mount fastening/extraction/testing, held parts, ongoing work, primary-button guards, tool timing and source poses |
| `world.progression` | Theatre geography, mission dispatch serial, selected/active order, per-site guard identities and counters, elapsed clocks, frontline, camp relocation, rewards, finite ammunition carts and fuel pickup identities |
| `world.environment` | Structural damage with fractional health, exact rubble poses, doors, trees/scatter, grass springs and persistent tread compression, blood evidence, foam/fire residues, camp population paths/tasks, environmental emitter trigger history and future acoustic events |
| `combat.projectiles` | Every in-flight projectile identity/position/velocity/seed/flight epoch, fixed-step accumulator, fragment state, suppression/passing-sound flags, casings, launched rockets, impact/transient events and analytical visual particle records |
| `combat.infantry` | Actual soldiers and policies, AI clocks and encounter routes, enemy abandoned crews, infantry casualties, anti-tank guns |
| `expedition.runtime` | Finite personnel/items/resources, actual medical casualties and beds/procedures/supplies, operations, successor and medical-wait state, actor observations, retired personal equipment |
| `personal.runtime` | Actual controlled foot soldier/inventory, pickups and their remaining contents, hearing/burn state, hand map/manual state, sidearm, cigarettes and dropped butts, hand-held geometry timing |

The field audit reviewed the 423-field Game structure. Source meshes, frame/slot registries, immutable contact grids, graphics/device resources, per-frame acceleration caches, and diagnostic/profiling counters are rebuilt or excluded intentionally. LAN/couch, legacy campaign/director/tutorial state and presentation death replay are rejected by this solo expedition format.

## Exact source and build identity

`build.rs` fingerprints `src/`, `shaders/`, `catalog/`, Cargo.lock, Cargo.toml and build.rs. A code change invalidates a development save rather than silently reinterpreting it.

The resource resolver records each successfully resolved regular file's content hash on its first load. Save creation rechecks those exact bytes. The closure is deliberately conservative across multiple Games in one process; an earlier resolved but currently unused asset may remain in it. No entire asset directory is scanned or uploaded, and the save contains fingerprints rather than private asset contents. Direct filesystem source loaders must register their input before consuming it.

Hash-container serialization orders keys deterministically, so an otherwise identical owner has identical bytes after deserialization. Physical floats use binary IEEE representation, preserving negative zero and legitimate infinity sentinels. Existing validated coordinator JSON compatibility uses serde_json float-roundtrip parsing; it is not used to encode arbitrary physical state.

## First-frame reconstruction

A zero-duration Game update is deliberately not a restore operation: existing zero-step paths can still constrain joints, consume action edges or alter cadence. The save retains loose source models/visibility/material emissions, static/moving/dynamic instance transforms and hidden masks, live light/smoke/analytic particle records, sky/weather/pressure state and pixel particles. Source-owned deformation is rebuilt separately for crew, held tools and workshop optics. The exact retained draw records are reapplied after those bindings.

The existing sparse R8 track-rut material history is read back only during save, using the occupied rows of its bounded atlas; its queued, not-yet-rasterized stamps remain separate and are not consumed twice. Grass tread memory is a different physical presentation model and is retained separately.

## Deliberate limits and pending verification

- Same code and exact source bytes are required. There is no cross-version migration or cross-build promise.
- This first implementation validates a singleplayer expedition, not a network session or a legacy campaign save.
- Graphics temporal/denoiser history and audio device playback sample offsets are not serialized. Audio trigger/one-shot history, scheduled future events and player hearing are retained; an active device loop restarts on resume.
- The outer FNV checksums detect accidental corruption; they are not authentication or encryption.
- Whole-world export may perform an explicit GPU readback and substantial source hashing. This is not a cheap per-frame autosave path.
- The reviewed staged fixture and twelve-step continuation are necessary evidence, not proof of a complete multi-hour human campaign. Additional casualty/evacuation, destruction/counterattack, moving-camp, ammunition transfer and held-equipment saves remain necessary regression cases.
- Source allocation restoration for initial guards/fuel and later three-slot counterattacks must be exercised on the actual theatre; a nominal codec roundtrip is insufficient.

## CPU regression cases added (pending root execution)

`expedition_world_save::tests` covers IEEE bits and corruption/trailing data; the mandatory owner set; an actual powertrain continuing a shift with a broken track after binary reload; canonical hash-container bytes; and recovery of the previous valid generation after the primary atomic save is truncated.

The root agent owns build, test and GPU execution. This document records implemented scope and outstanding checks without implying a successful run.

## Frontend checkpoint (following release 26)

The title now has New Expedition and Resume Expedition. Pause has Save expedition and Load manual expedition save; the relief screen also accepts F5 to save without selecting a successor. Manual and automatic saves occupy separate files with previous-valid backups. Title Resume searches the latest readable generation; a fallback is reported. A fresh run replaces only the automatic slot, retaining the manual slot.

Automatic saves are triggered by actual mission dispatch/outcome, personnel death/succession, camp movement, run outcome, or 120 seconds of simulated play, with attempts no more often than every 15 wall seconds. The same gate applies during deliberate medical waits and paused relief selection. Save work resets the frontend frame clock so its disk/GPU wait never becomes simulated catch-up time. Failures leave the earlier generation and running world intact and show a notice.

Window saves add a validated optional frontend record for the paper's pencil marks, pins, cursor, held state and relief selection. Window Resume refuses diagnostic saves without that record. A restored dead commander opens relief selection; a living unconscious commander is not treated as dead. Previously authorized medical waiting is cancelled on load, retaining its elapsed time, and must be explicitly requested again. The newly imported Game remains paused until the player resumes.

These frontend paths are implemented but await root compilation and actual roundtrip evidence. The first release-26 world review stopped at its finite-fuel assertion before saving: fresh expedition initialization had inherited the demo's unlimited `None` fuel. Initialization now allocates the existing 420-litre finite playtest quantity once; repeat enable calls and save imports do not refill it. This is an explicit starting-resource balance choice, not a newly claimed historical capacity.

Explicit Main Menu, starting a replacement run, title/desktop exit, and leaving the dead-commander relief screen now flush the automatic slot before abandoning the retained session. A failed flush keeps that session and displays the error; it does not discard the save or add a new permission dialog.

## Reading-light correction in the same checkpoint

The release-26 theatre map review showed valid black-and-white ink/albedo but an almost black physical page. Its near-vertical printed side faced away from the cabin fixtures. The existing hand-item controller also explicitly stowed the flashlight whenever the map was held, and its ordinary forward-pointing pose would illuminate past the paper.

`hand_items.rs`, `hand_map.rs`, and `Game::drive_hand_torch` now allow the existing F/L flashlight during map/manual reading and aim its actual 15 mm source lens toward the selected printed region. The source mesh, intensity, reflector cone and occlusion are retained. The paper has zero emission; no new light or fill is created. F/L is admitted before the physical-document key gate. The geometry regression checks front-side placement, aim and unchanged source scale across view yaw/pitch and map zoom. Actual readability/exposure and hand placement still require the root's next capture; geometry tests are not visual proof.

## Release-27 allocation failure and correction

The actual `expedition_world_20261030_b27a` review wrote a 9,555,845-byte save with 198 source fingerprints, then failed before owner import completed. Its 3,223 saved loose-mesh transforms were all finite. The candidate omitted the four mission flare source clones created by the first ordinary mission tick. Rebuilding later counterattack tanks before those flare clones would also assign incorrect mesh identities.

The physical owner is now version 2. Renderer records every source clone as `(source slot, allocated slot)`; the save retains that exact allocation journal and all dynamic enemy rig/flame slot arrays. Restore validates the candidate's initial prefix and every future reference before replaying the remaining allocation sequence. It then restores the saved actor slot registry. An interleaved mission/reinforcement CPU regression rejects reordered allocations, forward references and gaps. No fixture light is rendered or mission tick run to manufacture the missing geometry.

The world review now adds a second phase only after exact owner and continuation comparisons pass: it keeps the current Game/Renderer alive while constructing another candidate on the same device through `Game::build_expedition_resume_candidate`, which the actual frontend also calls. The candidate includes menu sources. The receipt compares candidate owners and verifies the old world did not advance or mutate. This explicitly tests the frontend's two-world GPU allocation peak; it does not claim a human window-input test. Until that phase succeeds on the root's GPU run, the memory cost remains unverified.

The release-27 follow-up also retains renderer crew-unconsciousness direction/intensity and exhaust draw records, alongside the new spatial-theatre weather flag. Build/source fingerprints intentionally reject older development generations rather than applying defaults for newly required fields.

## Verified release-28 world roundtrip

`captures/expedition_world_20261030_b28a.json` completed with `status: passed` and `pass: true`. The actual enlarged theatre wrote 22,209,753 bytes with 198 source fingerprints. All eleven owners matched byte-for-byte immediately after fresh reconstruction; all eleven also matched after twelve identical ordinary simulation steps. The saved scene includes the current 24 retained site sentries and the 128-building world, with actual in-flight projectile, wounded loader, jammed traverse, removed fuel and half-finished workshop release fixture.

The same run then retained the old Game/Renderer while the actual shared frontend candidate constructor built a second world on the same device, including menu source meshes. All candidate owners matched the saved snapshot, all old-world owners remained unchanged, and no simulation tick was introduced by construction. This verifies that bounded two-world path on the reviewed machine. It does not verify every user input route, extended human play, or every combination of medical evacuation, camp relocation, boarding and destruction.

The next source checkpoint bumps `transport.recovery` to owner version 2 alongside the crew agent's new finite service and boarding records. Release-28's immutable binary and its save remain internally consistent with its earlier owner version; future builds reject those development saves through the strict build fingerprint.


## Open-paper paused restoration follow-up (source checkpoint for build31)

The shared candidate constructor now repaints the physical map texture and visible repair-manual leaves directly from saved state after all owners are imported. This recreates derived GPU ink/material bindings only: it does not update Game, move the saved paper, complete a turn or replay an input.

The existing whole-world review now saves while a detailed area map is open and an order is active. It compares the generated map ink bytes after cold reconstruction and through the actual frontend candidate constructor, in addition to the eleven-owner and twelve-tick continuation comparisons. The later manual case uses ordinary page turns, saves a noninitial spread midway through turning, and verifies exact owner state, reading state and page ink on the first paused frame. Old worlds are dropped before that additional case, so it never holds three GPU worlds. These additions were source-complete for build31 but had not yet been run when this note was written.

## Verified build31: active paper and full owner continuation

`captures/expedition_world_20261030_b31a.json` reports both `status: passed` and `pass: true`. Its save contains 20,144,532 bytes and 212 exact source fingerprints. All eleven owners matched after fresh reconstruction and again after twelve identical ordinary simulation ticks. The shared frontend candidate constructor retained the old world unchanged while importing a second world, with no constructor simulation tick.

The open detailed map's generated ink matched exactly after cold reconstruction and on the first paused frontend candidate frame. The noninitial repair-manual spread, saved midway through a turn, retained its complete owner state, page/turn state and exact page ink with zero post-restore simulation ticks. This verifies the implemented paused-paper restoration, not a window-input test or arbitrary cross-build compatibility.

After build32, the world witness adds a before-first-tick assertion using the actual fresh camp-entry helper. Its failure handler now explicitly forces `pass: false` even if an earlier phase passed; consumers must require both a passed status and a true overall result. Newly added field-care/carrier-door owners use `crew.runtime=2` and `transport.recovery=4`; workshop currently uses version3. These next-source layouts require their own rerun. Strict fingerprints continue to reject earlier development saves rather than invent missing state.
