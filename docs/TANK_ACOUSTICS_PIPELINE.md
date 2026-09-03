# Tiger mechanical audio and CAD acoustics pipeline

Status: design locked 2026-08-11. Historical authoring receipt: the Mac has Blender, Ableton Live 12
Suite, Convolution Reverb Pro, and Ableton's IR Measurement Device, which may remain an offline
audition/mastering bench. Linux/Vulkan on the RTX 4090 is the sole runtime and performance authority.
The game currently uses a generic short Schroeder tail for the steel interior; this pipeline
replaces that approximation with authored machine layers and geometry-derived impulse responses.

## Decisions

1. **Convolution Reverb Pro is the audition and mastering bench, not the simulator.** It can load our
   generated WAV impulse responses, split early and late responses, EQ/damp them, and print comparison
   renders. It does not infer acoustics from CAD.
2. **Acoustic simulation is offline.** The distributed game must not require Ableton, Blender, COMSOL,
   Steam Audio, Rosetta, or a GPU acoustic solver.
3. **Use a hybrid response.** Below roughly 300 Hz, the tiny steel compartment is dominated by wave
   modes and panel/air coupling; use a wave solver when available. Above that crossover, use geometric
   reflection simulation. Merge the two responses, then audition the unmodified and art-directed forms.
4. **Runtime uses a small baked response matrix.** Source and listener zones select/crossfade IRs;
   nothing retraces the full tank every frame. Hatch/penetration state can change the chosen response.
5. **Preserve dry masters.** Mechanical recordings, dialogue, weapons, and impacts remain dry mono or
   synchronized multichannel assets. Perspective and compartment coloration happen afterward.

## Acoustic geometry

Start from a duplicate of the Tiger source model. The original editable CAD/Blend source is not on this
Mac, so the first bake can use the retained hull/turret/interior GLBs plus `tiger_interior.json` and
`tiger_cameras.json`; a later source transfer should replace this proxy without changing the manifest.

The acoustic mesh is not the render mesh:

- Retain the hull and turret inner skins, firewall, basket, breech, floor, roof, major stowage, crew
  bodies, hatch/visor apertures, and large equipment masses.
- Remove bolts, handles, cables, gauges, texture-only detail, exterior running gear, and any surface
  much smaller than the shortest simulated wavelength.
- Repair normals and nonmanifold seams. Model the cupola, driver hatch, visor and penetration breach as
  explicit open/closed aperture variants, not material changes.
- Assign frequency-dependent absorption, transmission and scattering to armor steel, painted steel,
  rubber, wood, canvas/clothing, glass/optic, ammunition/stowage, human bodies, and open air.
- Keep dimensions in metres and preserve the engine's hull-local convention: +X right, +Y up, +Z front.

## Listener probes and source zones

Use the five actual crew locations already shared by damage gameplay:

| Listener | Hull-local position (m) |
|---|---:|
| Commander | `(0.35, 2.05, -0.50)` |
| Gunner | `(-0.50, 1.95, 0.30)` |
| Loader | `(0.55, 1.90, 0.10)` |
| Driver | `(-0.70, 1.05, 1.70)` |
| Radio operator | `(0.70, 1.05, 1.70)` |

Minimum source zones:

| Zone | Representative origin | Content |
|---|---|---|
| Engine/firewall | `(0.00, 1.05, -2.40)` | Maybach combustion, fan, starter, shutdown |
| Front transmission/final drive | `(0.00, 0.75, 2.20)` | gear whine, shifts, steering brakes |
| Left/right running gear | `(±1.75, 0.50, 0.00)` | tread, sprocket, suspension, terrain strikes |
| Breech/gun | `(0.00, 1.90, 1.00)` | firing, recoil, return, breech and cases |
| Turret traverse | `(0.00, 0.90, 0.00)` | hydraulic motor, gears, hand wheels |
| Ready racks | `(0.00, 0.85, -0.10)` | rounds, clips, rack rattle, loose equipment |
| Radio | `(0.55, 1.15, 1.50)` | receiver noise and platoon net |
| Crew seats | the five listener positions | voices, clothing, boots, local controls |

First response matrix: five listeners × eight source zones × two aperture states (buttoned/open) =
80 source-to-ear responses. Add a breached state only after the penetration aperture has a stable size
model. A 0.5 s stereo 48 kHz float response is about 192 kB, so the whole uncompressed first matrix is
roughly 15 MB before lossless packaging—small enough to keep source identity rather than collapsing all
seats into one generic tank preset.

## Simulation and Ableton workflow

1. Export the simplified acoustic mesh and material table from Blender.
2. Low band: solve pressure acoustics/FEM for 20–350 Hz at each source/listener pair. If COMSOL is not
   available, retain measured/hand-fit compartment modes as an explicit approximation rather than
   pretending a ray tracer models wavelengths larger than the compartment.
3. High band: bake reflections using Steam Audio or an equivalent geometric solver with high offline ray
   and bounce counts. Export directional/Ambisonic responses where practical.
4. Time-align and crossfade low/high responses around 250–350 Hz. Do not peak-normalize individual IRs;
   preserve relative source-to-seat gain. Store a separate calibrated gain scalar in the manifest.
5. Load the early and late WAVs into Ableton Convolution Reverb Pro. Compare dry impulses, speech,
   footsteps, tread transients, breech, MG, and 88 mm reports. Pro permits separate early/late IRs, which
   is ideal for keeping the CAD-derived arrival pattern while carefully shortening or damping the tail.
6. Export `physical`, `production`, and `open_hatch` renders for blinded listening. The physical response
   stays archived even if the production response is darker/shorter for intelligibility.
7. Runtime: use zero-latency head convolution plus partitioned FFT tail convolution. Crossfade response
   changes over 80–150 ms. If that misses the authoritative Linux/RTX 4090 four-view CPU budget,
   convolve one shared interior bus per local listener rather than every source independently, while
   keeping source-zone early taps.

Steam Audio is suitable as an **offline** reflection baker and produces convolution IR data. Its
historical macOS x64/Rosetta constraints matter only to an optional offline Mac bake; do not add it
to the game until its Linux x86_64 runtime, licence and package behavior pass the authoritative build.

## Mechanical sound system

### Continuous synchronized beds

- Maybach: starter, ignition catches/misses, idle, load, overrun, fan, exhaust and shutdown.
- Front transmission/final drive: gear-dependent whine, shift/clutch events, steering brake strain.
- Independent left/right tracks: tread-link cadence from actual track speed, sprocket tooth rhythm,
  return-run slap, mud/snow packing, and skid during pivot turns.
- Suspension/hull: roadwheel shocks derived from sampled terrain height deltas, bump stops, torsion-bar
  groan, hull flex, loose-tool and ammunition-rack sympathetic rattle.
- Turret/gun: powered hydraulic traverse tied to engine RPM, manual traverse wheel, gear mesh, hard stop,
  elevation handwheel, recoil/return-to-battery, breech, ram, extractor and falling brass.

No loop may free-run independently of simulation. Track cadence follows left/right ground speed; engine
pitch follows RPM/load rather than vehicle speed; turret motor stops with the traverse; suspension hits
come from roadwheel/terrain impulses. Exterior and interior are synchronized stems from the same event.

### Footstep materials

The world/editor schema needs a semantic `foot_surface` tag, not filename guessing:

- `snow_fresh`, `snow_packed`, `mud`, `dirt`, `gravel`, `rubble_brick`
- `concrete`, `wood_floor`, `metal_plate`, `metal_grating`, `ice`, `water_shallow`

Every module, terrain control material, rubble field and vehicle walk surface compiles this tag into the
runtime cells. Foot cadence is distance-based and stance-aware; each step selects heel/toe variants,
applies weight from speed/landing/falling inventory, emits an AI-hearing radius, and routes through the
current acoustic source zone. Running on turret steel must therefore excite the compartment and be heard
outside differently from walking on snow beside the tank.

The old inventory mentions six candidate step families (snow, metal, gravel, dirt, concrete and wood),
but none is installed in the current runtime. Treat them as a shopping/recording list, not existing
coverage. Each final family needs at least 8 clean mono variations plus scuffs, starts/stops, crouch steps,
jumps/landings and carried-shell weight variants.

## Acceptance gates

- Blind A/B: all five seats can identify engine rear, transmission front, left/right track and breech.
- Buttoned/open transitions are click-free and do not change the dry event's timing or pitch.
- A spoken command stays intelligible under idle, movement and fire without muting the machine bed.
- Track cadence cannot drift from animation/ground speed over a ten-minute drive.
- Footstep surface changes occur on the exact authored boundary; no snow step plays upstairs.
- Destroying/opening a hatch or breaching armor changes the response without an abrupt tail cut.
- Solo, 2P and 3P audio CPU p95 stays inside the chosen budget on M1 Pro and Linux.
- All shipped IRs and recordings have source, creator, license, hash and processing-chain records.

## Tool references

- [Ableton Convolution Reverb](https://www.ableton.com/en/packs/convolution-reverb/) — Convolution
  Reverb Pro, separate early/late responses and the IR Measurement Device used for auditioning and
  mastering the generated responses.
- [Steam Audio C API](https://valvesoftware.github.io/steam-audio/doc/capi/index.html) and
  [offline reflection baking](https://valvesoftware.github.io/steam-audio/doc/capi/baking.html) —
  geometry-driven high-frequency reflection simulation and impulse-response export.
- [Steam Audio impulse responses](https://valvesoftware.github.io/steam-audio/doc/capi/impulse-response.html)
  — channel and sample representation for exporting baked responses rather than shipping the solver.
- [COMSOL Acoustics Module](https://www.comsol.com/acoustics-module) and its
  [hybrid wave/ray room-acoustics workflow](https://www.comsol.com/blogs/modeling-room-acoustics-using-a-hybrid-approach/)
  — the reference path for resolving low-frequency compartment modes and combining them with a
  geometric high-frequency response.
