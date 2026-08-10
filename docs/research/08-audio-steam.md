# 08 — Valve Steam Audio for UE 5.8 (acoustic-simulation engine evaluation)

Status: RESEARCH / DESIGN (no code; editor not launched; nothing built — MSVC absent this phase).
Engine: UE 5.8.0 at `C:/Program Files/Epic Games/UE_5.8`.
Pillar served: **#3 "Sound is the primary sensor"** and the governing **"deep simulation,
abstracted presentation"** — acoustics are a *system*, the highest-payoff place to spend simulation
budget, and Steam Audio is the in-engine candidate for the user's "ray-traced audio".

Scope of this doc: does Steam Audio support 5.8, how you install it, what it can actually do, how it
maps onto the four USER AUDIO VISION cases, and the concrete compatibility/build risk for this
machine. A full audio architecture (submix graph, MetaSound layout, view-state mixing) is a separate
doc — this only evaluates Steam Audio as the propagation engine. NOTE: `DESIGN_PILLARS.md` references
`AUDIO_ARCHITECTURE.md`, which **does not yet exist** — flag to author it next.

---

## 0. Headline finding (read this first)

There are **two different Steam Audios**, and the distinction decides the whole recommendation:

1. **In-box, shipped with UE 5.8 — DEPRECATED, do NOT build on it.**
   On disk at `C:/Program Files/Epic Games/UE_5.8/Engine/Plugins/Runtime/Steam/SteamAudio`.
   The `.uplugin` is explicit (`SteamAudio.uplugin:4-6`):
   - `"VersionName" : "2.0-beta.17"`
   - `"FriendlyName" : "Steam Audio (Deprecated)"`
   - `"Description" : "This plugin is deprecated and will be removed in a future engine release.
     Please use the plugin from Valve's website."`
   This is the ancient **Phonon-era 2.x** integration. `SteamAudio.Build.cs:54,60-62` links
   `libPhonon`, `DX11Audio`, and delay-loads `GPUUtilities.dll` + `tanrt64.dll` (AMD TrueAudio
   Next) — the old SDK. It is `Win64`+`Android` **only** (`SteamAudio.uplugin:23-30`,
   `PlatformArchitectureDenyList: Win64:arm64`): **no Linux, no macOS**. A prebuilt editor DLL
   exists (`Binaries/Win64/UnrealEditor-SteamAudio.dll`) so it loads without a rebuild, but it
   lacks the modern feature set and is a removal-scheduled dead end. **Verdict: ignore it.**

2. **Modern, from Valve's site/GitHub — the real product, latest `4.8.1` (Feb 2025).**
   This is the "ray-traced audio / acoustic-simulation engine" the user wants: real-time + baked
   physics propagation, transmission, pathing, HRTF, materials. It is a **separate download** you
   drop into the project's `Plugins/` folder; it shadows/replaces the in-box one. **This is what we
   evaluate and recommend** — with a hard 5.8/MSVC caveat (§7).

So "Steam Audio ships in UE 5.8" is technically true but a trap: the in-box copy is the wrong,
deprecated one. The capable plugin must be fetched from Valve.

---

## 1. Version & 5.8 compatibility

- **Latest release:** Steam Audio **4.8.1**, dated **2025-02-11** (GitHub releases). The 4.x line
  is the modern SDK (4.6.0 added the Wwise plugin; 4.6.0 deprecated Win32).
- **Stated engine floor:** "Steam Audio requires **Unreal Engine 4.27 or later**" (Getting Started).
  There is **no published statement that 4.8.1 explicitly supports 5.8** — and it can't have been
  tested against it, because **4.8.1 (Feb 2025) predates UE 5.8**. Treat 5.8 support as
  **ASSUMPTION / unverified**.
- **The concrete risk** is the standard UE prebuilt-binary ABI gate: Valve's prebuilt plugin
  binaries are compiled against a *specific* engine version. Opening them under a newer engine throws
  *"The following modules are missing or built with a different engine version"* — this exact failure
  is documented against UE 5.1 in Steam Audio issue #341. The fix is to **rebuild the plugin from
  source against 5.8**, which needs a C++ toolchain. (See §7.)

---

## 2. Install method

Modern plugin (the one we want):
1. Download `steamaudio_unreal.zip` (or `_X.Y.Z`) from Valve's downloads page / GitHub release
   assets — **prebuilt binaries are provided** (no build needed *if* they match the engine).
2. Extract; copy the `SteamAudio` plugin folder into `<Project>/Plugins/`. A project-level plugin
   overrides the in-box engine copy.
3. Enable the plugin; in **Project Settings → Platforms → Windows/Linux → Audio**, set the
   **Spatialization / Occlusion / Reverb plugins** to "Steam Audio" so it hooks Unreal's audio mixer.
4. Tag world geometry with a **Steam Audio Geometry** component + **acoustic materials**, place
   **probe volumes** for baking, add a **Steam Audio Listener**, and set per-source **Steam Audio
   Source** settings.

Build-from-source path (needed if prebuilt ≠ 5.8): clone the repo, run their build (CMake for the
native `phonon` core + UBT for the UE module), point the project at the rebuilt binaries. **Blocked
this phase — MSVC/VS still installing (§7).**

---

## 3. Capabilities (modern 4.x) — what it actually simulates

All confirmed from Valve's Unreal User's Guide / Settings / Occlusion-Settings docs:

| Capability | What it does | Real-time / baked |
|---|---|---|
| **HRTF binaural spatialization** | Head-related transfer functions place a source in true 3D — azimuth + elevation, front/back. The core L/R "it's in my left ear" effect. **Headphone-optimized.** | real-time |
| **Distance attenuation + air absorption** | Volume falls with distance; **high frequencies roll off faster than low** over air → distant sounds get muffled/darker automatically. | real-time |
| **Source directivity** | Sources radiate unevenly (e.g. a muzzle/exhaust projecting forward). | real-time |
| **Occlusion** | Ray/volumetric test for geometry blocking the *direct* path → attenuates/filters the dry sound. | real-time |
| **Transmission** | Sound that *passes through* occluding geometry, attenuated **per-frequency by the material's transmission coefficients** (low end leaks through a wall/hull, highs don't). | real-time |
| **Reflections / reverb (the "ray-traced" part)** | Rays are **traced and bounced through the scene each frame** from source→listener to build early reflections + late reverb; **directional** (reverb has a direction), responds to moving geometry (an opening hatch changes it). This is the user's "ray-traced audio". | real-time **or** baked |
| **Pathing / portaling** | Finds how sound **bends around corners and through openings** (door/window/hatch) when there is no straight line — positions the sound at the correct opening. Baked into probe volumes for cheap runtime lookup. | baked (probes) |
| **Acoustic materials** | Per-geometry, **frequency-dependent** absorption / scattering / transmission coefficients (snow, steel, concrete…). Drives muffle, reflection, and transmission realism. | both |
| **Ambisonics** | Higher Ambisonic-order IRs = more directional reflection detail at higher CPU. | both |

This is a genuine **physics-based acoustic simulator**, not a reverb-zone fake — exactly the depth
the "deep simulation" pillar asks for, and it stays invisible to the abstracted *visual* style.

---

## 4. CPU / GPU cost on THIS machine (RTX 4090 + i9-12900K)

- **GPU acceleration is AMD-only and does not help a 4090.** Steam Audio's GPU path = **Radeon Rays**
  (OpenCL, for ray tracing the indirect sim) + **AMD TrueAudio Next** (GPU convolution reverb), and
  Radeon Rays support in Steam Audio is **Windows-64 only**, designed around AMD's reserved-queue
  driver feature. On an NVIDIA 4090 you get **no Steam Audio GPU offload** — it runs **CPU-only**.
  The 4090's 24 GB / RT cores buy nothing for acoustics here. (The old in-box plugin's `tanrt64.dll`
  delay-load is the same AMD TAN path — also moot on NVIDIA.)
  - ASSUMPTION: Radeon Rays nominally runs on "any OpenCL 1.2 GPU", but Steam Audio's shipped GPU
    accel is built and validated for AMD; do not plan on NVIDIA GPU acoustics.
- **CPU cost is the real budget, and it's favorable for this game.** Cost scales with: number of
  sources doing reflections, **IR (impulse-response) duration**, and **Ambisonic order**. The
  expensive case is "many sources × long real-time reverb × high order". Kessel is the *cheap* case:
  **one listener, a handful of meaningful sources** (own gun/engine/tracks, 1–few enemy tanks, a
  wreck fire), short outdoor-blizzard reverb tails. The i9-12900K handles real-time reflections for
  this comfortably. ASSUMPTION: no measured numbers until we profile in-editor (MSVC-gated).
- **Use baking to cut cost where geometry is static.** Bake reflections/reverb **and pathing** into
  probe volumes for the static world (buildings, wrecks, terrain) → near-free runtime lookup; reserve
  real-time only for dynamic occluders (a moving enemy tank between you and a source, your own
  rotating hull/hatch). This baked/real-time split is the standard Steam Audio perf pattern and fits a
  mostly-static snow battlefield.

---

## 5. Mapping the four USER AUDIO VISION cases → concrete Steam Audio features

| User intent | Steam Audio mechanism | Notes / design action |
|---|---|---|
| **Snow muffles / quietens sound** | **Acoustic material** with high absorption + high transmission-loss assigned to the snow terrain/ground, **+ air absorption** (HF rolloff over distance). Highly-absorptive ground kills reflections → low, dead reverb = the muffled whiteout. | Author a "snow" material (low scattering, high HF absorption). Reinforce globally with a low, dark baked reverb. ASSUMPTION: snow coefficients are hand-authored; no stock "snow" preset. |
| **Positional combat — shell impact on the left bangs in the left ear** | **HRTF binaural spatialization** of the direct path (Steam Audio Source). Azimuth/elevation cues put the bang in the correct ear; **directional reflections** add the "it came from over there" tail. | This is the core, out-of-the-box win. **Headphone-first** (HRTF assumes headphones) — consistent with sound-as-primary-sensor. Provide a speaker/stereo fallback path. |
| **Head-out-of-hatch (open air) vs buttoned-up (muffled interior)** | Tie to the **view-mode state machine** (`UKesselViewComponent::EViewMode`, doc 07). Buttoned: listener sits **inside the tagged hull geometry** → exterior sources reach it via **occlusion + transmission** (steel material = strong HF loss, bass leaks) and reflections are the interior's. Hatch-open: listener at the **hatch socket in open air** → direct paths + full outdoor reflections, no hull transmission. | **Hybrid recommended.** Steam Audio gives the *physical* muffle (transmission through a tagged hull + listener-inside test); reinforce the *felt* tonal jump with a **submix EQ/reverb state** switched by `bExposed`. Steam Audio alone won't "know" you're buttoned unless the hull is real geometry and the listener point is genuinely inside it — so drive it explicitly from the state machine (move the Steam Audio Listener between an interior socket and the hatch socket on view change). |
| **Occluded threats — an enemy tank you can't see** | **Occlusion** (direct path blocked by a wreck/building → attenuated/filtered) + **Transmission** (muffled bleed-through) + **Pathing** (sound bends around cover and arrives from the correct *opening*, not through the wall). | The killer feature for "hear the war before you see it" in a whiteout. **Bake pathing** into probe volumes around static cover so an unseen enemy is heard at the right gap; real-time occlusion handles dynamic blockers. Directionally honest threat audio = the gameplay payoff. |

Supporting cast the user listed (reload, engine, track, collision) are ordinary **Steam Audio
Sources** that automatically inherit spatialization + occlusion + reflections once the system is
wired; no special handling.

---

## 6. Fit with the rest of the engine / native audio baseline

- Steam Audio hooks **Unreal's Audio Mixer** as the Spatialization/Occlusion/Reverb plugin and
  coexists with **MetaSound** (in-box: `.../Runtime/Metasound/Metasound.uplugin`) sources and
  **AudioGameplay** (`.../AudioGameplay/AudioGameplay.uplugin`). So the design is: **MetaSound**
  authors/varies the sound (engine RPM from the `FVehicleEngineConfig` curve, reload one-shots,
  procedural track clatter), **Steam Audio** propagates it through the world.
- In-box native alternatives that exist **today** without Steam Audio (relevant because MSVC is
  absent — §7): `Spatialization.uplugin`, `SoundFields.uplugin` (ambisonics), `ResonanceAudio.uplugin`
  (Google Resonance binaural+reverb), `Synthesis.uplugin`, `AudioWidgets.uplugin`. These give
  binaural panning + occlusion traces + reverb submixes, but **not** ray-traced reflections,
  transmission, or pathing — i.e. they cover the L/R and basic muffle cases but not the deep
  occlusion/pathing sim.

---

## 7. Risks & blockers (be explicit)

1. **5.8 prebuilt mismatch → source rebuild → MSVC absent (this phase's blocker).** Steam Audio
   4.8.1 predates 5.8; its prebuilt binaries almost certainly won't load under 5.8 ("modules built
   with a different engine version", cf. issue #341). The remedy is rebuilding the plugin from source
   against 5.8 — and **VS/MSVC is still installing**, so this **cannot be validated this phase**.
   Same gate as the rest of the C++ architecture (doc 01 §0). ASSUMPTION until tested: 4.8.1 prebuilt
   does **not** drop cleanly into 5.8.
2. **Do not use the in-box 2.0-beta.** It's deprecated, removal-scheduled, Phonon-era, and missing
   the modern transmission/pathing UX. Building the audio design on it is throwaway work.
3. **No 4090 GPU acceleration.** AMD-only (Radeon Rays + TrueAudio Next). Budget acoustics as a
   **CPU** cost on the i9-12900K. Fine for this game's source count, but don't promise GPU acoustics.
4. **Linux.** The *modern* plugin supports Linux-64, but a 5.8 rebuild for Linux additionally needs
   the `v26_clang-20.1.8-rockylinux8` cross-toolchain + `LINUX_MULTIARCH_ROOT` (doc 01 §1). The
   *in-box* plugin has **no Linux** at all. Plan Linux acoustics around the modern plugin only.
5. **HRTF = headphones.** Binaural is optimized for headphones; speaker users get a degraded cue.
   Acceptable given the sound-as-sensor pillar, but ship a stereo/speaker fallback mix.
6. **Integration maintenance lag.** Steam Audio's UE integration historically trails new engine
   releases (the in-box copy being abandoned is itself evidence). Budget integration time and verify
   MetaSound ↔ Steam Audio source-effect routing once it builds.
7. **`AUDIO_ARCHITECTURE.md` is missing** though the pillars cite it — author the submix graph,
   view-state mixing, and MetaSound layout there; this doc only covers the propagation engine.

---

## 8. Recommendation

**Adopt modern Steam Audio (Valve 4.8.1+) as Kessel's acoustic-simulation engine — it maps 1:1 onto
every USER AUDIO VISION case and is the in-engine answer to "ray-traced audio". But sequence it
correctly:**

- **Now (M1, MSVC absent, placeholder SFX):** build a **plugin-agnostic audio layer** on the in-box
  native stack — **MetaSound** sources + **AudioGameplay** + built-in binaural **Spatialization**
  (or **ResonanceAudio**) + **submix EQ/reverb states driven by `UKesselViewComponent` view mode**
  (this alone delivers the buttoned-vs-hatch tonal flip and basic L/R + occlusion-trace muffle). Wire
  the engine/track/reload/collision sources and the view-state mix. This is real, shippable M1 audio
  with **zero** Steam Audio dependency, and it de-risks the schedule.
- **Architect for drop-in:** keep sources and listener behind a thin interface so swapping the
  spatialization/occlusion/reverb provider to Steam Audio is a settings + tagging change, not a
  rewrite. Author world geometry knowing acoustic materials + probe volumes are coming.
- **Post-M1 audio milestone (after MSVC installs):** fetch the modern plugin, **rebuild from source
  against 5.8** (Win64 first, then Linux with the clang cross-toolchain), verify it loads, profile
  CPU on the i9-12900K, then turn on — in payoff order — **occlusion+transmission** (hull/hatch +
  occluded threats), **HRTF** (positional), **acoustic materials** (snow muffle), **baked
  reflections + pathing** (static cover/threat direction), **real-time reflections** last for dynamic
  occluders. Bake aggressively; the battlefield is mostly static.

Net: Steam Audio is the right engine for the vision and deepens the highest-value system in the game,
but it is **gated on MSVC + a 5.8 source rebuild**, so it is a **post-M1 integration**, not an M1
dependency. Ship M1 on the native stack behind an abstraction; land Steam Audio when the toolchain
allows.

---

## 9. Sources

- Steam Audio UE Getting Started: https://valvesoftware.github.io/steam-audio/doc/unreal/getting-started.html
- Steam Audio UE User's Guide: https://valvesoftware.github.io/steam-audio/doc/unreal/guide.html
- Steam Audio UE Occlusion Settings: https://valvesoftware.github.io/steam-audio/doc/unreal/occlusion-settings.html
- Steam Audio UE Settings: https://valvesoftware.github.io/steam-audio/doc/unreal/settings.html
- Steam Audio Releases (4.8.1, 2025-02-11): https://github.com/ValveSoftware/steam-audio/releases
- UE version-mismatch failure (issue #341, UE 5.1): https://github.com/ValveSoftware/steam-audio/issues/341
- AMD TrueAudio Next / Radeon Rays GPU acceleration (AMD-only): https://gpuopen.com/learn/beyond-spatial-audio/ , https://gpuopen.com/news/trueaudio-next-now-integrated-steam-audio/
- Epic in-box SteamAudio API page: https://dev.epicgames.com/documentation/unreal-engine/API/Plugins/SteamAudio
- On-disk truth (UE 5.8): `Engine/Plugins/Runtime/Steam/SteamAudio/SteamAudio.uplugin` (lines 4-6, 23-30), `.../Source/SteamAudio/SteamAudio.Build.cs` (lines 54, 56-63)
