# Kessel — Day 0 to Now: Visual and Gameplay Audit

Date: 2026-08-11
Truth point: `main` at `38b4b6e`
Purpose: reconcile the project's original intent, the systems that actually exist now, the most
important visual/gameplay gaps, and the next mechanics worth building.

## Executive conclusion

Kessel is no longer missing a game loop or an engine. It has a distinctive playable machine, a
three-act campaign state model, real armor consequences, manual loading and multi-station play, on-foot
scavenging, atmosphere, destructible architecture, LAN crew authority, an editor, and a repeatable
Mac build. The foundation is unusually deep for the amount of authored world presently surrounding
it.

That imbalance is now the central problem. The Tiger often looks and behaves like a finished hero
asset inside a blockout. Maps do not yet provide enough midground mass, human presence, tactical
structure, micro-history, or salvage decisions to exercise the systems already built. The next
quality leap therefore comes from a **dense 500×500 m campaign vertical slice**, not from adding
another isolated simulation subsystem or merely increasing resolution.

The highest-leverage rule for the next phase is:

> Every new visual object should either carry information, support a physical task, change a route,
> preserve the history of violence, or make the world feel inhabited. Prefer one object doing three
> of those jobs over ten decorative props.

## 1. What changed from day 0

### Era A — the paper vision and Unreal scaffold

The earliest surviving design and build reports establish the core identity clearly: one Tiger,
first-person embodiment, hostile weather, intermittent catastrophic combat, physically grounded armor and
sound, and a cold world punctured by a few warm lights. The first implementation was an Unreal 5.8
prototype with a drivable placeholder tank, fog, ballistics, basic crew/module damage, enemy stubs,
and a world-map scaffold.

What this era got right:

- simulation should be deeper than the deliberately abstract image;
- sound and incomplete information should be primary gameplay;
- the tank interior should be a spatial interface rather than a HUD skin;
- combat should be short and consequential, separated by long periods of uncertainty;
- the route home should pass through evidence of the failed advance.

What became obsolete:

- Unreal-specific implementation plans and renderer claims;
- the absolute prohibition on leaving the tank;
- the claim that allies must never appear;
- the claim that maps must be entirely handcrafted and never procedurally assembled;
- the claim that the story contains no dialogue or physical messages;
- the earlier absence of fuel as a resource.

Those were useful constraints for the first prototype, but current Kessel has a stronger identity:
**first-person and physically embodied everywhere**, inside the Tiger or on foot, with co-op crew and
authored procedural world layers.

### Era B — the custom Rust/wgpu game

The project moved from an engine prototype to a custom Rust/wgpu renderer and simulation. By the
initial reconciled Git import, most of the distinctive mechanical spine already existed: Tiger and
T-34 rigs, multiple stations and optics, manual loading, tank movement, on-foot play, ballistics,
spall/crew/modules, fire/cookoff, procedural acoustics, stealth, campaign structure, AI training
harnesses, and headless visual verification.

This was the decisive architectural choice. Kessel gained deterministic state, source-level control
of the low-resolution renderer, cheap headless tests, exact-nearest screenshots, and one simulation
shared by solo, split-screen and network play. It also inherited a cost: many high-end visual systems
that an off-the-shelf engine supplies now have to be built and maintained directly.

### Era C — world and Mac product foundation

The August consolidation made the game reproducible rather than workstation-bound:

- one clean `main` checkout became the source of truth;
- Metal forward rendering was added without compiling Vulkan-only ray-query resources;
- campaign terrain and editor manifests were unified;
- runtime assets gained deterministic catalogs, cooking and bundle resolution;
- the raw multi-gigabyte authoring library was reduced to a reviewed runtime closure;
- owner-development `.app`/DMG production became repeatable;
- LAN discovery and friend-facing crew setup landed;
- terrain/world placements moved to shared prototypes, packed pages and spatial culling;
- weather became act-specific, depth-aware and shelter-aware;
- a real enterable/destructible two-storey Pizza building proved the blueprint/runtime approach;
- building damage, loot and collapse became campaign- and LAN-persistent.

This era solved infrastructure that is easy to undervalue. Without it, every later mechanic would
remain a local demo.

### Era D — consequence and sensory detail

The newest work has started joining previously separate systems around one combat event:

- internal-resolution and independent retro-strength presets enable honest A/B comparisons;
- authentic tank-audio references and a geometry-derived interior acoustics plan are cataloged;
- impacts drive directional sound, station-distance severity, concussion and live spall feedback;
- armor retains local dents, holes, weakened regions and bounded persistent repair state;
- field repair uses visible spare track shoes instead of a magical generic patch;
- armor hits now shove the camera in the shell's actual travel direction;
- the Tiger's turret-floor disc is a real asymmetric articulated mesh that traverses with the turret.

The game is beginning to feel like one wounded machine instead of a collection of subsystems. That
is the correct direction.

## 2. What must be protected

These are Kessel's strongest, most ownable qualities. New work should reinforce rather than replace
them.

1. **Embodied work.** Loading, hatches, sights, carried stores and repair are substantially spatial.
   Station clicks/Q and one-click mounting still bypass timed physical transitions, so embodiment is
   a direction with known seams rather than a finished invariant.
2. **Information inequality.** Buttoned safety costs sight and hearing; exposure restores information
   but creates vulnerability.
3. **Persistent consequence.** Holes, patches, lost modules, looted rooms and collapsed structures
   remain part of the in-memory run. Loose/spent shells are still scene-local, and there is not yet
   an atomic disk save for a multi-hour campaign.
4. **Cold restraint.** Overcast, haze, blizzard and near-dark interiors make isolated warm light and
   sudden violence meaningful.
5. **Deterministic simulation.** The same event can feed rendering, audio, AI fitness, persistence and
   LAN replication without contradictory outcomes.
6. **Abstract rendering, serious systems.** The low-resolution image is an art direction, not an
   excuse for shallow behavior.
7. **The tank-to-foot loop.** The Tiger is power, safety, storage and noise; being outside is access,
   information and extreme vulnerability.

## 3. Visual audit

### 3.1 World density and composition — largest gap

Current maps contain many placements numerically, but the authored visual hierarchy is weak. Act I
is still a small city patch inside a large field. Acts II and III are dominated by repeated trees.
The camera often sees foreground snow, a few isolated objects and a fog horizon, with too little
midground structure to create depth or tactical reading.

Improve by authoring at four scales simultaneously:

- **hand reach:** labels, bolts, tools, casings, clothing, food, mud, blood, soot;
- **room:** furniture groups, storage logic, doors, stairs, light sources, personal history;
- **street/forest lane:** facade rhythm, fences, poles, wires, vehicles, shell holes, cover and routes;
- **horizon:** church, factory, smoke column, treeline wall, wrecked column, artillery flash.

Do not scatter every asset everywhere. Use dense story clusters separated by readable traversal
lanes and occasional visual silence.

### 3.2 Terrain surfacing — still betrays the world scale

The original mega-drape cannot supply convincing foot-scale ground. Even where its nominal texel
density is adequate, the source contains macro satellite color rather than local snow, mud, gravel,
rubble and track structure.

Build a layered material system:

- low-frequency macro color and wetness;
- authored control masks for snow, mud, ash, stone, grass and rubble;
- small crisp tiled albedo/normal/roughness sets;
- slope and height rules;
- road/trench/crater/track stamps;
- persistent local changes from tracks, explosions, fire and blood.

Ground marks should be tactical memory: tracks reveal a route, churn reveals recent movement, crater
direction suggests incoming fire, and clean snow signals an unvisited area.

### 3.3 Architecture and interiors — correct foundation, insufficient variety

The featured building proves rooms, stairs, lights, loot, collision, shelter, cover and collapse can
share one blueprint. The next step is not another bespoke one-off. It is a grammar that makes
different complete buildings from the same Pizza modules.

Needed variants:

- 2–5 storey row houses, workshops, sheds, schools, clinics, warehouses and military offices;
- intact, burned, shelled, partially collapsed and occupied states;
- room archetypes with plausible furniture/storage;
- cellars, courtyards, alleys and connecting breaches;
- partial collapse silhouettes that expose floors without flattening the whole structure;
- portal-driven lighting/acoustics/snow shelter and interest streaming.

Every enterable building needs at least one reason to enter, one threat, one navigational clue and
one story. Empty explorable geometry is not content.

### 3.4 Humans and signs of life — the most conspicuous missing layer

The world has sophisticated vehicle systems but almost no visible people. This makes settlements,
interiors, co-op stations and casualty scenes feel like sets.

Five boxy LAN crew placeholders already prove seat occupancy. The first useful human pass should
replace/rig those and extend the same bodies to solo NPC, injured and dead states:

- seated crew meshes with correct station silhouettes and full collision;
- simple gaze, breathing, bracing, recoil and task poses;
- wounded/dead pose variants occupying physical space;
- historically composed casualties at specific overrun sites;
- footprints, dragged trails, discarded kit and personal effects.

Human placement must be authored by tactical event, not a uniform corpses-per-square-metre rule.

### 3.5 Damage memory — strong system, weak surface language

Armor wounds and one modular building now persist, but most of the world still snaps between intact
and absent. Expand damage in a bounded hierarchy:

- small-arms strikes: cheap capped silver/primer chips, tracks or tools can occlude them naturally;
- cannon strikes: displaced plate, dent/entry/exit, soot direction, exposed primer and localized
  roughness;
- component damage: cracked optics, bent gun tube, jammed hatch, thrown track, missing panel;
- buildings: intact module → fractured cluster → grounded rubble and exposed room;
- battlefield: scorch, dust, snow displacement, fire, smoke and later settling.

Never simulate every brick or permanent bullet as an independent object. Store compact events and
compile them into decals, a few displaced meshes and state changes.

### 3.6 Lighting and atmosphere — mood works, transport is incomplete

The weather profile, palette and shelter masking are now distinctive. The weak points are the lack of
true volume interaction, flat cupola/interior regions, limited shadowed local lights and no common
cross-platform indirect-light solution.

Priorities:

1. local volumetric froxels or epipolar shafts for a few hero lights;
2. low-resolution moving cloud density feeding both sky light and weather;
3. shadowed clustered point/spot lights for interior lamps, headlights, fire and muzzle events;
4. a cross-platform radiance cache/probe or surfel GI approximation;
5. material transmission for glass and physically limited subsurface response for snow/skin;
6. improved smoke/fire volumes with extinction, self-shadowing and temperature-driven rise.

Super-real shafts are appropriate as art direction, but they should originate at real apertures and
lights so they remain spatially coherent.

### 3.7 Pixel treatment — tune, do not pivot blindly

The live game is **not using texel splatting**. Texel splatting is an isolated research subcommand
that captures probe cubemaps, shades texels and splats them as world quads. It is visually promising
for a single asset but has unresolved disocclusion, motion, world-scale and performance problems.

The current source renderer uses ordinary rasterization at a fixed internal resolution, HDR lighting,
posterization, 4×4 Bayer ordered dithering, restrained grain and exact nearest upscale. Independent
presets now make four useful endpoints: clean, light, standard and strong.

The resolution images show:

- 640×360 is the current default;
- 800×450 is the preferred visual candidate, with clearer silhouettes and retained pixel weight;
- 960×540 yields smaller visual gains and makes snow/dither physically finer;
- resolution cannot fix sparse scenes, coarse ground art or weak composition.

Before making performance claims or changing the default, record commit, Mac model, build profile,
scene, warmup/timed frame count and average/p50/p99 frame time. Normalize snow, dither and grain
against a 640×360 reference scale. Then test at minimum:

- 640×360 clean;
- 640×360 standard;
- 800×450 light;
- 800×450 strong.

### 3.8 Path tracing and Monte Carlo status — name it honestly

The current Vulkan ray path is a **stochastic direct-light/sky-visibility integrator**, not a complete
path tracer:

- one jittered sun-shadow ray per shaded pixel;
- one cosine-weighted sky visibility sample per shaded pixel;
- temporal accumulation across frames;
- direct point lights and a small ambient floor;
- no indirect bounce, no emissive GI, no multi-bounce transport, and no path-space material
  sampling.

On Apple Metal the required wgpu ray-query feature is unavailable, so Kessel never creates those
resources. Metal uses forward raster lighting plus a deterministic analytic compute occlusion pass:
contact sun visibility and three sky-cone marches against the heightfield and up to 12 OBB proxies.
It has no temporal Monte Carlo history or light bounce. Calling either current path fully path traced
would be false.

The best next lighting investment is a low-resolution cross-platform radiance cache or probe system,
then optional Vulkan ray queries can improve probe visibility. A true offline Monte Carlo path tracer
can remain a photo/reference tool. Runtime multi-bounce path tracing is lower priority than world
density, light clustering, volumes and Mac performance.

## 4. Gameplay audit

### 4.1 The loop exists; the world rarely forces it

Drive → detect → fight/avoid → dismount → scavenge → repair/reload → continue is mechanically
possible, but current levels do not repeatedly create those decisions. Routes need deliberate
pressure patterns:

- a loud fast tank route versus a quiet exposed foot route;
- fuel/ammo/medical needs whose likely sources are visible but risky;
- enemy armor that controls open ground without magically surrounding the player;
- buildings that provide concealment but can be penetrated or collapsed;
- weather lulls that temporarily reveal both opportunities and threats;
- safe-enough work periods after intense contact.

### 4.2 Crew roles are defined; task handoff is not yet the game

The five seats and basic co-op authority exist. The next gain comes from making a task interruptible
and transferable rather than simply giving each seat exclusive buttons.

Examples:

- a loader can leave a round aligned but unrammed for another player;
- the commander can mark a bearing physically for the gunner;
- a driver can hold a gear while someone else reaches the extinguisher;
- a wounded crewman can call out, drop an object, or block a passage;
- broken intercom forces shouting, movement or hatch signals.

The current reload implementation cannot safely absorb those features yet. Its truth is split across
the breech enum, lever, smoke, ram, ejection, shell nature, rack masks, carried mesh and casing flags.
The audit found concrete conservation errors: campaign setup creates a chambered round without
consuming a rack item; assisted reload can consume HE while leaving AP marked as loaded; export omits
some chamber/hand/floor locations; and released cases live in a short four-slot world pool instead of
riding with the turret. Introduce one renderer-independent reload/item aggregate before adding
hangfires, ghost placement or co-op handoff. Every round/case must occupy exactly one location.

### 4.3 Armor consequence is advanced; component consequence needs breadth

Armor, spall, crew/module hits, fire, concussion, scars and repair already form a convincing chain.
The next component layer should prioritize readable function:

- main gun/recoil mechanism;
- traverse power and manual fallback;
- sights/periscopes and replaceable glass;
- ventilation fan and accumulating fumes/heat;
- radio/intercom;
- engine accessories and removable deck panels;
- ammo rack lids and stowage exposure;
- tracks/suspension and visible mobility damage.

Each failure needs a symptom, diagnosis method, degraded mode and repair/salvage path.

### 4.4 Enemy AI has a training substrate but not battlefield doctrine

The current enemy system can aim, fire, hold distance and use encounter/platoon logic. Normal
campaign control remains scripted per tank, the learned network has only drive/steer/fire outputs,
and six slots are embedded throughout scene allocation and campaign wave sizing. Smoke does not
exist tactically; simply drawing it would still leave AI with perfect target state because line of
sight, engaged tracking and trainer observations ignore transmission. Enemy shots also use a
simpler resolver than the player's wall-transit AP/HE path. Fix those truths before training.

The target is not a universally smarter network. It is a small doctrine state machine feeding a
trainable low-level controller:

- find/fix/contact;
- crossfire and flank assignment;
- sacrifice/pressure element;
- smoke and displacement;
- casualty/disabled-vehicle response;
- break contact, withdraw and re-form;
- anti-infantry weapon/ammunition choice.

The reward must value role contribution, preserved firing lanes and platoon outcome, not only the
individual tank's survival or damage dealt.

### 4.5 Scavenging is present; diagnosis and logistics are not yet rich

Looting becomes memorable when the player can infer what is recoverable before touching a generic
container. Wreck posture, scorch, missing panels, hole direction, leaked fluids, thrown tracks and
crew evidence should suggest which components survived.

Fuel, radio, sight glass, batteries, medical supplies, ammunition and engine components should have
different sources, weights and handling. Avoid a universal repair currency.

### 4.6 Co-op architecture is credible; mutable task state is incomplete

LAN discovery, role ownership, bounded authoritative snapshots, late-join building state and stale
packet protection are significant progress. The live world snapshot still reduces reload state to a
breech enum, rounds-spent count and HE flag; it omits rack slots, lever/ram progress, carried/floor
rounds and casings. Co-op is not finished until that state is authoritative and 2/3/5-player task
flows, disconnect/rejoin during mutable actions, controllers, split-screen performance, Mac↔Linux
and a friend-installed notarized build are exercised.

## 5. New mechanics worth prototyping

These ideas are new combinations of systems Kessel already owns. They are ordered by leverage.

### A. Contact notebook / uncertainty ledger

Every uncertain sound or sighting can become a physical pencil mark with source, bearing, time and
confidence. A second observation intersects the first bearing. Old contacts decay or become crossed
out. In co-op, the commander owns the map while other players must speak or point. This joins sound,
map, watch, compass, weather and crew communication into one mechanic without adding a minimap.

### B. Signature budget

Every action emits one or more signatures: acoustic, visual, smoke, light, track and radio. Engine
RPM, headlights, a cigarette ember, muzzle flash, turret traverse, shouted commands, tracks in fresh
snow and a radio transmission all reveal different information at different ranges. The same event
model feeds player perception and enemy detection. Stealth becomes operating the machine, not a
crouch meter.

### C. Fault diagnosis through symptoms

Do not announce `FAN DESTROYED` or `FUEL LINE DAMAGED`. Present heat, smoke, gauge behavior, odor/crew
reaction, new vibration and sound. The player checks a physical panel, follows a line or opens the
engine deck. Correct diagnosis chooses the needed part and avoids wasting time. This turns component
damage into investigation rather than inventory clicking.

### D. Interrupted physical task graph

Reloading, first aid, repair, map work and radio procedure become resumable task graphs. Hands,
objects and controls preserve their last state when interrupted by a hit or station change. Another
co-op player can finish the same task. This creates natural panic and teamwork while preventing
repetitive restart-from-zero busywork.

### E. Weather windows

Subtle deterministic weather variation should alter information and routes, not simply color. A
brief blizzard lull exposes a crossing and both platoons; a gust masks engine noise but ruins optic
clarity; spring rain raises engine load and track noise; fog pools into shell craters and streets.
The encounter director can schedule pressure around these windows without visibly cheating.

### F. Wreck autopsy

A dead Tiger is a spatial puzzle. Entry/exit holes, soot, turret position, open hatches, empty racks,
crew placement and removed panels tell how it died and what remains. Deducing correctly finds a
working radio, optic, track shoe, fuel or engine part; guessing carelessly costs noise, time or a
booby-trap/mine risk. This makes the map's history mechanically useful.

### G. Compartment habitability

Firing, fire, fan state, hatch state, weather and engine heat determine smoke, temperature and
breathability. Opening the hatch clears the compartment but exposes light, sound and bodies. A
working fan enables sustained fire; a broken one forces pauses or dangerous unbuttoning. This makes
the requested fan damage, breech smoke, lanterns and weather part of one system.

### H. Ammunition geometry as risk

Rack location, lid state and shell nature affect retrieval time, cookoff/spall exposure and weight
distribution. Open lids speed loading but increase vulnerability. Loose or spent rounds become
collision/noise hazards under violent movement. The loader's optimization creates future risk.

### I. Collapse as tactics

Buildings are not merely destroyable cover. A crew can deliberately breach a wall, collapse a stair,
block a street, expose a firing lane, extinguish a room by smothering it, or create rubble that a
tank cannot cross but a person can. Enemy doctrine should understand a small authored vocabulary of
these outcomes rather than arbitrary voxel destruction.

### J. The interior as the save screen

Show campaign state physically: crossed-out map marks, empty rack spaces, patch shoes on armor,
spent cases, blood/soot, missing tools, a replaced radio, cracked glass, personal photos and orders.
Opening a separate stats screen should reveal less than looking around the Tiger.

### K. False contacts and confirmation

Ice cracks, distant artillery, friendly engines, building collapse, echoing MG fire and exhaust haze
can resemble threats. Contacts remain uncertain until multiple senses or crew reports agree. This
gives long quiet periods meaningful decisions without spawning constant enemies.

### L. Pressure director with real causes

The campaign needs an intensity controller, but it must select from authored dormant platoons,
weather cells, artillery risk, reinforcements and story events rather than teleporting enemies. It
tracks noise, time, route, player damage and cleared zones, then activates plausible pressure and
guaranteed lulls. The director shapes pacing while every threat remains physically explainable.

### M. Mechanical memory

Hard use creates subtle state before outright failure: hot breech, stretched track, declining
battery, fouled optic, weak radio, vibration, small fuel leak and warped hatch. The player can choose
preventive work during a lull or accept compounding risk. Keep this bounded to a handful of legible
conditions; do not turn the Tiger into a spreadsheet.

### N. Crew acknowledgement protocol

An order is not complete when a button is pressed. It must be spoken/transmitted, heard and
acknowledged. Engine noise, a failed intercom, concussion or a missing radio operator delays or
corrupts the handoff. Solo can use the same protocol as a task reminder; co-op turns it into real
crew procedure without a magical shared HUD.

### O. Watch rotation during lulls

Repair, map work, aid and reloading consume hands and attention. During a halt, someone must remain
responsible for listening/observing; if everyone works, the crew gains speed but becomes blind. This
makes quiet time tense and gives commander/radio jobs meaningful co-op work outside combat.

### P. Physical after-action reconstruction

The deterministic combat-event spine can support a bounded post-contact reconstruction at the map
table: bearings, hit times, armor wounds and crew reports. It helps the player learn without giving
them an omniscient in-run killcam or revealing enemies they never detected.

## 6. Production order from this audit

### Now — make one route feel like the finished game

1. Record a reproducible Mac frame-time baseline, then fix Act I cost against an explicit target.
2. Replace fragmented live round/case state with the tested invariant aggregate and prove that every
   round/case has exactly one owner/location through firing, interruption, retry and late join.
3. Define versioned LAN resumable-task state and an atomic, versioned **disk run-save**. The current
   in-memory campaign checkpoint is insufficient for a multi-hour friend-facing game.
4. Build one dense 500×500 m route: forest edge, street block, enterable interiors, wreck/repair site,
   fuel/ammo opportunity, authored casualty scene and 2–3 platoon encounter templates.
5. Add worldbuilder tools required to reproduce that route: density brush, spline road/trench,
   building grammar, encounter sockets and budget overlays.
6. Finish the main-gun firing/reload sensory vertical slice and the shared engine/track signature
   model.
7. Wire the tested doctrine foundation into real movement/perception/smoke and train 6-v-1 pressure
   in that same route. Ten-v-one waits for the hard six-enemy pool and scaling work.

### Next — make attrition and co-op carry a campaign

1. Fuel logistics, salvage and component diagnosis.
2. Resumable physical task graph and co-op handoff.
3. Component breadth: gun/traverse/optic/fan/radio/engine/track.
4. Physical instruments, map/contact notebook and message station.
5. Replace/rig existing LAN crew placeholders; add injury capability and aid foundation.
6. Scale building persistence and destruction across the route.

### Later — expensive systems after the slice proves their need

1. Full organ/limb trauma and body handling.
2. Enemy bailout/recrew and wounded evacuation.
3. Large-scale fire propagation and advanced destruction across whole blocks.
4. Cross-platform GI/radiance caching, richer volumes and offline path-traced reference mode.
5. Texel-splatting world/motion prototype only if the current raster art direction reaches a real
   ceiling after density and materials improve.

## 7. Things not to do yet

- Do not increase default resolution to hide empty maps.
- Do not call the Vulkan direct-light sampler a full path tracer.
- Do not build a per-brick structural simulation.
- Do not give mines an unexplained red LED; use physical clues or an explicit accessibility mode.
- Do not distribute casualties uniformly by a historical average.
- Do not promise medically exact death-to-the-second; use state and evidence-bounded ranges.
- Do not add a generic loot beam, repair currency or omniscient damage HUD.
- Do not let a pressure director spawn threats without authored routes and persistent state.
- Do not spend months on texel splatting before world density, terrain materials and Mac performance
  are fixed.

## 8. Acceptance gates for the next vertical slice

- A still frame at hand/room/street/horizon scale reads as the same finished world.
- The ground never exposes metre-scale source pixels at foot height.
- The player can identify at least three meaningful route choices without HUD markers.
- A full drive→contact→dismount→loot/repair→return loop occurs through authored pressure, not a demo
  command.
- One enemy platoon uses a recognizable fix/flank/smoke plan.
- Every destroyed structure, armor wound and looted container survives retry/advance and late join.
- A two-player crew can hand off an interrupted reload or repair without duplicated state.
- 640×360 hits a recorded, explicit Mac frame-time budget in the slice; 800×450 is judged only after
  the same reproducible benchmark exists.
- Screenshots retain crisp nearest pixels and a readable three-depth composition in all four times of
  day.
- The route is completable at extreme difficulty by information, planning and skill rather than a
  lucky spawn pattern.

## 9. Visual evidence used for this audit

- `artifacts/visual-moodboard-2026-08-11/output/kessel_visual_direction_moodboard_16k.png` — true
  16384×16384 board containing current Kessel frames plus official reference screenshots.
- `artifacts/visual-moodboard-2026-08-11/output/kessel_visual_direction_moodboard_preview_4k.jpg` —
  review-sized copy.
- `artifacts/visual-moodboard-2026-08-11/SOURCE_INDEX.md` — source pages, direct image links, local
  hashes and the specific lesson taken from every external frame. Third-party images are research
  references only and carry no publication/shipping rights.
- `artifacts/resolution-study/` — exact-nearest 640×360, 800×450 and 960×540 visual comparisons. It
  does not yet contain a self-describing performance receipt.
- `artifacts/retro-matrix/retro_resolution_matrix.png` — independent resolution/filter comparison:
  current-resolution clean/standard and 800×450 clean/strong.
- `artifacts/package-smoke-f8496b9/`, `artifacts/modular-building-gallery/`,
  `artifacts/runtime-asset-gallery/` and `artifacts/armor-damage-gallery/` — the current visual truth
  for acts, interiors, destruction, embodied props and persistent armor damage.
