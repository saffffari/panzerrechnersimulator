# Kessel — AUDIO ARCHITECTURE

> **Historical Unreal-era architecture.** The engine paths and MetaSounds/C++ implementation plan
> below are superseded. Preserve its sensory and acoustics principles, but use
> `docs/TANK_ACOUSTICS.md` and `docs/LINUX_4090_HANDOFF.md` for the current Rust/`wgpu` build.

Status: **DESIGN LOCKED (paper only)** — no C++ authored, no editor launch, no build this phase
(VS/MSVC absent). Engine: **UE 5.8.0** at `C:/Program Files/Epic Games/UE_5.8`. Every plugin/feature
is confirmed on disk with a cited path; anything not provable on disk is tagged **ASSUMPTION**.

Owns: **Pillar 3 — "Sound is the primary sensor."** This is the document `DESIGN_PILLARS.md §3` and the
research bridge docs (12, 13) forward-reference. It consolidates research `09-audio-project-acoustics.md`,
`10-audio-native-ue.md`, and the audio threads of `12`/`13` into one implementable plan.

Governing pillar: **DEEP SIMULATION, ABSTRACTED PRESENTATION.** The audio *systems* carry maximal realism
— procedural sources driven by the sim, spatial direction, hull/terrain occlusion, snow medium, buttoned
vs open mixing — while the *assets* stay placeholder in M1 (the user records real SFX later and swaps them
in with zero graph change). In a whiteout the ear leads the eye: a growing diesel note in the right channel
**is** a flanking enemy without rendering the enemy (the apophenia argument of `13`).

---

## 0. TL;DR — the recommended stack

| Layer | Decision (M1) | Why |
|---|---|---|
| **Spatial engine** | **Native UE 5.8 only.** Reject Project Acoustics; **defer** Steam Audio to post-M1. | §1 |
| **Sources** | **MetaSounds**, every source parameterized by the sim (RPM, TrackSpeed, reload step, report layers). Placeholder samples now, user SFX later. | §2 |
| **Direction** | **Attenuation panning** for the L/R bang + `bEnableOcclusion` on a dedicated `AudioOcclusion` trace channel; HRTF via the in-box `Spatialization` plugin as an optional headphone toggle. | §3 |
| **Space/muffle** | **Submix graph**: `SM_Interior` (LPF + small reverb + low-mid EQ) vs `SM_OpenAir` (light tail), fed by attenuation submix-sends. | §4 |
| **State (buttoned↔open)** | **Audio Modulation** control bus `Buttonedness`, written by `UKesselViewComponent`'s `EViewMode` machine, smoothly crossfades the interior/open chains. | §5 |
| **Snow/blizzard** | **One `BlizzardIntensity` scalar** (shared with the fog/`UAtmospherePreset`) → per-source distance×weather LPF + gain, plus master air-absorption. | §6 |
| **Timing** | **Quartz** for sample-accurate reload/clank/report — high payoff, **low M1 priority** (frame timers suffice for the placeholder pass). | §7 |
| **Glue** | **`UKesselAudioComponent`** on the pawn: the single seam the view/movement/gun/armor systems call into. | §9 |

**Enable in `.uproject` (beyond engine defaults):** `AudioModulation` (off by default — the buttoned/open
rig silently fails to load if forgotten). `Metasound` and `Synthesis` are already default-on. Everything
else (HRTF, AudioGameplayVolume, Quartz polish, any third-party engine) is optional/deferred.

This delivers the user's **entire verbatim audio vision** — directional bang ("a shell impact left of the
tank bangs in the left ear"), snow muffle, buttoned-vs-head-out mixing tied to the view-mode state machine,
plus reload/engine/track/collision/whistle sources — using only in-box, mostly precompiled features, with
**no dependency on the absent MSVC toolchain and no Linux-parity risk.**

---

## 1. The spatial-engine decision (native vs Steam Audio vs Project Acoustics)

The user's stated ideal is verbatim: *"Ray-traced audio / acoustic simulation engine would be ideal."*
That aspiration buys, over native, geometry-driven **diffraction + reflections + dynamically computed
reverb**. Weighed against this game's three hard facts — a **moving hull** as the dominant occluder, a
**Chaos-destructible** world, a **blizzard whiteout** with short draw distance and a solo-dev iteration
loop — the verdict is:

### REJECT — Microsoft Project Acoustics (research `09`)
Three independent, each-fatal reasons:
1. **Discontinued, no 5.8 path.** Repo archived 2024-07-01; last release (2022.1) targets UE 5.0/5.1;
   downloads pulled from MS Download Center + Marketplace. Reaching 5.8 means recompiling an abandoned
   plugin across 7 minor versions — and MSVC is absent this phase, so it cannot even be attempted.
2. **Static-geometry bake is the inverse of our game.** It bakes a wave field over *static* meshes
   offline; the **tank hull moves with the listener** and the world is destructible — the most important
   occluder (the steel box around the player's head) is excluded from the solve by construction.
3. **It cannot produce the marquee feature.** Buttoned-vs-hatch is a *dynamic, listener-local enclosure*
   change; the bake gives identical world propagation in both states, so the entire buttoned/open
   difference must be synthesized **outside** Project Acoustics anyway (a submix LPF/reverb swap keyed to
   the view state — exactly what we build natively). It also costs a cloud/Docker re-bake on every level
   edit. Pay-all-cost-for-the-smallest-part-of-our-problem.

### DEFER — Valve Steam Audio (the real "ray-traced" option)
- The **in-box** Steam Audio is a dead-end: `Engine/Plugins/Runtime/Steam/SteamAudio/SteamAudio.uplugin`
  is literally `"FriendlyName":"Steam Audio (Deprecated)"`, `v2.0-beta.17`, off by default, **Win64-only
  binary (no Linux `.so` on disk)**. Adopting it now is technical debt against the Linux parity target
  (`ARCHITECTURE §12`).
- The **current Valve Steam Audio 4.x** (free, Apache-2.0, cross-platform incl. Linux, real-time
  path-tracing, "mark the hull as dynamic geometry" — which *would* model buttoned/hatch correctly) is a
  **separate download requiring a C++ recompile** for 5.8 → blocked while MSVC is absent, and its Linux
  binary must be validated. **ASSUMPTION:** a 5.8 build is feasible but unverified this phase.

### ADOPT — Native UE 5.8 only for M1 (research `10`)
The native stack covers **100% of the stated vision for M1 and well beyond**. The dominant cues here are
*direction* (panning/HRTF), *occlusion of distant/blocked threats* (a single listener→source trace LPF),
*interior vs open* (submix + modulation), and *snow medium* (a custom DSP layer we build regardless of
engine choice — research `09 §8`). Geometric reflection/diffraction across open snow buys little audible
payoff under a whiteout, and a real geometric engine costs an MSVC recompile and/or a per-level bake — none
justified this phase. Native is the correct, lower-risk, fully Pillar-3-serving choice.

**Revisit Steam Audio 4.x post-M1 ONLY if** exterior duels start depending on *hearing a shell whip past
and reflect*, or reverb that tracks moving through a ruined building, becomes a pillar-level payoff — and
only with an MSVC recompile budget + a validated Linux binary, keeping the native submix rig as fallback.
Tracked as **OD-A1**.

---

## 2. Sources — MetaSounds (the procedural authoring system)

**Confirmed in-box, ENABLED BY DEFAULT, production-ready (not beta):**
`Engine/Plugins/Runtime/Metasound/Metasound.uplugin` (`v1.0`, `EnabledByDefault:true`, `IsBetaVersion:false`).

MetaSound is the node-graph procedural-audio system (successor to SoundCue). Every Kessel source is a
MetaSound whose pitch/cadence/level are **graph inputs the sim writes** — so the realism lives in the
systems (Pillar 1) and the same asset behaves differently as RPM/speed/state change, with placeholder
samples now and recorded SFX swapped in later with **zero graph changes**:

| Source | MetaSound input(s) | Driven by | Notes |
|---|---|---|---|
| **Engine (diesel)** | `RPM`, `EngineLoad`, `EngineHealth` | `UKesselTrackedMovementComponent` (RPM/torque curve, `ARCHITECTURE §3`) | Looping bed pitched + crossfaded by RPM; layer count grows with load. A pitch droop on `EngineHealth` makes the engine note a *health sensor* (`12`). |
| **Track clank** | `TrackSpeed` (per side L/R) | movement component | Retrigger interval scales inverse with speed; randomize sample+pitch per hit. Two instances (left/right) for stereo tread. |
| **Reload** | `ReloadStep` trigger (breech-open → shell-slide → ram → breech-close) | `UKesselGunComponent` reload timer | Stepped sequence; later Quartz-quantized (§7) so each click lands tight. When the loader (you) is hit, the sequence **stops** — the truest damage cue is what the machine can no longer do (`13`). |
| **Gun report** | `Fire` trigger, `Caliber`, `ButtonednessRoute` | gun component on fire | Layered crack + body + tail with randomized pitch/level; tail routed more to `SM_OpenAir` when exposed (§5). |
| **Incoming whistle** | `ClosingSpeed`, `MissDistance`, `Bearing` | enemy gun fire event → audio component | Doppler-ish pitch sweep panned by bearing; louder/sharper as `MissDistance`→0. |
| **Collision / hit-spang** | `ImpactEnergy`, `bPenetration`, `Material` | `UKesselArmorComponent` hit resolution | Two distinct timbres: **"spang"** (ricochet/non-pen, bright metallic) vs **"bang/thunk"** (pen). This spang-vs-bang split is a load-bearing cue (`13`). |

### 2.1 AudioMotorSim — relevant but EXPERIMENTAL, do not depend on it
`Engine/Plugins/Runtime/AudioMotorSim/AudioMotorSim.uplugin` (`v1.0`, **`IsExperimentalVersion:true`**, off
by default) is a purpose-built engine/RPM motor→MetaSound model — tempting for the engine note. But it is
experimental (API may shift) and **car-gearbox-shaped**, not a slow tank diesel. **M1: drive a MetaSound
engine layer directly from our own diesel RPM curve** (above) — zero experimental dependency, fully tunable.
Re-evaluate post-M1 only if gear/load modeling becomes worth it. **ASSUMPTION:** enabling a precompiled
in-box plugin needs no C++ build, but *validating* behavior needs an editor launch (blocked this phase).

---

## 3. Direction + occlusion (the L/R bang, "the hull occludes")

**Confirmed core engine (no plugin):** `Engine/Source/Runtime/Engine/Classes/Sound/SoundAttenuation.h`.

- **Direction (panning):** `SpatializationAlgorithm` (`:210`, `ESoundSpatializationAlgorithm` at `:31`)
  = `SPATIALIZATION_Default` (panning). Default panning already places a left-side shell impact in the
  left ear — the user's literal example, zero third-party code.
- **Occlusion (real line trace):** `bEnableOcclusion` (`:174`), `OcclusionTraceChannel` (`:234`),
  `OcclusionLowPassFilterFrequency` (`:345`), `OcclusionVolumeAttenuation` (`:349`),
  `OcclusionInterpolationTime` (`:353`), `bUseComplexCollisionForOcclusion` (`:178`). A built-in
  listener→source trace that ramps in an LPF + volume cut when blocked = "the hull/terrain shadows the
  threat." **Point it at a dedicated `AudioOcclusion` trace channel** (mirrors the `WeaponTrace` channel
  pattern from `MILESTONE_1_PLAN` step 8). Note (`10`): this is a *single* listener→source ray, not true
  diffraction/reflection — acceptable under the enclosed-interior/whiteout premise, but a known ceiling.
- **Sends + air absorption:** `bEnableSubmixSends` (`:198`) + `FAttenuationSubmixSendSettings` (`:127`)
  route each source into the submix graph (§4); `bEnableReverbSend` (`:182`); `EAirAbsorptionMethod`
  (distance HF roll-off) already makes far sounds duller, reinforcing the snow muffle (§6) before any
  custom DSP.

### 3.1 Binaural (optional headphone toggle)
Ship M1 on **panning** (simplest, robust, cross-platform). Offer **HRTF** via the in-box `Spatialization`
plugin (`Engine/Plugins/Runtime/Spatialization/Spatialization.uplugin`, `v1.0`, off) as a headphone option:
select it as the project Spatialization plugin, set attenuation to `SPATIALIZATION_HRTF`, `BinauralRadius`
(`:218`) for the near-swap radius. No recompile (ships precompiled). `ResonanceAudio` (default-on, **beta**,
HRTF + room reflections) and `SoundFields` (default-on, **beta**, Ambisonics) are optional ambience tools,
**not** the spine. **Skip `MicrosoftSpatialSound`** — Win64-only, breaks Linux parity (`ARCHITECTURE §12`).

---

## 4. Submix graph — interior-muffle vs open-air

**Core engine submix effects:** `Engine/Source/Runtime/AudioMixer/Classes/SubmixEffects/`
(`...Reverb.h`, `...EQ.h`, `...DynamicsProcessor.h`). **Synthesis & DSP Effects** plugin
(`Engine/Plugins/Runtime/Synthesis/Synthesis.uplugin`, `v1.1`, default-on, not beta) adds
`SubmixEffectConvolutionReverb.h` (IR reverb — drop in a recorded real-tank-interior IR later),
`SubmixEffectFilter.h`, `SubmixEffectFlexiverb.h`, `SubmixEffectMultiBandCompressor.h`, delays.

**Kessel submix topology:**
```
[MetaSound sources] --attenuation submix-sends-->  SM_World  (combat/engine/track, full spectrum)
                                                       |
                              +------------------------+------------------------+
                              |                                                 |
                        SM_Interior (buttoned)                            SM_OpenAir (head-out)
                        - SubmixEffectFilter  (LPF ~2-4 kHz)              - light SubmixEffectReverb
                        - SubmixEffectReverb  (small boxy cavity)           (snowfield "dead" short tail)
                        - EQ low-mid bump (boom in the steel)             - minimal EQ (bright/present)
                              \________________________ SM_Master ________________________/
                                                       |
                                          SM_Weather (master air-absorption LPF, §6)
```
Cleaner than hard-switching chains: keep **one signal path** whose filter cutoff / reverb wet / open-bed
gain are driven by the single `Buttonedness` modulation bus (§5), so the hatch transition **crossfades**
("thunk — the world goes muffled") instead of popping. Convolution reverb lets the user drop a real interior
IR into the buttoned state with no code change.

`SM_Interior` also hosts the **interior-present bed** (engine close, reload clatter, hull creaks, radio)
which stays loud when buttoned and drops when exposed; `SM_OpenAir` hosts the open snow-wind bed.

---

## 5. Buttoned-up vs head-out — tied to the EViewMode state machine

This is the heart of the user's "open-air vs muffled interior mixing tied to the view-mode state." All
native, event-driven (no per-tick coupling):

- `UKesselViewComponent` owns `EViewMode {GunnerOptic, CommanderButtoned, Unbuttoned}` (`ARCHITECTURE §5`).
  On `Enter(mode)` it writes a single scalar **`Buttonedness` ∈ [1.0 buttoned … 0.0 head-out]**:

  | EViewMode | Buttonedness | Mix character |
  |---|---|---|
  | `GunnerOptic` | **1.0** | Fully sealed — exterior heavily LPF'd + quiet; interior bed present; tiny hard cavity reverb. |
  | `CommanderButtoned` | **~0.85** | Periscope; still buttoned, marginally more exterior bleed than the optic eyecup. |
  | `Unbuttoned` | **0.0** | Head in open air — exterior direct + bright + fully directional; interior bed drops. |

- `Buttonedness` drives an **Audio Modulation control bus** (§5.1) mapped to: interior submix LPF cutoff
  (low when buttoned), interior reverb wet, open-air bed gain, master EQ tilt, **and** the gun-report tail
  routing (open report sends more to `SM_OpenAir`). The bus's interpolation time gives the smooth
  hatch-close muffle for free.
- This keeps audio **decoupled** from the render/gameplay tick (it reacts to view-change events), matching
  the architecture's decoupling discipline. The buttoned/open state is a **listener-state** concern → drive
  it with Audio Modulation, **not** an `AudioGameplayVolume` (which is for *world-placed* zones, §8).

### 5.1 Audio Modulation — the crossfade mechanism
**Confirmed in-box:** `Engine/Plugins/Runtime/AudioModulation/AudioModulation.uplugin` (`v2.0`,
**`EnabledByDefault:false`**, *not* beta, *not* experimental). Provides Control Buses, Bus Mixes, parameter
patches, and LFO/envelope modulation generators that any sound/submix/source-effect parameter can subscribe
to — the idiomatic way to do "one knob = buttoned-ness" feeding many DSP targets with smooth interpolation,
instead of hand-lerping volumes in C++. **MUST be enabled in the `.uproject`** alongside
`EnhancedInput`/`Niagara`/`ChaosVehiclesPlugin` — if forgotten, the buttoned/open rig silently won't load
(risk, §10).

---

## 6. Snow / blizzard muffling model — one weather value, shared with the fog

Falling snow is an **atmospheric medium** property (distance-dependent low-pass + level attenuation), not
surface absorption or geometry — neither Steam Audio nor Project Acoustics models it natively, so it is a
custom DSP layer **regardless of engine choice** (research `09 §8`). We own it, and we tie it to the same
dial that drives the look so audio and visuals breathe together:

- **One scalar `BlizzardIntensity` ∈ [0..1]** is the shared weather value. It is read from the active
  `UAtmospherePreset` (the same asset that sets volumetric fog density/extinction in `ARCHITECTURE §8`),
  so **fog thickness and sound muffle move together** — a deeper whiteout is a deader, quieter world. This
  is the "one weather value → air absorption + HF rolloff, shared with the fog" requirement.
- **Two coupled effects:**
  1. **Per-source distance × weather:** in each source's MetaSound, an LPF cutoff and gain curve as a
     function of `Distance × BlizzardIntensity` — distant threats in heavy snow are dull and faint; the
     same threat in light snow is crisp and carries. Reinforces (does not replace) the engine's native
     `EAirAbsorptionMethod` distance roll-off (§3).
  2. **Master air-absorption LPF:** a gentle global `SubmixEffectFilter` on `SM_Weather` (below Master)
     whose cutoff lowers as `BlizzardIntensity` rises — a subtle "everything is wrapped in wool" floor.
- **Couples with buttoned/open:** buttoned, the snow-muffle is already swamped by the hull LPF; exposed,
  the blizzard hiss bed (open snow-wind in `SM_OpenAir`) rises with `BlizzardIntensity`, so popping the
  hatch in a storm is loud, cold, and disorienting — the survival-horror payoff.

Implementation seam: a tiny modulation patch maps `BlizzardIntensity` onto the `SM_Weather` filter and a
global MetaSound parameter; `AKesselAtmosphereController` writes it whenever it applies a preset, so there
is exactly one source of weather truth for both fog and sound. Tracked as **OD-A2** (does snow ever get
loud enough to *mask* a threat as a gameplay mechanic, or stay purely atmospheric?).

---

## 7. Quartz — sample-accurate mechanical timing (core engine, low M1 priority)

**Confirmed core engine (no plugin):** `UQuartzSubsystem` at
`Engine/Source/Runtime/AudioMixer/Public/Quartz/QuartzSubsystem.h`. A clock/transport that schedules audio
on a sample-accurate timeline (not frame-quantized `SetTimer`). Kessel uses it for: **reload choreography**
(each mechanical step on a quantization boundary so the reload always sounds tight regardless of frame
rate), **track clank cadence** (a Quartz clock whose tempo scales with track speed → rhythmic, jitter-free),
and **layered gun-report transients** (exact sub-frame layer offsets for a punchy, consistent crack).

**Not mandatory for M1's first pass** — frame timers work for the placeholder — but it is free, in-box, and
the right tool the moment mechanical audio needs to feel deliberate. **High payoff, defer to a polish pass.**

---

## 8. Deferred / optional (post-M1)

- **`AudioGameplayVolume`** (`Engine/Plugins/AudioGameplayVolume/...`, `v1.0`, **beta**, off): world-placed
  acoustic zones — `Reverb/Filter/Attenuation/SubmixSend/SubmixOverride` volume components. Good for
  *environmental pockets* (a dugout, a burning-wreck zone, a heavy-snow hollow) that muffle/reverberate as
  you drive through. Post-M1 nicety; validate in-editor (beta) before relying on it. The buttoned/open
  state is **not** this — that is listener-state (§5).
- **`AudioGameplay`** (beta, off): actor audio-behavior component framework — convenience, not required.
- **`AudioSynesthesia`** (beta, off): real-time analysis (loudness/onset/NRT) — a post-M1 path to drive
  visual cues *from* audio (e.g. a muzzle-flash light pulse on report onset) if ever wanted.
- **Steam Audio 4.x** — see §1 / OD-A1.

---

## 9. Integration seams — `UKesselAudioComponent` + the sim systems

A single `UKesselAudioComponent : UActorComponent` on `AKesselTankPawn` is the seam every other system
calls into. It owns the `UAudioComponent`(s)/MetaSound instances, the `Buttonedness`/`BlizzardIntensity`
modulation patches, and the submix routing setup. Other systems push **events and scalars**; the audio
component never reads their internals (one-way, decoupled — matching `ARCHITECTURE §7`'s event/tick split).

| Caller (existing system) | Call into `UKesselAudioComponent` | When | Sound |
|---|---|---|---|
| `UKesselViewComponent` (`EViewMode`) | `SetButtonedness(float)` → writes the `Buttonedness` control bus | on `Enter(mode)` (event) | §5 interior/open crossfade |
| `UKesselTrackedMovementComponent` | `SetEngineState(RPM, Load, Health)`, `SetTrackSpeed(L, R)` → MetaSound inputs | each tick (cheap param writes) | §2 engine + track |
| `UKesselGunComponent` | `OnFire(Caliber)`, `OnReloadStep(EReloadStep)` | events | §2 report + reload |
| `UKesselArmorComponent` | `OnHitResolved(FHitResolution)` → spang vs bang, `OnCollision(energy, surface)` | events | §2 hit/spang/thunk, §11 collision |
| enemy `UKesselGunComponent` fire | `OnIncomingRound(bearing, closingSpeed, missDistance)` | event when an enemy fires toward player | §2 whistle |
| `AKesselAtmosphereController` | `SetBlizzardIntensity(float)` → weather patch (shared with fog) | on preset apply (event) | §6 snow muffle |

**Blueprint note:** all setters are `UFUNCTION(BlueprintCallable)` so the MCP/Blueprint side (and the
optional step-13 thin audio pass) can wire the placeholder loop without C++ when MSVC is still absent.
The component exposes the MetaSound assets as `UPROPERTY(EditAnywhere)` references so swapping placeholder
→ recorded SFX is a data edit, not a code change.

**Collision channel:** add `AudioOcclusion` to the project's trace channels (alongside `WeaponTrace`,
`MILESTONE_1_PLAN` step 8) for `OcclusionTraceChannel` (§3). Hull + terrain block it; the fog/blizzard do
not (they are the §6 medium layer, not a trace occluder).

---

## 10. Risks (paper-phase)

- **R-A1 (med):** **Audio Modulation is OFF BY DEFAULT.** If not enabled in the `.uproject`, the entire
  buttoned/open rig (§5) silently fails to load. Add it to the explicit plugin list at project-skeleton time.
- **R-A2 (med):** **No editor validation this phase.** Enabling precompiled in-box plugins needs no build,
  but confirming behavior needs an editor launch (blocked). All "verify in editor" items (§12) are
  deferred **ASSUMPTION**s — including that the in-box plugins behave as their headers imply.
- **R-A3 (low/med):** **Several adjacent plugins are BETA** (`ResonanceAudio`, `SoundFields`,
  `AudioGameplay`, `AudioGameplayVolume`, `AudioSynesthesia`) or **EXPERIMENTAL** (`AudioMotorSim`) —
  API/behavior may shift; keep them optional/deferred and validate before depending on any.
- **R-A4 (low/med):** **Native occlusion is a single ray**, not diffraction/reflection (§3). Acceptable
  under the whiteout/enclosed-interior premise, but a known fidelity ceiling — the trigger to revisit
  Steam Audio (OD-A1).
- **R-A5 (low):** **Linux parity** — the *recommended* native stack is fully cross-platform, but the two
  tempting shortcuts (`MicrosoftSpatialSound`, in-box deprecated Steam Audio) are Win64-only. Do not let
  either leak into the spine (`ARCHITECTURE §12`).
- **R-A6 (low):** **Cue collision.** The cue vocabulary (spang-vs-bang, diesel cadence = health, whistle =
  incoming, LED color language) must be one authoritative table or cues become unlearnable (`13`). It
  cannot be finalized until real SFX exist (M1 is placeholder) — lock it during the SFX-recording pass.

---

## 11. M1 placeholder slice (what to use NOW vs record LATER)

M1 uses **placeholder samples**; the user records real SFX later and swaps them in with **zero MetaSound
graph change** (the graphs are parameterized, §2). This is the optional, decision-gated step-13 "thin audio
pass" in `MILESTONE_1_PLAN` (**OD-3**).

**Use NOW (in-box engine content, no DCC, no recording):**
- Engine sample sounds under `C:/Program Files/Epic Games/UE_5.8/Engine/Content/EngineSounds` and template
  audio (e.g. `Templates/TP_VehicleAdv` engine/UI cues) as stand-in **engine loop, gun report, impact**.
- Procedural placeholders authored in MetaSound (oscillator/noise beds) where no sample fits — e.g. a
  filtered-noise "diesel" pitched by RPM, a noise-burst "clank," a click-train "reload," a band-passed
  noise "report." These are crude but **already correct as systems** (they react to the sim), which is the
  whole point — placeholder *pixels*, real *behavior*.
- One `BlizzardIntensity`-driven filtered-noise bed for open snow-wind.

**Minimal M1 wiring (step-13 slice, if OD-3 = yes):** `UKesselAudioComponent` with (1) engine loop pitched
by `RPM`/speed, (2) gun report on `OnFire`, (3) impact/spang on `OnHitResolved`, (4) the `Buttonedness`
LPF crossfade on view change, (5) the `BlizzardIntensity` open-snow bed. Even this thin set massively sells
the survival-horror loop. If OD-3 = no, **defer audio in writing** so it is not silently dropped.

**Record LATER (user's real SFX, swapped per-asset):** real Maybach/diesel engine layers at multiple
RPM/load points; track tread + road-wheel clanks; the full 8,8 cm report (crack/body/tail) and breech
mechanism (open/ram/lock/close); hull-creak and interior Foley; shell-impact spangs (ricochet) vs pen
thunks per material; incoming-round whistle; the real **tank-interior impulse response** for the
`SM_Interior` convolution reverb (the single highest-fidelity upgrade — turns the buttoned mix from
"a generic small reverb" into "this specific steel box").

---

## 12. Open decisions

- **OD-A1 — Steam Audio 4.x post-M1?** Adopt the modern Valve engine later *only if* exterior
  reflection/diffraction (hearing a shell whip past and reflect, reverb tracking through a ruin) becomes a
  pillar-level payoff — and only with an MSVC recompile budget + validated Linux binary. Default: **no**;
  native is sufficient. (Mirrors `09`/`10`.)
- **OD-A2 — Snow as a gameplay mask vs pure atmosphere.** Does `BlizzardIntensity` ever get loud/muffling
  enough to *mask* a real threat (a mechanic — heavy snow can hide an approaching tank's note), or stay
  purely atmospheric? Affects whether the §6 curves are tuned for legibility or immersion.
- **OD-A3 — HRTF default.** Ship M1 on panning (default) with HRTF as an opt-in headphone toggle, or make
  HRTF the default? Panning is the safe cross-platform default; HRTF needs the `Spatialization` plugin
  enabled and validated.
- **OD-A4 — Audio in M1 (= `MILESTONE_1_PLAN` OD-3).** Thin placeholder pass (step 13) in M1, or defer
  audio wholesale to post-M1? Recommend **the thin pass** — even placeholder audio is the cheapest large
  win for the "feeling" north star, and the §9 seams exist regardless.
- **OD-A5 — Quartz now or polish pass.** Frame-timer reload/clank for M1 (simplest) vs Quartz from the
  start (tighter but more setup). Recommend **frame timers for M1**, Quartz in the mechanical-audio polish
  pass (§7).
- **OD-A6 — Cue vocabulary lock.** When real SFX are recorded, finalize the single authoritative cue table
  (spang-vs-bang, diesel-cadence-as-health, whistle, interior-bed presence) so cues stay learnable (`13`,
  R-A6). Deferred until SFX exist.
```
