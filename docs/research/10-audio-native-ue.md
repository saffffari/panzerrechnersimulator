# 10 — Native UE 5.8 Audio (the baseline we use regardless of the spatial engine)

Status: **RESEARCH** (design only — no C++, no editor launch, no build this phase).
Engine: **UE 5.8.0** at `C:/Program Files/Epic Games/UE_5.8`. Every plugin/feature below is confirmed
on disk; paths are cited. Anything not provable on disk is tagged **ASSUMPTION**.

Governing pillar: **DEEP SIMULATION, ABSTRACTED PRESENTATION** + Pillar 3 **"Sound is the primary
sensor."** The audio *systems* carry maximal realism (procedural sources, spatialization, occlusion,
buttoned/open mixing) while assets stay placeholder in M1 (user records real SFX later). This doc
catalogs what ships **in-box** so we never wait on a community plugin (MSVC absent → nothing needing a
C++ recompile can be built/validated this phase — see §7).

Scope answered: MetaSounds, Attenuation + trace Occlusion, the Submix graph + submix effects,
spatialization/binaural, Audio Modulation, Quartz, AudioGameplay / AudioGameplayVolumes — plus where a
dedicated spatial engine (Steam Audio / Project Acoustics) is genuinely needed vs overkill.

---

## 0. TL;DR recommendation

**The native UE 5.8 stack covers 100% of the user's stated audio vision for M1 and well beyond** —
directional bang ("shell left of the tank → left ear"), snow muffle, buttoned-vs-open mixing tied to
the view-mode state machine, and procedural engine/track/reload/gun sources. Nothing in the user's
verbatim intent *requires* a third-party acoustics engine to ship M1.

Native baseline (all in-box, see §1–§6):
- **Procedural sources → MetaSounds** (enabled by default): engine pitched by RPM, track clank rate by
  speed, reload mechanism, layered gun report. **Author RPM→pitch in the MetaSound graph from a
  gameplay parameter** rather than depending on the *experimental* `AudioMotorSim` plugin (§1.2).
- **Direction → Sound Attenuation spatialization** (core): panning for the L/R bang now; optional true
  **binaural HRTF** via the in-box `Spatialization` or `ResonanceAudio` plugins (no recompile).
- **Snow muffle + buttoned/open → Submix graph + submix effects + Audio Modulation**, switched by the
  `UKesselViewComponent` `EViewMode` state machine (§4, §5). One "buttoned-ness" modulation control bus
  is the clean way to crossfade interior LPF/reverb vs open-air.
- **Mechanical timing → Quartz** (core) for sample-accurate reload steps / layered report.

**A dedicated geometric/"ray-traced" acoustics engine is OVERKILL for M1** (§8). The in-box Steam Audio
is **deprecated + Win64-only + beta** (dead-end for the Linux target); Project Acoustics is **not
in-box** (separate plugin + offline bake tooling). Defer both; revisit only post-M1 if exterior
reflection/diffraction becomes a pillar-level payoff, and budget an MSVC recompile + Win64-first if so.

---

## 1. MetaSounds — procedural sources (THE source authoring system)

**Confirmed in-box, ENABLED BY DEFAULT, production-ready (not beta):**
`Engine/Plugins/Runtime/Metasound/Metasound.uplugin` — `"FriendlyName":"MetaSound"`,
`"VersionName":"1.0"`, `"EnabledByDefault":true`, `"IsBetaVersion":false`, `"Category":"Audio"`.
269 headers under `Engine/Plugins/Runtime/Metasound/Source`.

MetaSound is the modern node-graph procedural-audio system (the successor to SoundCue) — sample-accurate
DSP graphs with typed inputs you drive from gameplay. This is where every Kessel source lives:

| Kessel source | MetaSound approach (no extra plugin) |
|---|---|
| **Engine pitched by RPM** | A looping engine layer (or oscillator/granular bed) whose pitch + crossfade is driven by an `RPM` graph input the movement component writes each tick. Reuse the `FVehicleEngineConfig` RPM/torque curve already in ARCHITECTURE §3 to feed it. |
| **Track clank by speed** | A retrigger node (or Quartz-clocked trigger, §6) whose interval scales with a `TrackSpeed` input — clank cadence rises with speed; randomize sample + pitch per hit for variation. |
| **Reload mechanism** | A sequence of triggers (breech open → shell slide → breech close) fired as a timed graph or stepped by Quartz quantization so each mechanical click lands on the beat of the reload timer. |
| **Gun report (layers)** | One MetaSound that mixes layers — crack + body + tail/echo — with randomized pitch/level; tail length later swappable per buttoned/open (route to a different submix, §4). |

Because pitch/cadence are **graph inputs fed by the sim**, the realism lives in the systems (pillar 1):
the same MetaSound asset behaves differently as RPM/speed/state change, with placeholder samples now and
the user's recorded SFX swapped in later with zero graph changes.

### 1.2 AudioMotorSim — relevant but EXPERIMENTAL (use with care)
`Engine/Plugins/Runtime/AudioMotorSim/AudioMotorSim.uplugin` — `"FriendlyName":"Audio Motor Sim"`,
`"VersionName":"1.0"`, **`"IsExperimentalVersion":true`, `"EnabledByDefault":false`**. Modules:
`AudioMotorSim`, `AudioMotorSimStandardComponents`, `AudioMotorSimDebug`. It is a purpose-built
engine/RPM/gear motor model (originally from the Ridge Racer-style sample) that drives a MetaSound.
**Recommendation:** tempting for the engine, but it is *experimental* (API may shift) and tank-diesel ≠
car-gearbox. For M1, drive a MetaSound engine layer directly from our own RPM curve (above) — zero
experimental dependency, fully tunable. Re-evaluate `AudioMotorSim` post-M1 if we want gear/load
modeling for free. **ASSUMPTION:** since it ships precompiled with the engine, enabling it needs no C++
build — but validating it does (editor launch is blocked this phase).

---

## 2. Sound Attenuation + trace-based Occlusion (direction + "the hull occludes")

**Confirmed core engine (no plugin):**
`Engine/Source/Runtime/Engine/Classes/Sound/SoundAttenuation.h`. Verified fields:

- **Occlusion (real line-trace):** `bEnableOcclusion` (`:174`, "Enables realtime occlusion tracing"),
  `OcclusionTraceChannel` (`:234`, "Which trace channel to use for audio occlusion checks"),
  `bUseComplexCollisionForOcclusion` (`:178`), `OcclusionLowPassFilterFrequency` (`:345`),
  `OcclusionVolumeAttenuation` (`:349`), `OcclusionInterpolationTime` (`:353`). This is a built-in
  listener→source trace that, when blocked, ramps in a low-pass + volume cut — exactly "the hull/terrain
  occludes the threat." Point it at a dedicated `AudioOcclusion` channel (mirrors the `WeaponTrace`
  channel pattern from MILESTONE_1_PLAN step 8).
- **Spatialization method:** `SpatializationAlgorithm` (`:210`) of `ESoundSpatializationAlgorithm`
  (`SoundAttenuation.h:31`) = `SPATIALIZATION_Default` ("Panning") or `SPATIALIZATION_HRTF`
  ("Plugin-Spatialized"); `BinauralRadius` (`:218`) sets the near radius where it swaps off binaural.
- **Sends for the muffle/reverb rig:** `bEnableReverbSend` (`:182`), `bEnableSubmixSends` (`:198`) +
  `FAttenuationSubmixSendSettings` (`:127`) — per-attenuation routing into our submix graph (§4).
- **Focus + air absorption:** `bEnableListenerFocus` (`:166`) and `EAirAbsorptionMethod`
  (`:~45`, Distance-Linear etc.) — distance-based HF roll-off that already reads as "far sounds are
  duller," reinforcing the snow-muffle feel before any custom DSP.

**For the L/R bang:** default panning already places a left-side impact in the left ear. Occlusion +
air-absorption already darken distant/blocked threats. This is the directional + occlusion baseline with
zero third-party code.

---

## 3. Spatialization / Binaural options (in-box)

Stereo headphone realism beyond panning, all on disk:

- **`Spatialization`** plugin (Epic's built-in binaural/ITD panner):
  `Engine/Plugins/Runtime/Spatialization/Spatialization.uplugin` — `"VersionName":"1.0"`, **off by
  default**. The modern in-box HRTF/ITD option; selected as the project's Spatialization plugin, then
  attenuation set to `SPATIALIZATION_HRTF`. No recompile (ships precompiled).
- **`ResonanceAudio`** (Google Resonance): `Runtime/ResonanceAudio/ResonanceAudio.uplugin` — v1.0,
  **enabled by default**, beta. HRTF binaural **plus** a room/early-reflection model — a cheap way to get
  directional + simple reverberation together.
- **`SoundFields`** (Ambisonics renderer): `Runtime/SoundFields/SoundFields.uplugin` — v1.0, enabled by
  default, beta. First-order Ambisonics submix/bed — useful for a rotating ambient battlefield field that
  stays correct as the turret/head turns.
- **`MicrosoftSpatialSound`** (Windows Sonic / ISAC): `Runtime/MicrosoftSpatialAudio/` — present, **off
  by default, Win64-only** (no Linux binary on disk). Platform endpoint spatializer; **skip** — it breaks
  the Linux parity rule (ARCHITECTURE §12).

**Recommendation:** ship M1 on **panning** (simplest, robust, cross-platform). Offer **HRTF via the
in-box `Spatialization` plugin** as a headphone toggle — it is the cleanest in-box binaural and needs no
build. Treat `ResonanceAudio`/`SoundFields` as optional ambience tools, not the M1 spine. (See §7 for the
beta-status risk.)

---

## 4. Submix graph + submix effects (interior-muffle vs open-air)

**Confirmed core engine:** `SoundSubmix.h`, `SoundSubmixSend.h`, `SoundEffectSubmix.h`
(`Engine/Source/Runtime/Engine/Classes/Sound/`); mixer submix at
`Runtime/AudioMixer/Public/AudioMixerSubmix.h`.

**Built-in submix effects (no plugin):**
`Engine/Source/Runtime/AudioMixer/Classes/SubmixEffects/` →
`AudioMixerSubmixEffectReverb.h`, `AudioMixerSubmixEffectEQ.h`,
`AudioMixerSubmixEffectDynamicsProcessor.h`.

**Synthesis & DSP Effects plugin** (`Runtime/Synthesis/Synthesis.uplugin` — v1.1, **enabled by
default**, not beta) adds richer submix effects at
`.../Synthesis/Classes/SubmixEffects/`: **`SubmixEffectConvolutionReverb.h`** (impulse-response reverb —
record/IR the real tank interior later), `SubmixEffectFilter.h`, `SubmixEffectFlexiverb.h`,
`SubmixEffectMultiBandCompressor.h`, plus Delay/StereoDelay/TapDelay. Source-level DSP is also here
(`.../Classes/SourceEffects/`: `SourceEffectEQ/Filter/DynamicsProcessor/ConvolutionReverb/...`) for
per-source shaping (e.g. the engine source).

**Kessel submix topology (proposal):**
```
[Sources] --send--> SM_World (combat/engine/track, full spectrum)
                       |
        +--------------+-----------------+
        |                                |
   SM_Interior  (buttoned)          SM_OpenAir (head-out)
   - SubmixEffectFilter (LPF ~2-4kHz)   - light SubmixEffectReverb (snowfield "dead" tail)
   - SubmixEffectReverb (small boxy IR) - minimal EQ
   - EQ low-mid bump (boom in steel)
        \________________ Master ________________/
```
The view-mode state machine (§5) selects which interior/open chain is hot, or — cleaner — keeps **one
chain** whose filter cutoff / reverb wet are driven by a single **modulation control bus** (§5/§6) so the
transition crossfades instead of hard-cutting. Convolution reverb lets the user drop in a real interior
IR for the buttoned state with zero code change.

---

## 5. Buttoned-up vs head-out mixing tied to the view-mode state machine

This is the heart of the user's "open-air vs muffled interior mixing tied to the view-mode state."
Wiring (all native):

- `UKesselViewComponent` already owns `EViewMode {GunnerOptic, CommanderButtoned, Unbuttoned}`
  (ARCHITECTURE §5). On `Enter(mode)` it sets a single scalar **`Buttonedness` (1.0 buttoned → 0.0
  head-out)**.
- `Buttonedness` drives an **Audio Modulation control bus** (§5.1) that maps to: interior submix LPF
  cutoff (low when buttoned), interior reverb wet, open-air bed volume, and master EQ tilt. Interp time on
  the bus gives the "thunk, the world goes muffled as the hatch closes" transition for free.
- The exposed/buttoned flag also gates which ambience bed plays (open snow-wind vs damped hull interior)
  and the gun-report tail routing (§1: open report sends more to the tail submix).

This keeps audio *decoupled* from the render/gameplay tick (event-driven on view change), matching the
architecture's decoupling discipline.

### 5.1 Audio Modulation (the clean crossfade mechanism)
**Confirmed in-box:** `Engine/Plugins/Runtime/AudioModulation/AudioModulation.uplugin` —
`"FriendlyName":"Audio Modulation"`, `"VersionName":"2.0"`, **`"EnabledByDefault":false`**, **not beta,
not experimental**. Provides **Control Buses, Bus Mixes, parameter patches, and modulation generators
(LFO/envelope)** that any sound/submix/source-effect parameter can subscribe to. This is the idiomatic
way to do "one knob = buttoned-ness" feeding many DSP targets with smooth interpolation, instead of
hand-lerping volumes in C++. **Must be enabled in the `.uproject`** (off by default) — add it alongside
`EnhancedInput`/`Niagara`/`ChaosVehiclesPlugin`.

---

## 6. Quartz — sample-accurate mechanical timing (core engine)

**Confirmed core engine (no plugin):** `UQuartzSubsystem` at
`Engine/Source/Runtime/AudioMixer/Public/Quartz/QuartzSubsystem.h` (impl `.../Private/Quartz/
QuartzSubsystem.cpp`); quantization utilities at
`Engine/Source/Runtime/Engine/Classes/Sound/Quartz*.h`.

Quartz is a clock/transport that schedules audio events on a sample-accurate timeline (not frame-quantized
`SetTimer`). Kessel uses:
- **Reload choreography:** each mechanical step (breech, ram, lock) fired on Quartz quantization boundaries
  so the reload always sounds mechanically tight regardless of frame rate.
- **Track clank cadence:** drive the §1 clank retrigger off a Quartz clock whose tempo scales with track
  speed → rhythmic, jitter-free clanks.
- **Layered gun report:** schedule report layers with exact sub-frame offsets for a punchy, consistent
  transient.

Not mandatory for M1's first pass (frame timers work for a placeholder), but it's free, in-box, and the
right tool the moment mechanical audio needs to feel deliberate. **Low priority for M1, high payoff later.**

---

## 7. AudioGameplay / AudioGameplayVolume (spatial muffle zones)

Both are **plugins, BETA, off by default** — enable in `.uproject` if used:

- **`AudioGameplay`** (`Engine/Plugins/AudioGameplay/AudioGameplay.uplugin` — v1.0, beta, off):
  component framework (`AudioGameplayComponent.h`, `AudioComponentGroup.h`) for attaching audio behavior
  to actors. Optional convenience; not required for M1.
- **`AudioGameplayVolume`** (`Engine/Plugins/AudioGameplayVolume/...` — v1.0, beta, off): **spatial
  volumes that apply audio effects when the listener is inside.** Confirmed components:
  `ReverbVolumeComponent.h`, `FilterVolumeComponent.h`, `AttenuationVolumeComponent.h`,
  `SubmixSendVolumeComponent.h`, `SubmixOverrideVolumeComponent.h`.

**Where this fits Kessel:** these are the **modern replacement for legacy Audio/Reverb Volumes** — drop a
volume over a dugout, a burning-wreck zone, or a heavy-snow pocket and it muffles/reverberates automatically
as you drive through. Good for *world-placed* acoustic zones. The *buttoned/open* state, by contrast, is a
**listener-state** thing (the hatch, not a place) → drive that with Audio Modulation (§5), not a volume.
**Recommendation:** treat `AudioGameplayVolume` as a post-M1 nicety for environmental pockets; it is beta,
so validate behavior in-editor before relying on it (blocked this phase).

---

## 8. Where a DEDICATED spatial engine is actually needed (vs overkill)

User's aspiration (verbatim): *"Ray-traced audio / acoustic simulation engine would be ideal."* What that
buys over native: geometry-driven **occlusion + diffraction + reflections + dynamically computed reverb**
(sound bends around the hull, reflects off a wall, reverb changes with the space) instead of a single
listener→source trace + authored submix reverb.

**In-box reality (confirmed on disk):**
- **Steam Audio ships in-box but DEPRECATED:**
  `Engine/Plugins/Runtime/Steam/SteamAudio/SteamAudio.uplugin` —
  **`"FriendlyName":"Steam Audio (Deprecated)"`, `"VersionName":"2.0-beta.17"`, beta, off by default.**
  Binaries are **Win64 only** (`.../Binaries/Win64/UnrealEditor-SteamAudio.dll`; **no Linux binary on
  disk**). This is the legacy Phonon-era build Epic bundled; it is a **dead-end** (deprecated) and breaks
  the Linux parity target. The current Valve Steam Audio (4.x) is a **separate download** and would need a
  **C++ recompile** for 5.8 → blocked while MSVC is absent (§ HARD CONSTRAINTS).
- **Microsoft Project Acoustics is NOT in-box** (no plugin found under `Engine/Plugins`). It needs a
  separate plugin **and an offline bake/voxelization step** (Azure/Windows tooling) per level — heavy
  pipeline, Win-centric baking.

**Verdict — OVERKILL for M1, defer:**
1. The world is a **whiteout** with very short draw distance and the player is sealed in **one enclosed
   interior**. Geometric reflection/diffraction across open snow buys little audible payoff; the dominant
   cues are *direction* (panning/HRTF), *occlusion of distant threats* (trace LPF), and *interior vs open*
   (submix + modulation) — all native (§2–§5).
2. The in-box Steam Audio is deprecated + Win64-only → adopting it now is technical debt against the Linux
   target.
3. A real geometric engine (Valve Steam Audio 4.x or Project Acoustics) costs an MSVC recompile and/or a
   per-level bake pipeline — neither is available/justified this phase.

**When to revisit (post-M1, only if it becomes a pillar payoff):** if exterior duels start depending on
*hearing a shell whip past and reflect*, or on reverb that tracks moving through a ruined building, then
evaluate **Valve Steam Audio 4.x** (real-time path tracing, free, cross-platform incl. Linux) — but budget
the MSVC recompile and validate the Linux binary, and keep the native submix rig as the fallback. Project
Acoustics only if a baked, authored acoustic field (and Win-first baking) is acceptable. Until then, the
native stack is the correct, lower-risk choice and fully serves Pillar 3.

---

## 9. Recommended native baseline (the M1 audio spine)

Enable in `.uproject` (beyond defaults): **`AudioModulation`** (off by default; needed for §5). MetaSound
and Synthesis are already on by default. Everything else optional/deferred.

1. **Sources = MetaSounds** (§1): engine(RPM), track-clank(speed), reload-steps, gun-report-layers — all
   driven by gameplay parameters; placeholder samples now, user SFX later. Skip experimental `AudioMotorSim`.
2. **Direction = Attenuation panning** (§2), `bEnableOcclusion` on an `AudioOcclusion` trace channel for
   the hull/terrain shadowing; air-absorption for distance dulling. HRTF (`Spatialization` plugin) as an
   optional headphone toggle.
3. **Muffle/space = Submix graph** (§4): `SM_Interior` (Filter LPF + small Reverb/Convolution + low-mid EQ)
   vs `SM_OpenAir` (light reverb tail), attenuation submix-sends route into them.
4. **Buttoned↔open = Audio Modulation** (§5): one `Buttonedness` control bus written by
   `UKesselViewComponent` drives the interior LPF/reverb/open-bed with smooth interp on hatch transitions.
5. **Timing = Quartz** (§6) when reload/clank/report need to feel deliberate (defer first pass).
6. **Defer:** `AudioGameplayVolume` (environmental pockets, beta), any third-party acoustics engine (§8).

This delivers the user's entire stated vision — directional bang, snow muffle, buttoned/open mixing tied to
the view state, plus engine/track/reload/gun/collision sources — using only in-box, mostly precompiled
features, with no dependency on the absent MSVC toolchain and no Linux-parity risk.

---

## 10. On-disk verification index (UE 5.8)

| Feature | Path | Status |
|---|---|---|
| MetaSound | `Engine/Plugins/Runtime/Metasound/Metasound.uplugin` | v1.0, **default-on**, not beta |
| Synthesis & DSP Effects | `Engine/Plugins/Runtime/Synthesis/Synthesis.uplugin` | v1.1, **default-on**, not beta |
| Submix effects (core) | `Engine/Source/Runtime/AudioMixer/Classes/SubmixEffects/{Reverb,EQ,DynamicsProcessor}.h` | core engine |
| Submix effects (Synthesis) | `.../Synthesis/Classes/SubmixEffects/{ConvolutionReverb,Filter,Flexiverb,MultiBandCompressor,*Delay}.h` | default-on |
| Source effects | `.../Synthesis/Classes/SourceEffects/{EQ,Filter,DynamicsProcessor,ConvolutionReverb,...}.h` | default-on |
| Sound Attenuation + Occlusion | `Engine/Source/Runtime/Engine/Classes/Sound/SoundAttenuation.h` (`bEnableOcclusion:174`, `OcclusionTraceChannel:234`, `SpatializationAlgorithm:210`, `bEnableSubmixSends:198`) | core engine |
| Submix graph | `.../Sound/{SoundSubmix,SoundSubmixSend,SoundEffectSubmix}.h`; `Runtime/AudioMixer/Public/AudioMixerSubmix.h` | core engine |
| Audio Modulation | `Engine/Plugins/Runtime/AudioModulation/AudioModulation.uplugin` | v2.0, **off by default**, not beta |
| Quartz | `Engine/Source/Runtime/AudioMixer/Public/Quartz/QuartzSubsystem.h` | core engine |
| Spatialization (binaural) | `Engine/Plugins/Runtime/Spatialization/Spatialization.uplugin` | v1.0, off by default |
| Resonance Audio | `Engine/Plugins/Runtime/ResonanceAudio/ResonanceAudio.uplugin` | v1.0, default-on, **beta** |
| SoundFields (Ambisonics) | `Engine/Plugins/Runtime/SoundFields/SoundFields.uplugin` | v1.0, default-on, **beta** |
| Microsoft Spatial Sound | `Engine/Plugins/Runtime/MicrosoftSpatialAudio/MicroSoftSpatialSound.uplugin` | **Win64-only**, off → skip |
| AudioMotorSim | `Engine/Plugins/Runtime/AudioMotorSim/AudioMotorSim.uplugin` | v1.0, **EXPERIMENTAL**, off |
| AudioGameplay | `Engine/Plugins/AudioGameplay/AudioGameplay.uplugin` | v1.0, **beta**, off |
| AudioGameplayVolume | `Engine/Plugins/AudioGameplayVolume/...` (`Reverb/Filter/Attenuation/SubmixSend/SubmixOverride` VolumeComponents) | v1.0, **beta**, off |
| Audio Synesthesia (analysis) | `Engine/Plugins/Runtime/AudioSynesthesia/AudioSynesthesia.uplugin` | v1.0, **beta**, off |
| Sound Utilities | `Engine/Plugins/Runtime/SoundUtilities/SoundUtilities.uplugin` | v1.0, **beta**, off |
| Audio Insights (profiling) | `Engine/Plugins/AudioInsights*`, `AudioModulationInsights` | dev tooling |
| **Steam Audio (in-box)** | `Engine/Plugins/Runtime/Steam/SteamAudio/SteamAudio.uplugin` | **DEPRECATED**, v2.0-beta.17, **Win64-only**, off → avoid |
| Project Acoustics | — | **NOT in-box** (external plugin + bake) |
