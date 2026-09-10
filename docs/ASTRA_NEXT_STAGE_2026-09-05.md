# Panzerrechnersystem: next development stage

Audit begun September 4, 2026 PDT / September 5 UTC on HAL. Active executable/repository name:
Kessel; checkout `/home/alex/panzer/tank_simulator`, baseline `fb89dc7`. This document records the
user's renewed direction, verified implementation gaps, the first repairs, and the next playable
milestone. It does not certify the campaign as finished or the physics as experimentally validated.

## The direction

Sound is the primary means of understanding the war beyond the player's view. Abstract pixel
characters and vehicles provide distance from the subject while the spatial arrangement of objects,
light, physical action and sound carries its human reality. Pizza Doggy's purchased bundle supplies
the foundational art and sound vocabulary. The 4090 should buy better transport and simulation,
with measured budgets; expensive algorithms alone are not an aesthetic or a credibility test.

Priorities are an embodied Tiger, dangerous dismounted exploration, believable opposing behavior,
and specific historically grounded environmental scenes. Build a short coherent experience before
expanding the campaign or relying on hours of sequential playtesting to demonstrate a feature.
Use reproducible videos **with audio**, plus direct entry points to the relevant mechanic.

The retained design specifies five persistent crew, station workload, a compressed alternate-history
campaign, and a Reichstag last stand; the executable still has the older Halbe ending. Those choices
are recorded in `DESIGN_PILLARS.md` and `LINUX_4090_HANDOFF.md`; this audit does not silently replace
them. Historically documented scenes and the fictional travel of this particular Tiger crew must
remain distinct in authoring metadata.

## What is actually here

| Area | Verified state at the start of this audit | Consequence |
|---|---|---|
| Rendering | Raster primary visibility, exact secondary triangle/material/texture queries, one diffuse/GGX bounce, bounded glass transport, traced participating media and explicit snow | Some README/ENGINE text understates current code. Still not a general converged camera path tracer. |
| Performance | The old 1.5 FPS was five crew views; fresh solo cabin is 10.6 FPS at 640×360, while an exterior orbit is about 0.51 FPS at 320×180 | Both viewpoint and workload must accompany every FPS claim. Tiny pixel count does not eliminate repeated per-grain atmosphere solves. |
| Audio/impacts | Deterministic impact synthesis, delayed source reports, stereo/head shadow, live fragment-contact cues, hearing effects, shared game events | No measured structural/cavity pressure solver or geometry-derived impulse-response system. Some projectile semantics are lost between damage and sound. |
| Pizza | Full preserved source warehouse including 1,243 original GLBs and 914 audio files; active tree 1,194 GLBs; runtime catalog selects 120 Pizza models | Assets are underused, not all missing. The model/audio browser does not expose the whole preserved warehouse. |
| Soviet model | A complete expected pose-loading pipeline, but the replacement source/17 baked poses are absent from the active character folder | Actual infantry use rigid procedural fallback figures. Import/rigging must precede animation acceptance. |
| Infantry | Active perception, shooting, escort behavior and learned foot-policy integration | Structural movement collision is missing; they can traverse building geometry. They are a dismounted threat, not a finished infantry system. |
| Panzerfaust | Pickup/equip/aim/fire/flight/hit/presentation are present | Sphere hits and direct enemy destruction bypass the shared armor solver; splash presentation is not a complete damage solution. |
| Tiger embodiment | Hatches, wheels, breech, ammunition and several controls move | First-person arms are absent; living solo crew are hidden when no co-op frame is supplied. Static crew poses do not operate controls. |
| Map | Functional navigation/map interaction | The held map, compass and pencil are principally screen-space UI drawing, not the intended convincingly handled objects. |
| Story/editor | A historical dossier, 36 candidate scenes, 12 shortlisted scenes; visual story placements and editor schemas | The curated scene catalog has no runtime consumer. Saved sound emitters have no gameplay tick/playback consumer. |

Detailed evidence and limitations:

- [Acoustics and terminal-event audit](ASTRA_ACOUSTICS_AUDIT_2026-09-05.md)
- [Art, Pizza inventory and historical scenes](ASTRA_ART_STORY_AUDIT_2026-09-05.md)
- [Gameplay, characters, Panzerfaust, map and editor](ASTRA_GAMEPLAY_AUDIT_2026-09-05.md)

## Performance evidence and first repairs

Logs and machine-readable results are in `artifacts/astra-audit-20260905/`. The measured adapter is
NVIDIA GeForce RTX 4090 / Vulkan, driver 610.57.04. Measurements are optimized release builds on the
active desktop with other desktop applications still running. They are fixed-step, fenced headless
tests, not a measured interactive input-to-display latency result.

The existing `cpuprof` measures simulation, API submission and the final GPU wait. New opt-in
`KESSEL_GPU_PROFILE=1` adds actual GPU pass timestamps to its JSON and to `gfxbench` console output.
Repeated view passes are summed per simulation frame; query readback is outside the frame timer.
The instrumented pass durations may overlap and do not include TLAS construction, uploads or all
between-pass work. Timestamp support is requested only when explicitly enabled, and ordinary
gameplay never activates or reads the query buffers. API reference:
[wgpu timestamp queries](https://wgpu.rs/doc/wgpu_examples/timestamp_queries/index.html).

Initial results, same seed `13897573272538644481`, 24 measured frames after built-in warmup:

| Workload | Before | First cabin optimization |
|---|---:|---:|
| Solo campaign spawn, High, 640×360 | 94.11 ms / 10.63 FPS | 72.64 ms / 13.77 FPS |
| GPU volume estimate, instrumented | 60.09 ms | 45.58 ms |
| GPU surface combine, instrumented | 21.55 ms | 21.23 ms |
| GPU sky, instrumented | 5.83 ms | 0.002 ms |

The simulation update at this spawn is approximately 0.1 ms. This is not an AI-heavy scene, so a
contact benchmark is still required. The inspected tank policy forward pass is a small CPU MLP
(`src/ai.rs`); reserving GPU capacity for a hypothetical neural runtime is not a present explanation
for the measured stall. Preserve future AI headroom using real workload measurements.

The initial exterior test (`gfxbench tag 16`, High, 320×180) averaged **1969.76 ms**, of which
**1920.89 ms** was the physical snow pass. Its 16 camera samples traverse one orbit, not a stationary
frame. At each pixel the snow shader traverses cells and, for each contributing grain, can evaluate
multiple complete atmosphere-lighting integrals. This is the immediate performance priority.

First changes preserve the modeled scene, emitter powers, resolution, sample counts and transport:

1. Use existing G-buffer depth to evaluate sky only on primary misses. The same atmosphere shader
   remains in use; `KESSEL_UNGATED_SKY=1` retains a diagnostic baseline.
2. In volume lighting, return zero immediately for a fully occluded light rather than calculate
   atmospheric/local transmittance before multiplying by zero.
3. Apply the same visibility-first evaluation to snow lighting, while preserving dark grains'
   extinction. This is not permission to delete unlit grains or reduce snow density.
4. Correct armor-hit stereo bearing and listener distance to use the actual struck point and
   camera/listening eye, including turret-rotated stations, instead of the remote shooter and an
   unrotated role position.
5. Complete the preselected local Pizza sound bank from preserved source bytes and provenance.
   This is a small beginning; it does not implement the full Pizza sound foundation or spatial
   environmental emitters.

The first exterior repair bundle (visibility-first volume/snow lighting, sky depth gate and corrected
atlas packing) reduced the 16-frame orbit to **947.24 ms / 1.06 FPS**. Its snow pass fell to 905.94 ms.
Because the atlas correctness repair is included, this is a bundle comparison, not an isolated
snow-only speedup. The pixel result of the sky gate versus `KESSEL_UNGATED_SKY=1` is identical on the
captured final orbit frame (decoded-image PSNR infinite).

With that bundle, the cabin spawn measures 73.38 ms / 13.63 FPS; the contact scenario measures
73.45 ms / 13.62 FPS with three live enemy slots verified after warmup. Simulation updates are
0.094 ms and 0.128 ms respectively. These short captures establish a renderer-dominated starting
point, not a battle-load or interactive-frame-pacing certification.

A conservative snow-cell rejection experiment was also tested. Its independent reference cases
retained all accepted grain IDs, optical depths and ordering; the captured GPU image matched its
unculled comparison exactly. However, paired runs were **961.00 vs 963.00 ms**, a negligible gain.
The extra cull/helper/test code was therefore removed. Its isolated patch and test/timing/parity
receipts remain in the audit artifacts. This result supports prioritizing repeated source-radiance
integration instead of candidate enumeration. No claimed gain is a release-ready frame rate.

## The next playable milestone

Build one repeatable 10–15 minute section, entered directly from a development scenario selector.
It begins inside the Tiger, supports purposeful preparation and navigation, contains one tank
engagement and one brief dismounted task, encounters one sourced spatial story scene, and ends at
a clear extraction/survival boundary. All demonstrations must use actual gameplay state and the
same events driving the live audio mix.

### 1. Establish a usable render and audio budget

- Measure cabin, open hatch, optic, on foot, contact, smoke/penetration and a dense Pizza street at
  fixed seeds. Include percentile frame times, CPU simulation, GPU passes, audio underruns, memory,
  startup/asset loading, and observed window/input behavior.
- First remove redundant work. Snow candidate broadphase and per-particle lighting reuse deserve
  investigation before further atmospheric complexity. Distinguish a mathematically equivalent
  cull from a changed estimator; validate image differences and temporal stability explicitly.
- Proposed solo target: 60 FPS at the selected pixel grid, with a declared 30 FPS minimum under
  the heaviest accepted encounter. These are targets, not achieved results. Four/five-view crew
  performance is a separate budget and should not block the first solo milestone.
- The next architectural candidate is reusing solved atmospheric source radiance instead of
  reintegrating an entire atmosphere for each snow grain. [Bruneton's reference implementation](https://ebruneton.github.io/precomputed_atmospheric_scattering/)
  demonstrates physically parameterized precomputation with numerical/reference tests. Adapting that
  approach here requires explicit treatment of this engine's clouds and local media, cache invalidation
  and interpolation-error bounds; it is a proposal, not an implemented speedup or drop-in substitute.
- Do not increase path depth or simulation scale until cost, energy behavior, variance and visible
  benefit are demonstrated. Fix noise, near-black silhouettes and broad orange interior wash
  through scene/material/emitter/camera diagnosis against Pizza references.

### 2. Make one impact mean the same thing to damage, picture and sound

- Preserve actual projectile contact velocity, normal, material/plate, impulse/energy accounting,
  residual state and fragmentation/blast classification in one event contract.
- Resolve the audited filled-AP/HE semantic losses before sound tuning. Keep structural response,
  airborne cavity response, fragment contacts and listener hearing response as separately testable
  parts of the game audio model.
- Build a small source/receiver acoustic test scene: opposite plate hits; the same hit at all
  stations; head rotation; hatch open/closed; nonpenetration, penetration and filled-shell events.
  Use level-matched listening comparisons and objective channel/delay/decay checks.
- Firsthand descriptions constrain perception, sequence and relative character. They cannot by
  themselves calibrate pressure waveforms or prove absolute acoustic accuracy. Any modal/impulse
  response approximation needs stated geometry, boundaries, source data and error limits.

### 3. Make the crew operate the machine

- Recover the exact Soviet source with its provenance, validate scale/axes/bind pose, bake the
  expected pose set, then inspect walk/run/aim/fire transitions in the actual game.
- Give infantry structural collision and navigation before judging tactical intelligence from
  animation alone. Test cover, alert, pursuit, shooting, injury and persistent bodies.
- Establish the solo five-body roster and shared first-person arm rig. Start with one complete
  action: take a round, open breech, insert, close, traverse/elevate, fire, extract hot casing.
  Hands must contact the actual moving handles/rounds; interrupted actions retain coherent state.
- Then add hatch entry/exit, map/compass handling, driving controls and remaining roles. Avoid
  expanding the interaction list while the foundational reach/contact/ownership logic is absent.
- Route Panzerfaust contacts into shared game damage, including appropriate impact and blast
  outcomes, and add a direct demonstration scenario. Do not accept a cinematic explosion as proof.

### 4. Build the first visual and acoustic scene together

- Choose one of the documented scene candidates with clear provenance and uncertainty. Assemble
  it as a relocatable Pizza-based room/street unit with entrances, walkable collision, sightlines,
  cover, lighting and sound-source transforms authored in the same coordinates.
- Connect editor sound emitters to actual runtime playback, unloading, deterministic loops and
  listener motion. Sound must disappear/change for spatial reasons rather than only a scene trigger.
- Expand the asset palette from the preserved Pizza warehouse deliberately: establish room-scale
  composition, readable material values, prop density and cold/warm contrast using vendor examples.
  Supplementary models must be reconciled to that visual language.
- Give the map one coherent paper/ink/compass design before adding decorative UI. It must remain
  readable at the chosen internal/display resolutions and operable while the world continues.

### 5. Make review inexpensive and acceptance meaningful

- Add direct scenario entry for loader cycle, incoming-hit acoustics, Panzerfaust, infantry contact,
  map handling and the historical scene. One seed/event record should reproduce each.
- Record live audio alongside the gameplay timeline, with cue/event metadata. Existing staged
  films with assembled soundtracks remain useful previews, but cannot validate the live mixer.
- Deliver short normal-speed clips plus targeted slow motion where necessary; state capture FPS
  separately from measured runtime FPS. Include hands, object contact, sound direction, and what
  state changed, rather than only an exterior beauty shot.
- Verify one uninterrupted player run, including controls/discoverability, death/retry, pause,
  loading, inventory/resource ownership and the end condition. Unit/contract tests and successful
  captures are necessary evidence, not a substitute for playing the complete sequence.

## Additional gaps to keep visible

Persistent damage/corpses across encounters; a consistent campaign ending; simulation-driven enemy
loss rather than shortcut kills; source-derived collision; separate sound-source and hearing state;
content provenance; controller/keyboard discoverability; reproducible debugging; and a cooked owner
asset payload. Defer larger battle counts, full campaign expansion and advanced editor polish until
the short section establishes the intended experience.


## Review media and verification receipt

[Fresh Tiger mechanics video](../captures/astra_tiger_mechanics_20260905.mp4): 12.5 seconds,
1920×1080 H.264, 24 encoded FPS, stereo AAC at 44.1 kHz, rendered from High at 320×180. The existing
`terror-video` harness stages four co-op bodies and scripted mechanics in the compact arena. It
assembles its soundtrack separately and mutes the live mixer. This demonstrates current mechanics
and visual gaps; it does not validate corrected live impact positioning, Pizza playback or measured
runtime FPS. The clip passed its firing/loading/penetration choreography assertions. Reload and
impact frames were visually inspected; substantial gold/orange dominance, weak material separation
and missing first-person hands remain art/embodiment issues. The audio stream is present, stereo and
finite; digital peak is -7.84 dBFS. No subjective listening/physical acoustic calibration is claimed.

[Selected Pizza dry audition](../artifacts/audio/astra-pizza-selected-audition-20260905/pizza_audio_private_audition.wav)
and the cue CSV beside it expose the eleven imported source layers for owner review. Runtime logs
confirm all eleven load with `KESSEL_ENABLE_PIZZA_AUDIO=1`. The owner launcher enables this by default
and respects explicit opt-out; the procedural base and protected primary identities are still active.

After the texture-atlas repair, the complete release test run passed **710 tests across 20 suites**,
with **one pre-existing ignored audio-device initialization test**. Fifteen Pizza bank Python tests and shell launcher checks also
passed. The first full release run's seven failed surface/BVH tests exposed the atlas bug; all seven
pass unchanged after its correction, alongside the new multipage payload regression. The separate snow-cull experiment passed 28 physical-environment tests including two new reference
cases; its code/tests were then removed because measured GPU performance did not improve. These are tests and headless visual checks, not a completed
hands-on campaign playthrough.
