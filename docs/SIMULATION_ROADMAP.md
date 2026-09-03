# Kessel — SIMULATION ROADMAP

> **Superseded engine and crew plan:** this is preserved Unreal-era research, not the current
> production roadmap. Kessel now uses Rust/`wgpu` on Linux/Vulkan; `docs/STATUS.md` and
> `docs/LINUX_4090_HANDOFF.md` own production order, and `docs/DESIGN_PILLARS.md` owns the persistent
> five-man crew. References below to a lone operator, C++, UE milestones, Niagara or Mac performance
> budgets are historical inputs only.

Status: **HISTORICAL DESIGN / RESEARCH** (no C++ compiled this phase — VS/MSVC absent; do not build or launch).
Engine: **UE 5.8.0** at `C:/Program Files/Epic Games/UE_5.8`.

This is the prioritized roadmap of **realism systems** for Kessel, governed by the pillar
**"Deep simulation, abstracted presentation"** (`DESIGN_PILLARS.md §1`). It synthesizes the simulation
research track (`research/11` UE 5.8 capabilities, `research/12` hardcore-sim systems, `research/13`
abstracted-realism taxonomy) and slots every system against the locked build plan (`MILESTONE_1_PLAN.md`).

It does **not** restate the load-bearing specs it extends (`research/03` movement, `04` ballistics,
`06` fog, `07` view modes). Where this roadmap and those specs disagree, **they win** and this doc is
reconciled. Engine features are cited to on-disk truth carried from `research/11`; anything not provable
on disk this phase is tagged **ASSUMPTION**.

---

## 0. How to read this roadmap

### 0.1 The two axes the pillar separates
- **Simulation budget goes deep and wide.** Model the system even when its visual is a few cues.
- **Visual budget goes to fog, light, silhouette, grain — never polycount or PBR.** Every system is
  presented through the cheapest legible cue, and the whiteout denies the rest of the detail.

### 0.2 The three-bucket render taxonomy (from `research/13 §4`, the standing triage)
Every system is tagged with the bucket that governs its *presentation*:
- **A — Simulate deep / render near-nothing.** Footprint is a sound, a number, a light pulse, one
  particle. **Default bucket** — most systems live here.
- **B — Simulate deep / render a stylized cue.** A silhouette, a glow, a reticle, a posterized bloom.
  Impressionistic, never literal. Promote to B only when a stylized cue measurably improves legibility
  or feeling.
- **C — Render with fidelity (the single hero exception).** The interior cabin only — authored high,
  then crushed through `MasterRetro`. No system other than the interior earns Bucket C.

### 0.3 Effort tiers (from `research/12 §0`)
| Tier | Meaning | Rough cost |
|---|---|---|
| **T1** | Trivial — a few fields + one timer/curve; mostly data | hours–1 day |
| **T2** | Moderate — one component + state, reuses existing systems | days |
| **T3** | Heavy — new subsystem, physics/audio integration, tuning loop | 1–2 weeks |
| **T4** | Research-grade — GPU/solver work or large content lift | weeks+ |

### 0.4 Milestone phasing (this roadmap's spine)
- **M1 — Drivable + shooting in fog.** The locked critical path (`MILESTONE_1_PLAN §3`, steps 0–14):
  pawn + Phase-1 scaffold movement, first-person multi-sight view, kinetic ballistics + armor/module
  knockout, volumetric fog atmosphere, enemy stub + win/lose closure, muzzle/blizzard FX, PS1 retro
  dial, Phase-2 custom tracked movement + Linux bring-up. **Plus the cheap-win folds called out in §2.**
- **M2 — The acoustic sensor + the powertrain.** Build the audio spine that Pillar 3 demands and the
  engine/transmission/fuel/overheat model that makes the escape a resource arc. This is where Kessel
  starts to *feel* like the game it wants to be.
- **M3 — Consequence & the lone-crew interior life.** Mobility-kill/thrown-track, ammo-cook-off drama,
  recoil-stroke + fire-control feel, crew thermal/morale orchestrator, hero wreck destruction.
- **M4+ — Long-tail fidelity of systems.** Geometry-collection environmental destruction, snow-rut
  terrain deformation, faux-thermal optic, collision-reading GPU FX, picture-in-periscope, spaced-armor
  /HEAT defeat. Deferred for cost, Beta-risk, or marginal payoff under the whiteout.

### 0.5 Ranking principle
Systems are ordered by **simulation/feeling payoff per unit effort under the pillars**, with two
tie-breakers: (1) does it lie on or trivially adjacent to an already-planned system (cheap fold), and
(2) does it protect a **cross-system coupling** (the deep-sim payoff lives in the couplings, not the
parts — `research/12 §10`).

---

## 1. Master priority table

Ranked by payoff-per-effort. "Fold" = cheap enough to pull into M1 alongside an already-planned step.

| # | System | Bucket | Tier | Milestone | Payoff driver |
|---|---|---|---|---|---|
| 1 | Internal armor + module/crew knockout | A | T2 | **M1** (planned) | The entire combat loop; crew-of-1 lethality |
| 2 | Gun handling: traverse / elevation / reload | A→B | T1–T2 | **M1** (planned) | Pillar 2 made literal; the core loop's bottleneck |
| 3 | Fog-as-spotting + silhouette fade | B | T1 | **M1** (planned) | Pillar 3 mood; nearly free — it's the atmosphere |
| 4 | Physical-surface mobility (snow/ice/mud grip) | A | T2 | **M1 fold** (with step 14) | Cheapest big mobility-realism win; one enum feeds physics+audio+VFX |
| 5 | Ammo cook-off — catastrophic branch | A→B | T1 | **M1** (step 10) | Highest drama-per-line; reuses the wreck light |
| 6 | Fire-control: recoil kick + idle optic shake | B | T1 | **M1 fold** (with steps 6/11) | The satisfying shot; "halt to shoot" tension |
| 7 | Suspension over snow (camera ride) | A→B | T3 | **M1** (step 14, Phase-2) | Makes the tank weigh 57 t; couples to 3 other systems |
| 8 | Acoustic sensor spine (spatial + occlusion + buttoned/open mix) | A | T2–T3 | **M2** | Pillar 3 headline; highest feeling-per-effort in the game |
| 9 | Engine / transmission / fuel + overheat | A | T2 | **M2** | Engine-as-sensor; resource-pressure arc |
| 10 | Mobility-kill / thrown track | A→B | T2 | **M3** | *The* thematic escape failure; movement hook already exists |
| 11 | Ammo cook-off — delayed-detonation dread + stowage modifier | A | T1–T2 | **M3** | Pure feeling; a timer + a light ramp |
| 12 | Recoil-stroke constraint (real gun stroke) | B | T2 | **M3** | High tactile detail down the optic |
| 13 | Fire-control: stadiametric ranging + elevation steps | B | T2 | **M3** | Gunnery skill ceiling at the fog edge |
| 14 | Crew thermal / morale orchestrator (lone operator) | A→B | T2–T3 | **M3** | Highest feeling-payoff after audio; pure abstraction |
| 15 | Hero wreck destruction (Geometry Collection, one tank) | B | T3 | **M4** | The iconic turret-toss; Beta + asset pipeline |
| 16 | Snow-rut terrain deformation (RVT + Niagara grid) | B | T3 | **M4** | High feel, but whiteout makes it near-optional |
| 17 | Collision-reading GPU FX (snow/spall bounce) | B | T2–T3 | **M4** | Polish; makes snow/spall feel physical |
| 18 | Spaced-armor / multi-layer + HEAT defeat | A | T3 | **M4** | Schema reserved now; threat-model depth later |
| 19 | Faux-thermal optic (Custom Stencil + post) | B | T2 | **M4 / optional** | Cool factor — but anachronistic for a 1942 Tiger |
| 20 | Picture-in-periscope SceneCapture sub-display | C-ish | T3 | **M4 / optional** | A true second view; ~2× render cost |
| — | True ray-traced / geometric acoustics | — | T4 | **Descoped** | User's "ideal"; no in-box 5.8 path; whiteout obviates |
| — | Link-by-link simulated tracks / constrained suspension | — | T4 | **Avoid** | Instability/cost; forces already fake it convincingly |
| — | NiagaraFluids gas smoke; photoreal-chasing GPU features | — | — | **Avoid** | Fails the pillar litmus; flat cards read fine in fog |

---

## 2. M1 — cheap wins to FOLD IN, and what stays deferred

M1's locked steps (`MILESTONE_1_PLAN §3`) already deliver systems #1, #2, #3, #5, #7. The roadmap's
job at M1 is to identify **near-free folds** that sharpen the sim for little cost, and to be explicit
about what is **deliberately deferred** so nothing is silently dropped.

### 2.1 FOLD INTO M1 (cheap, high-leverage, ride on already-planned work)
- **#4 Physical-surface mobility (T2).** The moment Phase-2 tracks land (step 14), per-roadwheel traces
  *already exist* for suspension; reading `FHitResult.PhysMaterial` → `EPhysicalSurface` → per-surface
  grip costs almost nothing and is the single largest mobility-realism gain. **Reserve the winter
  surface slots in `PhysicsSettings` NOW** (`Snow_Deep`, `Snow_Packed`, `Ice`, `Mud`, `Frozen_Ground`,
  `Rubble`, `Road`) — the enum is a finite named-slot budget and late churn ripples across
  physics+audio+VFX lookups (`research/11 §14.8`).
- **#6 Recoil reticle kick + idle optic shake (T1).** A curve on the camera's local rotation driven by
  an engine-load scalar + a fired-recoil impulse — no new geometry, no new asset. Folds onto the reticle
  (step 6) and muzzle FX (step 11). It is the punctuation of every 88 mm shot and creates the
  "halt-to-shoot" coupling for free.
- **#5 Cook-off catastrophic branch (T1).** Already essentially in step 10 (player crew-pen → fade-to-
  black; enemy knockout → burning-wreck light). Make the `bCritical` ammo-rack hit explicitly route to
  the catastrophic end-state so the branch is named, not incidental.
- **Reserve-but-don't-wire schema (T0, paper only):** spaced/multi-layer armor as `TArray<FArmorSection>`
  per face with a gap field; `EShellType` HEAT/HE/APCR fields; per-side `TrackHealth`/`bTrackDisabled`
  on the movement component; `Morale`/`Cold`/`Wound` floats on a future crew-state component. Reserving
  the fields now (per `04 §2` precedent) prevents a data-layout churn later for zero M1 cost.

### 2.2 DEFERRED out of M1 (deliberately, in writing)
- **#8 Acoustic sensor spine → M2.** The headline of the user's audio vision, but a real subsystem
  (spatial sources + occlusion channel + submix graph + buttoned/open modulation bus). M1 ships at most
  the **decision-gated thin placeholder pass** (step 13, OD-3): engine loop pitched by speed, gun report,
  impact one-shots. *Even placeholder audio massively sells the loop* — push for OD-3 = (b).
- **#9 Engine/fuel/overheat → M2.** Depends on Phase-2 movement (step 14) existing first.
- **#10 Mobility-kill, #11 cook-off drama, #12 recoil constraint, #13 ranging, #14 crew state → M3.**
  All need their hooks (Phase-2 movement, audio bus, view-state, reload timer) to exist first.
- **All destruction, terrain-deform, thermal, PiP, GPU-collision FX → M4+.** Beta/Experimental,
  asset-pipeline cost, or marginal under the whiteout. Keep a static-wreck fallback at all times.

---

## 3. System cards

Each card: **what it simulates · why it matters for the crew-of-1 blizzard escape · abstracted visual
(the bucket cue) · UE 5.8 feature · effort tier · dependencies/ordering.**

---

### M1 systems (on the locked critical path)

---

#### 1 — Internal armor + module / crew knockout  · Bucket A · T2 · M1 (steps 7–10)
- **Simulates:** multi-layer/sloped armor (effective LOS = nominal/cos θ), overmatch + normalization,
  ricochet-before-thickness, then a fragment/spall cone resolved against discrete modules and crew at
  their real positions (`04`). Not a hit-point bag.
- **Why it matters:** this is the combat spine, and the crew-of-1 twist makes the *interior* dramatic —
  you are M=1, so a single solid crew-position penetration is mission-ending. The Tiger's thin
  side/roof makes *which face you present* to the all-around threat ring a live, constant decision.
- **Abstracted visual:** three stylized hit states — RICOCHET = bright spark streak + hard *spang*;
  NON-PEN = dull flat flash + dust puff; PEN = inward emissive spall spark + red interior flash. One
  sprite each. Internal damage reads as a CRT LED flipping red + the system *behaving* broken — never
  gore, never an x-ray. The armor scheme itself is never seen; it lives in data.
- **UE 5.8 feature:** `UKesselBallisticsSubsystem : UWorldSubsystem` (`WorldSubsystem.h:16`) running
  the `04 §4` pipeline; `UKesselArmorComponent` axis-compare face mapping on the placeholder cube
  (`MILESTONE_1_PLAN` step 8); line/multi-traces (`World.h:2161/:2210`) for the spall cone.
- **Dependencies/ordering:** apply the §5 ballistics calibration fixes (R-B1..R-B3) at step 7. Blocks
  cook-off (#5) and the whole worked-example loop. Foundational; nothing combat-related precedes it.

#### 2 — Gun handling: traverse / elevation / reload  · Bucket A→B · T1–T2 · M1 (steps 5–6, 9)
- **Simulates:** finite turret traverse rate (slow, hydraulic, eventually RPM-coupled), elevation/
  depression limits (−8°..+15°, and later terrain-relative), the breech/reload cycle as a timed
  vulnerability, and (later) turret-ring jam.
- **Why it matters:** Pillar 2 made literal. The commander, gunner, and loader are all *you*; the slow
  traverse forces you to choose between turning the hull to help the turret around (coupling gun↔
  mobility) and the reload is the moment of maximum dread for a solo crew with the threat ring still
  live.
- **Abstracted visual:** the world *slews slowly* past the optic at the capped rate (you feel the
  weight); the reticle simply stops at the elevation limits; reload = a CRT "LOADING/READY" element +
  an audible breech clack-ram-clack in the cabin mix, gun inert until READY. The loader is the sound.
- **UE 5.8 feature:** `UKesselTurretAimComponent` (`FMath::FixedTurn`/`FInterpConstantTo`, rate-capped,
  hull-independent), `UKesselGunComponent` + `FTimerManager` reload, FOV-step zoom on `UCameraComponent`
  (`CameraComponent.h:46`), UMG reticle.
- **Dependencies/ordering:** needs the view component (step 4) and turret/gun scene components (step 5).
  RPM-coupled traverse and ring-jam are T2 polish that wait on #9 (engine state).

#### 3 — Fog-as-spotting + silhouette fade  · Bucket B · T1 · M1 (step 2)
- **Simulates:** detection/recognition as costly, weather-gated steps. The blizzard is a hard
  acquisition limiter *by design* — enemies resolve only as they close; far ones are pure value-on-fog.
- **Why it matters:** Pillar 3 and the entire mood. With no allies ever on screen (Pillar 5), spotting
  is a lonely dread-soaked scan; threats arrive as sound first, then a shape. The fog *is* the spotting
  system, and it makes crude geometry read as atmosphere, not budget (Pillar 1).
- **Abstracted visual:** distance-driven silhouette fade against volumetric fog. No detection meter, no
  outline highlight, no nameplate — the eye does the work; enemy hulls must read by *outline* (gun
  length, turret shape) not texture.
- **UE 5.8 feature:** in-box volumetric fog (`ExponentialHeightFogComponent.h:136`, locked in `06`);
  LOS via the same `LineTraceSingleByChannel` (`World.h:2161`) the enemy stub needs.
- **Dependencies/ordering:** ships in step 2 (atmosphere-first). The *audio half* of spotting is system
  #8 (M2) — this card is only the visual limiter. Tie into OQ-2 (silhouette-identifiability playtest)
  once placeholder hulls exist.

#### 4 — Physical-surface mobility (snow / ice / mud grip)  · Bucket A · T2 · M1 fold (with step 14)
- **Simulates:** per-wheel ground surface → per-surface longitudinal/lateral friction + rolling
  resistance. Deep snow = low grip + high drag (bogging); ice = near-zero lateral grip (the tail slides
  on a slope); mud = grip but drag. The *same* throttle feels completely different by ground.
- **Why it matters:** snow is the medium of the entire game; "heavy, sluggish, low-grip" is the explicit
  movement target (`03`). It makes the escape a terrain-reading problem and couples to gun depression
  (hull attitude) and optic shake. Nearly free because the suspension traces already exist.
- **Abstracted visual:** felt in the handling; one shared surface enum also routes audio (crunch/squeak
  per surface) and VFX (snow vs mud vs sparks spray). No new render.
- **UE 5.8 feature:** `UPhysicalMaterial` + `EPhysicalSurface` (`ChaosEngineInterface.h:20`); slots in
  `PhysicsSettings.h`; read from `FHitResult.PhysMaterial` on the per-roadwheel `LineTraceSingleByChannel`.
- **Dependencies/ordering:** wire the instant Phase-2 tracks (#7) land. **Reserve the winter surface
  slots at M1 start** regardless (finite slot budget). Biggest cheap mobility win in the stack.

#### 5 — Ammo cook-off (catastrophic branch)  · Bucket A→B · T1 · M1 (step 10)
- **Simulates:** a penetrator/jet reaching stowed propellant with residual energy → high-order
  detonation (turret-off catastrophic kill). The most lethal post-penetration event in WW2 armor.
- **Why it matters:** the catastrophic-kill branch already named in `04 §9.4`. For a solo crew it also
  frames every *unfired* round as stowed danger — an implicit "spend ammo / reduce your own bomb"
  tension over the escape. The Tiger stows 88 mm rounds in the side sponsons, so a flank hit is doubly
  punishing (easiest plate + the propellant).
- **Abstracted visual:** a *light*, not a mesh sim — an intense warm interior bloom spilling through the
  apertures, screen-filling flash, then fade-to-black + abort. On the enemy: the burning-wreck emissive
  light (step 10) bleeding red-orange into the snow-fog. First-person-only means you never see your own
  turret leave — you see white, then black.
- **UE 5.8 feature:** `bCritical` ammo-rack hit in the spall cone → pulsed `UPointLightComponent`
  (high `VolumetricScatteringIntensity`, `LightComponentBase.h:120`) + the existing atmosphere/wreck
  light. No new engine system.
- **Dependencies/ordering:** rides on #1 (spall cone) and step 10's wreck light. The *delayed*-detonation
  drama + stowage modifier is split out to M3 (#11).

#### 6 — Fire-control: recoil kick + idle optic shake  · Bucket B · T1 · M1 fold (with steps 6/11)
- **Simulates:** sight picture jitter from the running engine (settles when halted), a hard recoil punch
  on firing that decays over ~0.3–0.5 s, and parallax (gun vs sight not coaxial — falls out of geometry).
- **Why it matters:** optic shake makes you *want to halt to shoot* (safer shot, longer exposure —
  couples to mobility/engine), and the recoil reticle kick is the satisfying punctuation of every shot,
  paired with the muzzle-bloom-into-fog.
- **Abstracted visual:** camera/reticle micro-jitter from an engine-load scalar + a fired-recoil impulse
  curve; dead-steady at idle/halt. Parallax is modeled-but-tiny (correct, invisible) — the deep-sim/
  abstracted sweet spot.
- **UE 5.8 feature:** drive `UCameraComponent` local rotation + `PostProcessSettings` from scalars;
  parallax = the offset between `MuzzlePoint` and `GunnerEye` child scene components (already locked).
- **Dependencies/ordering:** reticle exists at step 6; recoil ties to firing (step 9) and muzzle FX
  (step 11). Idle-shake's engine-load input is a stub until #9; ship a constant-amplitude version first.

#### 7 — Suspension over snow (camera ride)  · Bucket A→B · T3 · M1 (step 14, Phase-2)
- **Simulates:** per-roadwheel spring+damper following contours, hull pitch/roll over obstacles, heavy
  high-momentum mass that "does not bounce," traction loss on slopes, bogging in deep snow. Feeds gun
  depression (hull attitude) and ride-induced optic shake.
- **Why it matters:** makes the tank weigh 57 t rather than hover, and this one system couples to
  gunnery, fire control, and mobility-kill at once — exactly the deep-systems interlock the pillars
  reward. It physically generates the idle/drive jitter that #6 turns into the halt-to-shoot decision.
- **Abstracted visual:** the **camera ride** — the first-person view pitches/rolls/jitters with the hull,
  snow streaks across the optic, spindrift kicks up. You feel the terrain through the eyepiece with no
  track-link or terrain-deform mesh.
- **UE 5.8 feature:** the custom `UKesselTrackedMovementComponent` core (`03 §3`): per-roadwheel
  `LineTraceSingleByChannel` → spring/damper → `AddForceAtLocation` (`PrimitiveComponent.h:1770`) on the
  `HullMesh` rigid body, with physics **substepping** (`PhysicsSettings.h:325/344`) for stable forces on
  the heavy mass. Built on the **stable Chaos rigid-body core**, not the experimental car solver.
- **Dependencies/ordering:** the biggest movement lift; it *is* step 14. Replaces the Phase-1 scaffold.
  Enables #4 (surface friction layers on top), #9 (engine torque feeds tractive force), #10 (per-side
  track disable). **Verify substep stability on the 57 t hull once the editor is available** (open item).

---

### M2 systems (the acoustic sensor + the powertrain)

---

#### 8 — Acoustic sensor spine: spatial + occlusion + buttoned/open mix  · Bucket A · T2–T3 · M2
- **Simulates:** spatial acoustic propagation — direction (a shell impact left of the tank bangs in the
  left ear), distance attenuation + air absorption, snow muffling, and **hull/terrain occlusion** (a
  single listener→source trace LPF+volume-cut). The buttoned-up (muffled interior) vs head-out (open-air)
  state changes the entire mix, tied to the view-mode state machine.
- **Why it matters:** Pillar 3 — *sound is the primary sensor*. In a whiteout you hear the war before you
  see it; this is the Silent Hunter model (bearing/class/range from sound). It is the single highest
  feeling-per-effort system in the game and the headline of the user's audio vision. Buttoned/head-out
  makes *which sight you choose* a trade of visibility for acoustic information.
- **Abstracted visual:** none — it is the cue channel, not a render. Threat bearing/class/range live
  entirely in spatialized, filtered audio + the growing silhouette.
- **UE 5.8 feature (all in-box, no recompile):** MetaSound sources (`Metasound.uplugin`, on by default)
  driven by sim params (RPM, track speed, reload steps, layered gun report) so placeholder samples swap
  to the user's real SFX with no graph change; Sound Attenuation panning + `bEnableOcclusion` on a
  dedicated `AudioOcclusion` trace channel (`SoundAttenuation.h:174/234`); a submix graph
  `SM_Interior` (LPF + small/convolution reverb + low-mid EQ) vs `SM_OpenAir`, fed by attenuation
  submix-sends; a single **`Buttonedness` Audio Modulation control bus** (`AudioModulation`, v2.0, **OFF
  BY DEFAULT — must enable in the .uproject**) written by `UKesselViewComponent`'s `EViewMode` machine,
  driving interior LPF cutoff / reverb wet / open-bed volume with smooth interp on hatch transitions.
  HRTF via the in-box Spatialization plugin as an optional headphone toggle. Defer `AudioGameplayVolume`,
  AudioMotorSim, and Quartz choreography to first-pass-plus.
- **Dependencies/ordering:** **enable AudioModulation in the .uproject alongside EnhancedInput/Niagara/
  ChaosVehiclesPlugin** or the buttoned/open rig silently won't load (risk from `research/10`). Needs the
  view-mode state machine (M1 step 6) and the projectile/impact events (M1 step 9) as inputs. This system
  completes the *audio half* of spotting (#3) and is the precondition for the crew-state mix ducking
  (#14). **Owns the cue vocabulary** — finalize `AUDIO_ARCHITECTURE.md` (referenced by the pillars, not
  yet on disk) as part of this work; real SFX are placeholder until the user records, so the spang-vs-
  bang/diesel-cadence/LED-color table can only be drafted now (OQ-1).
- **Reject / descope:** the in-box Steam Audio is literally "Steam Audio (Deprecated)" (Win64-only, no
  Linux binary — breaks parity); Project Acoustics is discontinued (no 5.8, static bake can't include the
  moving hull). True ray-traced/geometric acoustics (the user's "ideal") has **no in-box 5.8 path** —
  T4/ASSUMPTION; the occlusion-trace approximation is an accepted ceiling under the enclosed-interior +
  whiteout premise. Revisit Valve Steam Audio 4.x only post-M2 with an MSVC recompile budget + Linux
  validation, and only if exterior reflection/diffraction becomes a genuine pillar payoff.

#### 9 — Engine / transmission / fuel + overheat  · Bucket A · T2 · M2
- **Simulates:** a torque/RPM curve feeding the transmission, fuel as a finite consumable *and* a fire
  hazard, and thermal state — overheat from sustained load, cold-start difficulty, power sag, stall.
  Engine and transmission are separately killable (engine dead → coast; transmission dead → runs but no
  drive).
- **Why it matters:** General Winter is the antagonist — a Maybach HL230 in a Stalingrad blizzard is a
  cold-start, overheat, fuel-anxiety machine, and the escape is literally "keep this thing running long
  enough to get out." For a solo crew the engine is the **primary sensor companion**: its note *is* the
  dashboard you hear, so its health is conveyed almost entirely through sound (Pillar 3). Fuel turns the
  run into a resource arc; overheat punishes flooring it through deep snow (couples to #7).
- **Abstracted visual:** almost entirely audio + instrument. Engine state = the loop's pitch/timbre
  (idle rumble → strained high-load → stutter/stall); a temp bar climbing into red; a fuel gauge; one
  Niagara exhaust plume that thickens with load and coughs on a cold start. A deep powertrain model
  presented as *two gauges, one engine sound, one smoke plume*.
- **UE 5.8 feature:** engine/transmission *feel* mirrors the `FVehicleEngineConfig`/`FVehicleTransmissionConfig`
  *shape* (`ChaosWheeledVehicleMovementComponent.h:223-258,312`) as a data-tunable struct inside the
  custom layer — **not** by running the car solver. Thermal/fuel are plain `float` state gating max
  tractive force. Engine-note-as-sensor: **AudioMotorSim** is in-box and purpose-built for RPM/load →
  MetaSound (ASSUMPTION on exact 5.8 API until MSVC; recommend driving a MetaSound engine layer from our
  own diesel RPM curve for the first pass rather than the experimental car-gearbox-shaped AudioMotorSim).
- **Dependencies/ordering:** needs Phase-2 movement (#7) for the RPM/load signal. Feeds RPM-coupled
  traverse (#2 polish), optic-shake amplitude (#6), and the engine half of mobility-kill (#10). Its hits
  reuse the armor→module path (#1).

---

### M3 systems (consequence & lone-crew interior life)

---

#### 10 — Mobility-kill / thrown track  · Bucket A→B · T2 · M3
- **Simulates:** a running-gear hit that immobilizes without killing (M-kill); throwing a track (slews/
  locks to one side); single-track drive (pivot toward the dead side, dragging); field-repair over time.
- **Why it matters:** for an *escape* game this is arguably *the* most thematically loaded failure state
  — not death, but **stranded inside the Kessel**, which for survival-horror is worse. A thrown track on
  a crew-of-1 means *you* must diegetically dismount to repair under fire in a blizzard — a natural
  high-tension set-piece.
- **Abstracted visual:** felt through the controls (the tank yaws hard toward the dead side, won't drive
  straight) + one cue: a track-warning panel element red + a grinding/snapping sound hard-panned to the
  damaged side. Repair = a timed, exposed, diegetic state (hatch state + progress beat + vulnerability),
  not an animation set.
- **UE 5.8 feature:** per-side `bTrackDisabled`/`TrackHealth` in `UKesselTrackedMovementComponent` zeroes
  or clamps that side's tractive force and raises lateral drag asymmetrically → authentic drag-to-one-
  side, all on `AddForceAtLocation`. The hit reuses the armor→module path (`Tracks` module). Repair =
  `FTimerManager` + the view-mode state machine. Buildable on the **stable rigid-body core**.
- **Dependencies/ordering:** the movement hook exists once #7 lands; the `Tracks` module exists in #1.
  Couples directly to the neutral-steer model (lose left track → `vL`→0). Days of wiring; signature
  "escape" failure.

#### 11 — Ammo cook-off: delayed detonation + stowage modifier  · Bucket A · T1–T2 · M3
- **Simulates:** propellant deflagration (fast internal fire) distinct from high-order detonation;
  **delayed** cook-off (fire starts, reaches the racks seconds later); ammo state scaled by how many
  rounds remain stowed (full rack = bomb; near-empty often just burns).
- **Why it matters:** the delayed beat is pure dread — a horrible half-second where you know it's coming.
  The stowage modifier gives the "spend ammo / reduce your bomb" tension a concrete number.
- **Abstracted visual:** a rising internal roar + smoke haze + a warm light ramp for a beat before the
  rack goes; two-stage emissive event reusing the atmosphere stack. No turret-toss ragdoll.
- **UE 5.8 feature:** `FTimerManager` schedules the delayed-detonation beat; a `bCritical` module hit
  rolls cook-off vs deflagration weighted by rounds-remaining; pulsed point light + Niagara smoke
  `ULocalFogVolumeComponent` plume. No new engine system.
- **Dependencies/ordering:** extends #5 (catastrophic branch must exist). A few fields + a timer.

#### 12 — Recoil-stroke constraint (real gun stroke)  · Bucket B · T2 · M3
- **Simulates:** a prismatic recoil stroke-and-return on the gun barrel (breech slams back, returns on
  the recuperator) instead of a faked timeline — a strong tactile detail seen down the gunner's optic.
- **Why it matters:** the most satisfying mechanical detail in the gunnery loop; sells the weight of the
  88. Pairs with the recoil reticle kick (#6) and muzzle bloom.
- **Abstracted visual:** the barrel physically strokes back and returns; felt in the optic and the cabin.
- **UE 5.8 feature:** a prismatic/linear-drive `UPhysicsConstraintComponent` (`ConstraintDrives.h`,
  stable core) on the gun body. Hatch/cupola hinges are an optional companion (low priority).
- **Dependencies/ordering:** needs the firing event (#2/M1 step 9). Migrate the StaticMesh gun visual to
  a constrained body — note this is the first place M1's "no skeletal/no constraint" simplification is
  relaxed; keep it isolated to the gun, **not** the tracks/suspension (those stay force-faked).

#### 13 — Fire-control: stadiametric ranging + elevation steps  · Bucket B · T2 · M3
- **Simulates:** WW2 manual fire-control — estimate range off the reticle's stadia/mil marks against a
  known target size, set sight elevation for the round's drop (zeroing), no laser rangefinder.
- **Why it matters:** the gunnery skill ceiling. At fog ranges (40–80 m) the flat 8.8 cm trajectory means
  you mostly point-and-shoot, but manual ranging gives an expert player a real edge at the fog's outer
  edge where a silhouette is barely there.
- **Abstracted visual:** CRT-green stadia ticks (`#33FF66`) on the reticle the player matches to a
  target's width; an optional amber range readout; elevation as a steppable reticle aim-point shift.
- **UE 5.8 feature:** UMG `WBP_GunnerReticle` + player skill (zero engine cost); drop is the `04 §3`
  `v(r)` model already specced.
- **Dependencies/ordering:** extends the M1 reticle (step 6). Pairs with OQ-3 (minimum legible
  instrument set). Full coincidence-rangefinder UI is T3 and low-priority given fog ranges.

#### 14 — Crew thermal / morale orchestrator (lone operator)  · Bucket A→B · T2–T3 · M3
- **Simulates:** the *human* layer collapsed onto one person — suppression/morale (a near-miss or non-pen
  shakes you → slower hands, longer reload, de-directionalized hearing), and cold/thermal stress (numb
  hands, frosted optics, stiffer traverse, the cabin warmth mattering). Plus the `04 §9.4` bleed-out
  window for a solid-but-not-instant crew wound.
- **Why it matters:** Kessel collapses the whole crew into one person under sustained pressure (Pillar 2),
  so morale/thermal is the **player-character's own state** and the most direct lever on the survival-
  horror feeling. It gives buttoned-vs-head-out a *psychological* weight (head-out is colder, more
  exposed, but sees and hears more; buttoned is warmer, safer, half-blind and half-deaf).
- **Abstracted visual:** entirely post-process + audio + instrument. Shaken = optic-shake spike +
  desaturation/vignette pulse + a low-pass/ringing on the spatial mix that recovers over seconds. Cold =
  periscope-glass frost accumulation + breath fog + a slow blue grade creep + stiffer traverse. Wounded =
  red vignette + heartbeat in the mix + degraded hands. Zero new geometry.
- **UE 5.8 feature:** a thin `UKesselCrewStateComponent` holding `Morale`/`Cold`/`Wound` floats that
  *modulate existing systems* — camera `PostProcessSettings` (`CameraComponent.h:353`) for vignette/
  desat/frost; an `AudioModulation` low-pass/duck on the spatial mix; the reload/traverse rates on the
  gun/turret components; periscope frost via a `UMaterialInstanceDynamic` param on the optic mask. A thin
  orchestrator, no new engine feature.
- **Dependencies/ordering:** the **highest feeling-payoff system after audio**, but every hook must exist
  first — PostProcess (M1), the audio modulation bus (#8/M2), view-state (M1), reload/traverse timers
  (M1). That is why it is an *early M3 track*, not M1. The morale/cold/bleed-out numbers are entirely
  ASSUMPTION/tuning targets until an in-editor tuning loop is possible.

---

### M4+ systems (long-tail fidelity of systems)

---

#### 15 — Hero wreck destruction (one tank)  · Bucket B · T3 · M4
- **Simulates:** a destructible enemy turret/hull that fractures into clustered pieces on catastrophic
  kill (the iconic ammo-cook-off turret-toss), with per-cluster damage thresholds.
- **Why it matters:** a strong early showcase of consequence and weight; the chunky low-poly fracture is
  on-style and the whiteout hides it fast.
- **Abstracted visual:** one hero wreck fractures + a Chaos→Niagara dust/spark burst from the actual
  break events; cooling-wreck glow.
- **UE 5.8 feature:** Geometry Collection (`GeometryCollectionPlugin`, **Beta + EnabledByDefault:false**),
  optionally a `Field_ExternalClusterStrain` field at the impact (`FieldSystemTypes.h:151`, Beta), and
  `NiagaraDataInterfaceChaosDestruction` for break-driven FX. `ChaosCaching` can bake a hero collapse for
  cheap replay.
- **Dependencies/ordering:** **off the critical path; keep the static-wreck + smoke fallback at all
  times.** Beta + must-enable + an in-editor fracture-authoring asset pipeline. Verify runtime stability
  and Linux/Vulkan parity before committing (open item). Pairs with #11 (cook-off triggers the toss).

#### 16 — Snow-rut terrain deformation  · Bucket B · T3 · M4
- **Simulates:** persistent track ruts / compressed-snow trails written under the tank.
- **Why it matters:** high-feel winter detail — but the whiteout makes it near-optional; a darkened
  decal trail + light WPO dip reads as ruts without a deformation sim.
- **Abstracted visual:** a compressed-snow darkening trail + slight ground displacement behind the tank.
- **UE 5.8 feature:** Runtime Virtual Texture (`RuntimeVirtualTexture.h`, stable) as the persistent
  displacement/trail mask the ground material samples (WPO + albedo darkening), with a Niagara render-
  target/grid DI as the GPU writer (Beta). VirtualHeightfieldMesh/Landmass are Experimental — avoid.
- **Dependencies/ordering:** needs Phase-2 movement (#7) for the writer position. Medium effort, medium-
  high feel, but explicitly low priority under the abstraction premise.

#### 17 — Collision-reading GPU FX (snow / spall bounce)  · Bucket B · T2–T3 · M4
- **Simulates:** particles that trace against the scene — snow/spall bounce off the hull and ground,
  snow accretes on or slides off the moving body, track spray follows the surface contour.
- **Why it matters:** makes snow and spall feel *physical* rather than decorative — a polish multiplier.
- **Abstracted visual:** collision-aware spall cones, snow spray, embers colliding with geometry.
- **UE 5.8 feature:** Niagara GPU sims with `NiagaraDataInterfaceCollisionQuery.h`,
  `NiagaraDataInterfaceRigidMeshCollisionQuery.h`, `NiagaraDataInterfaceLandscape.h` (Niagara is
  stable/on; these DIs are the post-M1 polish layer). On the 4090, high-count GPU sims are affordable.
- **Dependencies/ordering:** the standard (non-colliding) emitters ship in M1 (step 11); this upgrades
  them. No hard dependency beyond the FX existing.

#### 18 — Spaced-armor / multi-layer + HEAT defeat  · Bucket A · T3 · M4
- **Simulates:** an ordered list of plates with an air gap that strips a HEAT jet's penetration by
  standoff and bleeds kinetic energy per layer; schürzen/side-skirts as a threat-model option.
- **Why it matters:** deepens the *threat* model (enemy HEAT/ATR) once the kinetic core is solid; mostly
  a Tiger-as-target concern.
- **Abstracted visual:** same three hit states as #1; the layering is invisible (data).
- **UE 5.8 feature:** `TArray<FArmorSection>` per face with a gap field, re-running the pen step per
  layer (`04 §2` schema, **reserved now, unwired**). No engine feature beyond #1.
- **Dependencies/ordering:** schema reserved at M1; wiring waits until HEAT/HE shells are added
  (`EShellType` reserved). Pure extension of #1.

#### 19 — Faux-thermal optic (Custom Stencil + post)  · Bucket B · T2 · M4 / optional
- **Simulates:** *presentation* of simulated heat state (engine deck, recently-fired barrel, crewed
  enemy tanks) — NOT a physical IR sim (none exists in 5.8).
- **Why it matters:** cool factor and a real legibility aid in fog — **but historically anachronistic for
  a 1942 Tiger.** Offer as an explicit stylistic option, never a realism claim (Pillar 6).
- **Abstracted visual:** a thermal LUT post material reading a per-object heat scalar other systems
  already compute (engine load #9, rounds fired #2).
- **UE 5.8 feature:** Custom Depth/Stencil (`PrimitiveComponent.h:712/861`) + a post-process material,
  optionally over SceneCapture depth/normal sources. Systems-cheap, on-style.
- **Dependencies/ordering:** needs the heat scalars from #9/#2. Flag historically; user decision required
  before building.

#### 20 — Picture-in-periscope SceneCapture sub-display  · Bucket C-ish · T3 · M4 / optional
- **Simulates:** a genuine second magnified view inside the commander's periscope / a rear episcope that
  a post-process pass cannot fake.
- **Why it matters:** a true second view for the multi-sight fantasy — but it adds a full second scene
  render (~2× cost), so it must be measured and kept off the default optic path.
- **Abstracted visual:** a small magnified inset in the periscope mask.
- **UE 5.8 feature:** `USceneCaptureComponent2D` → render target (stable core). ARCHITECTURE §5 **rejects**
  this as the *default* optic; it is justified only as an explicit, optional second render.
- **Dependencies/ordering:** measure the cost before shipping. Opt-in only.

---

### Descoped / Avoid (explicit, so nothing is silently reconsidered)

- **True ray-traced / geometric acoustics (T4, descoped).** The user's stated "ideal," but **no in-box
  UE 5.8 ray-traced audio engine ships** (Steam Audio in-box is deprecated/Win64-only; Project Acoustics
  is discontinued, no 5.8, static bake can't include the moving hull). Approximate with occlusion traces
  + attenuation + reverb zones + HRTF (#8). The enclosed-interior + whiteout premise makes geometric
  acoustics unnecessary; revisit Valve Steam Audio 4.x only post-M2 with an MSVC recompile + Linux
  validation budget. ASSUMPTION until MSVC lands.
- **Link-by-link simulated tracks / constrained-body suspension (T4, avoid).** Huge solver cost +
  instability on a 57 t hull; the design correctly fakes track *behavior* with per-side forces (#7/#10).
  Constraints are for the gun (#12) and hatches only.
- **NiagaraFluids gas smoke (avoid for now).** Beta + heavy; flat scrolling smoke cards into the fog read
  fine under abstraction. Revisit a single hero fluid-smoke column only if budget allows, post-M3.
- **Packaged Chaos vehicle modules as shipping movement (avoid).** Experimental + wheeled-only (no
  neutral-steer); ChaosVehiclesPlugin is throwaway Phase-1 scaffolding only — its API must not leak into
  shipping movement code.
- **Any photoreal-chasing GPU feature (avoid, by pillar litmus).** Photoreal track-link rendering, high-
  poly spall fans, PBR mud/wetness on the exterior, lens-accurate film grain. Simulate the *effect* in
  Bucket A; skip the render (guardrail G8, `research/13 §7b`).

---

## 4. Cross-system couplings to protect

The deep-sim payoff lives in the **couplings**, not the individual systems (`research/12 §10`). Build
each system so these emerge from shared signals rather than scripted outcomes:

- **Engine RPM → traverse rate** (#9 → #2): the hydraulic turret slews slower when the engine is starved.
- **Hull attitude → gun depression + optic shake** (#7 → #2, #6): cresting a snow ridge exposes your thin
  roof while the gun can't depress onto a close threat.
- **Track/engine kill → stranded in the Kessel** (#10, #9): the signature escape failure, not death.
- **Buttoned vs head-out → spotting + audio + thermal** (#3, #8, #14): one view-state choice trades
  visibility, acoustic information, and warmth/exposure simultaneously.
- **A side hit is the easiest plate AND the ammo stowage AND the fuel** (#1, #5/#11, #9): one geometric
  fact, three coupled consequences.
- **One ground-surface enum feeds physics + audio + VFX** (#4 → #8, FX): compute the surface once,
  present it three ways.

These couplings are the argument for spending simulation budget once and presenting it many ways —
exactly "deep simulation, abstracted presentation."

---

## 5. The standing triage rule (apply to every future feature)

From `research/13 §4`, into the per-build review checklist:
1. **Litmus (G1):** *"Does this deepen the simulation or the feeling?"* If the only answer is "it looks
   more real," reject or re-scope to a system.
2. **Default to Bucket A (G2):** a new system renders *nothing* until a stylized cue is justified. Ask
   "what is the cheapest legible cue?" before approving any new mesh/material.
3. **Author high, display crushed (G3):** surface art must pass through `MasterRetro`; nothing ships
   that only looks right with the dial at 0.
4. **The concealment stack is non-removable (G4):** volumetric fog + emissive scatter + CRT-green stay
   ON at every dial position. Not a graphics option.
5. **One fact, one primary channel (cue discipline):** bearing is audio-primary; damage is instrument-
   primary. Cues form a learnable vocabulary; never reuse a cue for two unrelated facts.
6. **Prefer capability-change over annotation:** the truest damage cue is what the machine can no longer
   *do* (reload timer stops, turret won't slew) — but pair it with an immediate confirming cue (bang +
   red flash) so it reads as a hit, not a bug.
7. **Screenshot drift test (G9/§7c):** if an in-game still could be mistaken for a modern milsim
   screenshot, the abstraction has failed regardless of simulation depth.

---

## 6. Open decisions

Roadmap-level decisions for the user. (The M1-specific OD-1..OD-8 live in `MILESTONE_1_PLAN §6`; these
extend them across milestones.)

- **SR-OD-1 — Audio in M1 vs M2.** `MILESTONE_1_PLAN` OD-3 asks whether to add the thin placeholder audio
  pass (step 13) in M1. The roadmap recommends **yes (OD-3=b)**: even placeholder engine/gun/impact audio
  massively sells the survival-horror loop, and it de-risks the M2 acoustic spine by exercising MetaSound
  + the .uproject AudioModulation enable early. Confirm.
- **SR-OD-2 — `AUDIO_ARCHITECTURE.md` authoring timing.** The pillars (§3), `research/12`, and `research/13`
  all reference it; it does not exist on disk. It owns the cue vocabulary (spang-vs-bang, diesel cadences,
  LED colors) and the buttoned/open mix spec. Draft it alongside M1's placeholder pass (vocabulary stays
  provisional until the user records real SFX), or defer wholesale to M2? Recommend: **draft now, finalize
  in M2.**
- **SR-OD-3 — Milestone boundary for the acoustic sensor (#8).** It is the highest feeling-per-effort
  system but a real subsystem. Is "M2, after M1's drivable+shooting loop is proven" the right boundary, or
  should the spatial L/R bang + buttoned/open mix be pulled *into* M1 (raising M1 scope materially)?
- **SR-OD-4 — Crew thermal/morale (#14) ambition.** Full coherent crew-state model (what moves morale/
  cold, recovery rates, bleed-out arc, the buttoned/head-out thermal trade) vs a minimal "shaken =
  shake+muffle, cold = frost overlay" subset? All numbers are ASSUMPTION until an in-editor tuning loop.
- **SR-OD-5 — Faux-thermal optic (#19) at all?** It is a cool legibility aid but **anachronistic for a
  1942 Tiger** (Pillar 6). Build it as an explicit out-of-fiction stylistic option, or cut it to protect
  historical grounding? Default: **cut / hold** unless the user wants it.
- **SR-OD-6 — Destruction ambition ceiling (#15, #16).** Both are Beta + asset-pipeline + Linux-parity
  unknowns. Commit to one hero wreck-on-kill at M4, or hold all Geometry-Collection/terrain-deform work
  until the editor can validate runtime stability and Vulkan parity? Default: **one hero wreck, static
  fallback always present.**
- **SR-OD-7 — Recoil-stroke constraint (#12) vs faked timeline.** #12 relaxes M1's "no constraints"
  simplification for the gun barrel only. Worth the constrained-body migration for the tactile payoff, or
  keep a `USceneComponent` transform timeline (cheaper, less authentic)?
- **SR-OD-8 — Surface-slot reservation set.** Confirm the winter `EPhysicalSurface` set to reserve at M1
  start (`Snow_Deep`, `Snow_Packed`, `Ice`, `Mud`, `Frozen_Ground`, `Rubble`, `Road`) — the enum is a
  finite named-slot budget and late additions churn physics+audio+VFX lookups.

---

## 7. Verification items (resolve once VS/MSVC + editor are available)

Carried from the research track; all paper until the editor can launch:
1. **Substep stability** on the 57 t hull under per-substep track forces (#7); async-physics determinism.
2. **AudioModulation .uproject enable** behaves (the buttoned/open rig loads) — and the broader
   ASSUMPTION that enabling precompiled in-box plugins is a non-issue without an editor launch (#8).
3. **AudioMotorSim 5.8 API surface** (#9) — present on disk, exact class/property names unverified.
4. **Geometry Collection / Field / ChaosNiagara runtime stability + asset-pipeline cost + Linux/Vulkan
   parity** at the min-spec fog tier (#15) before committing destruction.
5. **SceneCapture second-render cost** (#20) and faux-thermal cost (#19) — measure before shipping.
6. **No native IR/thermal** confirmed by absence on disk — any thermal feature is explicitly a
   compositing trick, never a radiometric-realism claim (#19, Pillar 6).
7. **Linux parity** for all Beta GPU destruction/Niagara sims — confirm on the Vulkan path.
8. **Ballistics calibration** (R-B1..R-B3, `MILESTONE_1_PLAN §5`) before #1 ships.

---

## 8. Sources / grounding
- **Pillars & locked plans (primary):** `DESIGN_PILLARS.md`, `MILESTONE_1_PLAN.md`, `ARCHITECTURE.md`,
  `ART_BIBLE.md`.
- **Simulation research track (this roadmap synthesizes these):** `research/11-sim-ue58-capabilities.md`
  (on-disk UE 5.8 feature truth), `research/12-sim-tank-realism-systems.md` (hardcore-sim system cards +
  ranking), `research/13-sim-abstracted-realism.md` (three-bucket taxonomy + guardrails).
- **Audio research track (feeds #8):** `research/09-audio-project-acoustics.md` (reject Project
  Acoustics), `research/10-audio-native-ue.md` (the all-native in-box audio spine).
- **Extended system specs (win on conflict):** `research/03-tracked-vehicle.md`, `research/04-ballistics-
  armor.md`, `research/06-volumetric-fog-scatter.md`, `research/07-view-modes-controls.md`.
- **Planned sibling (owns the cue vocabulary, not yet on disk):** `AUDIO_ARCHITECTURE.md` (SR-OD-2).
