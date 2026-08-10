# 09 — Audio: Microsoft Project Acoustics evaluation (UE 5.8)

Status: RESEARCH (design only; no plugin installed, no editor launch, MSVC absent this phase).
Engine: UE 5.8.0. Pillar under test: **#3 "Sound is the primary sensor"** and the governing
**"deep simulation, abstracted presentation."**
Scope: should Kessel adopt **Microsoft Project Acoustics** for its spatial/occlusion audio? How does
it handle the defining **buttoned-up vs. hatch-open** case, a **moving tank**, a **destructible
world**, and the **blizzard**? Compared against **Steam Audio** for THIS game.

---

## 0. VERDICT (decision-first)

**Do NOT adopt Project Acoustics for Kessel. Reject.** Three independent, each-fatal reasons:

1. **It is discontinued and has no UE 5.8 path.** Microsoft archived the repo on **2024-07-01** and
   pulled all downloads from the Microsoft Download Center and the Epic Marketplace. The last release
   (**2022.1**, 2022-11-17) targets **UE 5.0/5.1**. Reaching 5.8 means recompiling an abandoned
   third-party plugin against an engine 7 minor versions newer — and **MSVC/VS is not installed this
   phase**, so we cannot even attempt that build. This is exactly the "needs a C++ recompile →
   unvalidatable this phase" risk the brief flags.

2. **Its core assumption — static world geometry — is the inverse of our game.** Project Acoustics
   bakes a wave simulation over **static visual geometry** offline. In Kessel the dominant acoustic
   occluder is **the tank hull itself, which moves with the player**, and the world is **Chaos-
   destructible**. The one structure the bake physically *cannot* contain is the moving steel box
   around the listener's head — which is precisely the buttoned-up-vs-hatch-open effect we most need.

3. **It solves a problem we mostly don't have, and not the one we do.** Project Acoustics' headline
   win is cheap, physically-correct **diffraction/occlusion through complex *static* level geometry**
   (corridors, buildings, caves) with zero zone markup. Our level is a **blizzard whiteout on open
   terrain** with deliberately crude, abstracted geometry and short draw distance — there is very
   little rich static acoustic structure to bake. We pay all of Project Acoustics' cost (cloud/long
   local bakes, rebake-on-every-edit, static-world constraint) for the part of the problem that is
   smallest here.

**Recommendation:** pursue **Steam Audio** (dynamic-geometry, real-time, local, open-source, free) as
the propagation/occlusion engine, with the buttoned-up/hatch mix driven by our own view-mode state
machine feeding Steam Audio transmission + a MetaSound submix chain. See §7 and the dedicated Steam
Audio doc (planned `10-audio-steam-audio.md`). In-box fallbacks if Steam Audio's own 5.8 status
disappoints: UE's shipped `ResonanceAudio` / `Spatialization` / `MicrosoftSpatialAudio` plugins for
HRTF panning, plus `AudioGameplayVolume` for reverb zones and a custom occlusion DSP (§9).

---

## 1. What Project Acoustics is

A **wave-based, precomputed (baked) acoustics engine** descended from Microsoft Research's *Project
Triton*. Instead of tracing rays at runtime, it solves the **acoustic wave equation** offline over the
scene and stores a compressed field ("ACE" acoustics asset). At runtime it does cheap **table
lookups** to drive, per source→listener pair: **occlusion/obstruction**, **diffraction around
corners**, **portaling** (sound bending through doorways/openings), **decay/reverb time**, and
**arrival direction**. It feeds these as a **Source Data Override (SDO)** into the host engine's
spatializer/reverb — in UE5 it overrides UE's own sound-engine spatialization & reverb rather than
shipping its own renderer (Project Acoustics 3.0).

Its genuine strength: **physically-correct diffraction and occlusion through arbitrarily complex
static geometry, with no manual zone/portal markup and near-zero runtime CPU** (cache hit ~15 µs,
cache miss ~40 µs after the 3.0 optimizations). That is a real and impressive capability — for a game
whose acoustic interest lives in *static* architecture.

---

## 2. UE 5.8 support status — DISCONTINUED

| Fact | Detail | Source |
|---|---|---|
| Repo archived (read-only) | **2024-07-01** | github.com/microsoft/ProjectAcoustics |
| Downloads removed | After 2024-07-01, no downloads from MS Download Center or Epic Marketplace; existing licensees may keep using under EULA | Epic forums; GitHub |
| Last release | **2022.1**, 2022-11-17 | GitHub README |
| Highest UE supported | **UE 5.0 / 5.1** (marketplace listing "Unreal Engine 5/5.1") | README, Marketplace |
| UE 5.8 | **No official support. None planned (project dead).** | inference from archive |
| Local-bake tooling | `microsoft/acoustics-baketools` Docker image still exists on Docker Hub | Docker Hub |

**Implication for us:** even if we obtained the plugin binaries (forum users now trade them privately),
the editor-side integration and the runtime module are built against the 5.0/5.1 ABI. UE plugin C++
does not load across that many engine versions without a recompile, and the marketplace build that
would normally provide a 5.8 binary **no longer exists**. We would be maintaining a fork of an
abandoned plugin. **ASSUMPTION** (cannot validate — no MSVC, no editor this phase): a 5.8 recompile is
*possibly* feasible but unsupported, undocumented, and a standing maintenance liability across every
future engine bump. Not acceptable for a foundational system.

---

## 3. The baked pipeline (and why each stage is friction for us)

1. **Pre-bake** — pick which static meshes are acoustically active; auto-assign acoustic materials
   (absorption coefficients); paint navigable areas to bias **listener** probe sampling. *Friction:* in
   an open blizzard field there is little static geometry worth baking, yet you still author the whole
   probe layout.
2. **Voxelization & geometric analysis** — local step turns the static scene into a voxel grid;
   visualized in-editor to sanity-check. *Friction:* the **player tank and all enemy tanks are
   excluded** (they move) — i.e., the most important occluders are absent from the simulation.
3. **Bake (the long pole)** — voxels go to **Azure Batch** (parallel cloud VMs) *or* a **local bake**
   (separately-downloaded Docker tools; **much** slower) to solve the wave field per probe. 3.0
   claimed "2× faster bakes, 5× faster pre-bakes" for ~1000 probes — i.e., it was still measured in
   *probe-thousands and minutes-to-hours*, and "Too many probes!" is a known failure (Issue #34).
   *Friction:* every meaningful level edit (and our world is destructible) **invalidates the bake and
   forces a re-bake** — cloud cost or long local turnaround, in a one-person iteration loop.
4. **Runtime** — load ACE asset; per-frame **table lookup** drives the SDO into UE's spatializer.
   *This part is excellent and cheap* — but it is reading a field that, for us, was baked without the
   tank, without destruction, and over mostly-empty terrain.

Net: the pipeline is "lightmaps for sound." That trade (huge offline cost → tiny runtime cost, but a
**static** world) is a great deal for a fixed architectural level and a **bad** deal for a moving,
destructible, mostly-open one.

---

## 4. The buttoned-up vs. hatch-open case — where it fails hardest

This is the single most important audio feature in the design (Pillar #3: "buttoned-up vs. head-out
completely changes the mix"). Examine what physically produces it:

- **Buttoned-up:** the listener's head is inside a sealed steel hull. The hull is the occluder/low-pass
  filter; exterior threats are heavily muffled, interior sounds (reload clatter, engine, radio) are
  present and close; reverb is the tiny hard cavity of the fighting compartment.
- **Hatch-open:** the head pops above the hull ring into open air; exterior sound arrives direct and
  bright with full directionality; interior sounds drop in relative level.

**Why Project Acoustics cannot bake this:** the occluding hull is **rigidly attached to the moving
player**. Project Acoustics bakes occlusion from **static world geometry only**; dynamic objects are
not part of the wave solve. So the hull — the entire reason buttoned-up sounds different from
hatch-open — is invisible to the simulation. You would get **identical** propagation from the world in
both states, and would have to synthesize the whole buttoned/hatch difference **outside** Project
Acoustics anyway (a manual low-pass + reverb-send swap keyed to the view-mode state machine).

So for the marquee feature, Project Acoustics contributes **nothing**, and you still build the custom
view-mode-driven filter chain regardless. The hatch transition is fundamentally a **dynamic, listener-
local enclosure** problem — exactly Steam Audio's "mark the hull as dynamic geometry" wheelhouse, or
trivially a MetaSound/submix low-pass tied to `EViewMode` (ARCHITECTURE §5: `UKesselViewComponent`
already owns `bExposed`).

---

## 5. Dynamic vs. static geometry — the structural mismatch

| Dynamic element in Kessel | Project Acoustics handling | Verdict |
|---|---|---|
| **Listener (player) moves** | ✅ Supported — moving sources & listener is the *whole point* of the baked field | OK |
| **The tank hull (occluder bolted to listener)** | ❌ Not in the static bake; excluded as dynamic | **Fatal for hatch feature** |
| **Enemy tanks (moving, can be occluders/sources)** | ❌ Their hulls aren't baked occluders | Inter-tank shadowing wrong |
| **Chaos destruction / collapsing geometry** | ❌ Invalidates the bake; no live re-solve | Destructible world unsupported |
| **Doors / discrete openings** | ⚠️ "Dynamic openings" supported as limited dynamic geometry | Narrow; not our case |
| **Snow as an attenuating medium** | ❌ Not modeled (bake assumes air; absorption is per-surface, not volumetric/atmospheric) | Custom layer needed (§9) |

Project Acoustics supports a moving **listener and sources** beautifully, but **not moving
occluders** and **not changing geometry**. Kessel's defining acoustic objects are all moving occluders
and a destructible world. The mismatch is structural, not a tuning detail.

---

## 6. Bake cost and runtime cost (summary)

- **Runtime cost: excellent.** Compressed field + table lookup; 3.0 cache hit ~15 µs / miss ~40 µs;
  files compressed 2–3×. On an RTX 4090 / i9-12900K this is negligible. *(This is the one axis where
  Project Acoustics clearly beats ray-traced Steam Audio.)*
- **Bake cost: heavy and recurring.** Cloud (Azure Batch, paid, fast-ish) or local (free Docker tools,
  **much** slower). Probe counts in the thousands; hard ceilings ("Too many probes!"). Crucially the
  bake is **invalidated by geometry edits** — punishing in a solo iteration loop, and incompatible with
  a runtime-destructible world.
- **Pipeline dependency:** an external (now-unmaintained) Azure/Docker toolchain in the content
  pipeline forever. Adds operational surface for a one-person team for little payoff here.

---

## 7. Project Acoustics vs. Steam Audio — for THIS game

| Axis | Project Acoustics | Steam Audio | Winner for Kessel |
|---|---|---|---|
| Model | Wave eq., **baked** static field | Geometric **ray-traced**, real-time (+ optional baked reverb) | Steam Audio (dynamic) |
| **Moving occluders (the hull!)** | ❌ static only | ✅ mark mesh **dynamic geometry**; live occlusion/transmission (Embree) | **Steam Audio** |
| **Buttoned/hatch occlusion** | ❌ hull not baked | ✅ hull transmission, or trivial via submix | **Steam Audio** |
| **Destructible world** | ❌ rebake | ✅ updates with moved/destroyed geometry | **Steam Audio** |
| Diffraction quality (static) | ✅✅ wave-accurate, no markup | ⚠️ geometric approx (pathing/portals) | Project Acoustics (but we have little static geo) |
| Directional cue (L/R of shell impact) | ✅ baked arrival dir + HRTF | ✅ ray-traced direction + HRTF | tie (both fine) |
| Runtime CPU | ✅✅ µs lookups | ⚠️ ray tracing per frame (cheap on 4090 w/ crude geo + short draw) | Project Acoustics (but we can afford SA) |
| Bake/iteration | ❌ cloud/long local; rebake on edit | ✅ mostly real-time; optional bake | **Steam Audio** |
| **UE 5.8 status** | ❌ dead since 2024-07-01, max UE5.1, needs recompile (no MSVC) | ⚠️ actively maintained by Valve, free/open-source; confirm 5.8 build in dedicated doc | **Steam Audio** |
| Cost/licensing | EULA-restricted, abandoned | Free, open-source (Apache-2.0) | **Steam Audio** |
| Solo-dev fit | ❌ heavy offline pipeline | ✅ lighter, local | **Steam Audio** |

**Why Steam Audio fits Kessel's three hard facts:**
- **Dynamic player/occluder:** the tank hull becomes Steam Audio *dynamic geometry*; occlusion and
  **transmission** through it update in real time → the buttoned-up muffle and the hatch-open opening
  fall out naturally and respond to head position, hatch state, even hull rotation. Project Acoustics
  cannot represent this at all.
- **Blizzard:** the whiteout already mandates **short draw distance and crude, abstracted geometry**
  (Pillars #1, #4). Sparse simple meshes make real-time ray-traced audio **cheap** on a 4090 — removing
  Project Acoustics' only decisive advantage (runtime cost) while keeping Steam Audio's iteration win.
- **Crew-of-1:** a solo dev cannot afford a cloud/Docker re-bake every time the level changes. A
  real-time, local, free engine matches the iteration loop in `MILESTONE_1_PLAN`.

**Honest caveat:** Steam Audio's *static* diffraction is a geometric approximation, not wave-accurate;
Project Acoustics would sound subtly better *in a dense static interior*. Kessel is the opposite
environment, so this advantage is mostly moot here. Steam Audio's own UE 5.8 build/health must be
verified in `10-audio-steam-audio.md` (it has lagged engine releases before) — that is the one open
risk on the recommended path, and it is far smaller than reviving a dead plugin.

---

## 8. The blizzard / snow muffling — neither tool solves it natively

Falling snow as a **distance-dependent atmospheric low-pass + level attenuation** is a *medium*
property, not surface absorption or geometry. **Neither** Project Acoustics nor Steam Audio models it
out of the box. This is a **custom DSP layer regardless of engine choice**: a per-source distance →
low-pass-cutoff + gain curve in **MetaSound** (in-box; `MetaSound` ships under
`Engine/Plugins/Runtime`), scaled by a "blizzard intensity" parameter. It composes cleanly on top of
either spatializer. Because we build it ourselves either way, it is **neutral** in the
Project-Acoustics-vs-Steam-Audio decision — and it further erodes the case for adding a dead,
static-world baking dependency.

---

## 9. In-box ground truth (verified on disk, UE 5.8)

Confirmed under `C:/Program Files/Epic Games/UE_5.8/Engine/Plugins`:

- `Plugins/Runtime/MicrosoftSpatialAudio` — Microsoft Spatial Sound / ISAC HRTF (in-box).
- `Plugins/Runtime/ResonanceAudio` — Google Resonance ambisonics + HRTF (in-box).
- `Plugins/Runtime/Spatialization` — UE spatialization plugin (in-box).
- `Plugins/Runtime/AudioGameplay`, `.../AudioGameplayVolume` — gameplay-driven audio + **reverb/filter
  volumes** (in-box) → usable for cheap buttoned/hatch reverb zones and occlusion volumes with **zero**
  third-party dependency.
- `Plugins/Runtime/AudioModulation`, `AudioSynesthesia`, `AudioCapture`, `AudioWidgets`,
  `AudioMotorSim`, plus `AudioInsights*` debugging — all in-box.
- NOTE: the `Steam` plugin on disk is the **Steamworks online subsystem**, *not* Steam Audio. **Steam
  Audio is NOT in-box** — it is a separate Valve plugin to be vetted in `10-audio-steam-audio.md`.
- NOTE: MetaSound ships in-box (verify exact path under `Plugins/Runtime` at build time — ASSUMPTION
  on path string only; presence is certain for 5.8).

**Fallback ladder if Steam Audio's 5.8 build disproves:** (a) `ResonanceAudio`/`Spatialization`/
`MicrosoftSpatialAudio` for HRTF directionality + (b) `AudioGameplayVolume` for reverb/occlusion zones
+ (c) a MetaSound submix chain for the buttoned/hatch low-pass and the blizzard medium filter, all
driven by `UKesselViewComponent`'s `EViewMode`. This is **100% in-box**, ships with no abandoned
dependency, and already delivers the directional + buttoned/hatch + snow goals at "abstracted
presentation" fidelity — at the cost of Steam Audio's true dynamic ray-traced occlusion.

---

## 10. Open items / to verify later (post-MSVC, in-editor)

1. Steam Audio official UE **5.8** binary/source status, dynamic-geometry cost on crude blizzard geo,
   and Embree availability — the deciding factor; owns `10-audio-steam-audio.md`.
2. Whether in-box `AudioGameplayVolume` occlusion + MetaSound LPF alone is "good enough" for the
   buttoned/hatch feel before committing to any third-party propagation engine (cheapest path).
3. MetaSound exact on-disk plugin path under 5.8 (path string only; presence certain).
4. HRTF choice among in-box `MicrosoftSpatialAudio` / `ResonanceAudio` / `Spatialization` for the
   left-ear/right-ear shell-impact cue.

---

## Sources

- Project Acoustics for Unreal — UE Marketplace: https://www.unrealengine.com/marketplace/en-US/product/project-acoustics-for-unreal-audio
- microsoft/ProjectAcoustics (GitHub, archived 2024-07-01): https://github.com/microsoft/ProjectAcoustics
- ProjectAcoustics README: https://github.com/microsoft/ProjectAcoustics/blob/main/README.md
- Project Acoustics 3.0 announcement (bake/pre-bake speedups, SDO, runtime cache µs): https://developer.microsoft.com/en-us/games/articles/2022/08/project-acoustics-30-is-now-available/
- Project Acoustics 2.0 (dynamic openings, faster bakes): https://www.neowin.net/news/microsoft-announces-project-acoustics-20-with-faster-bake-times-dynamic-openings-and-more/
- Project Triton (Microsoft Research — wave-sim lineage): https://www.microsoft.com/en-us/research/project/project-triton/
- "Plugin removed from marketplace, share files?" (Epic forums — discontinuation): https://forums.unrealengine.com/t/project-acoustics-could-anyone-share-the-plugin-files-since-its-been-removed-from-marketplace/2295631
- Local vs Azure bake (Microsoft Q&A; local Docker baketools): https://learn.microsoft.com/en-us/answers/questions/2073061/using-microsoft-azure-to-bake-acoustics
- microsoft/acoustics-baketools (Docker, local bake): https://hub.docker.com/r/microsoft/acoustics-baketools
- "Too many probes!" bake limit (Issue #34): https://github.com/microsoft/ProjectAcoustics/issues/34
- Steam Audio dynamic geometry (occlusion/transmission, Embree): https://valvesoftware.github.io/steam-audio/doc/unity/geometry.html
- On-disk verification: `C:/Program Files/Epic Games/UE_5.8/Engine/Plugins/Runtime/{MicrosoftSpatialAudio,ResonanceAudio,Spatialization,AudioGameplay,AudioGameplayVolume,...}`
</content>
</invoke>
