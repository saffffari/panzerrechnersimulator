# Kessel — AUDIO ASSET INVENTORY & NATIVE WIRING PLAN

Status: **INVENTORY (read-only audit)** — no source edited, no editor launched, no build run this pass.
Scope: catalogue every audio file on disk, map it to the `UKesselAudioComponent` seams, and give a
**native UE 5.8-only** wiring plan for Milestone 1 (no Steam Audio / no new plugins).

This is the pragmatic raw-asset companion to `docs/AUDIO_ARCHITECTURE.md`. That doc owns the long-term
MetaSounds + submix + modulation design ("Pillar 3 — sound is the primary sensor"); **this** doc owns the
question *"what sound files do we actually have right now, and how do we make the tank make noise in the
cheapest possible way?"* When the two disagree, ARCHITECTURE wins long-term; this wins for the M1 thin pass.

---

## 0. TL;DR

| Question | Answer |
|---|---|
| **How many audio files on disk?** | **40** usable source files, all **inside two ZIPs** in `C:\shop\ts_assets` (nothing extracted or imported yet). |
| **Where?** | `Echoes - Audio Super Kit [Demo].zip` (12 × MP3) and `Rust & Blood - SFX Library v1.1d [Sample].zip` (28 × OGG). |
| **Categories covered?** | Engine/machine loop, gun fire, explosion, shell impact, reload/racking, footsteps (incl. snow/metal), ambience loops, GUI, music. **Missing:** a true tank main-gun report, a dedicated track/tread loop, a wind/blizzard bed, an incoming-shell whistle (all "record later"). |
| **The 3 priority seams (engine / fire / impact)?** | **All three have a usable source.** Engine → `machine_10_loop.mp3`; Fire → `explosion_close_short_ufx_2.ogg`; Impact → `explosion_close_long_ufx_7.ogg` / `impact_concrete_ufx_1.ogg`. |
| **Seam callers already in code?** | `OnFire()`, `OnReload()`, `OnTrack()`, `OnImpact()` **are called**. `OnEngineRPM()`, `OnCollision()`, `SetButtonedState()` are **defined but never called**. |
| **Build.cs change needed?** | **No.** `Engine` is already a dependency (`Kessel.Build.cs:16`); it covers `UGameplayStatics` + `UAudioComponent` + `USoundBase`. Native audio needs zero module change. |
| **The pizzadoggy "bundle with sounds"?** | **Not on disk.** `pizza_assets/The Humble Bundle Key.zip` holds only a thumbnail, a logo PNG, and a 443-byte redemption key `.txt` — the bundle has not been redeemed/downloaded. No pizzadoggy audio exists yet. |
| **Import gotcha** | Files are raw MP3/OGG. UE 5.8's stock SoundWave importer ingests **16-bit PCM WAV**; convert MP3/OGG → WAV before import (the *full* Echoes pack ships WAV — the demo is MP3-only). |

---

## 1. Inventory

**Critical fact: there are zero loose/imported audio files anywhere in the project or asset trees.**
Every sound lives inside a ZIP. `Glob **/*.{wav,ogg,mp3,flac,aif,aiff}` returns nothing across
`ts_assets`, `pizza_assets`, `references`, `Content`, and `RawAssets`. The listings below were read from
the archives without extracting them (`unzip -l`).

### 1.1 `Echoes - Audio Super Kit [Demo]` — the "Echoes" pack
Archive: `C:\shop\ts_assets\Echoes - Audio Super Kit [Demo].zip` (≈7.1 MB, 24 entries, **12 audio**).
**Format: MP3 (demo build).** The full retail kit ships WAV + OGG — note this when the full pack lands.
Internal path prefix: `Echoes - Audio Super Kit [Demo]/mp3/`.

| Category | File (under `…/mp3/`) | Size |
|---|---|---|
| **Engine / machine (LOOP)** | `SFX/Machines/machine_10_loop.mp3` | 538 KB |
| **Ambient (LOOP)** | `Ambiences/ambience_d6_loop.mp3` | 1.05 MB |
| **Ambient (LOOP)** | `Ambiences/ambience_d22_loop.mp3` | 1.05 MB |
| **Explosion** | `SFX/Explosions/explosion_distant_4.mp3` | 403 KB |
| **Shell impact** | `SFX/Impacts/impact_distant_6.mp3` | 136 KB |
| **Atmos / horror** | `Bonus/Pure Horror/phantom_1.mp3` | 538 KB |
| **UI** | `GUI/gui_click_1.mp3`, `gui_click_2.mp3` | 16 KB ea |
| **UI** | `GUI/gui_click_3.mp3` | 27 KB |
| **UI** | `GUI/gui_submit_9.mp3` | 70 KB |
| **Music (LOOP)** | `Soundtracks/pitch black (middle_loop).mp3` | 1.29 MB |
| **Music (LOOP)** | `Soundtracks/subliminal (middle_loop).mp3` | 1.45 MB |

### 1.2 `Rust & Blood - SFX Library [Sample]` — the SFX workhorse
Archive: `C:\shop\ts_assets\Rust & Blood - SFX Library v1.1d [Sample].zip` (≈8.2 MB, **28 audio**).
**Format: OGG.** Internal path prefix: `Rust & Blood - SFX Library [Sample]/OGG/Sound Effects/`
(music under `…/OGG/Bonus/OST/`).

| Category | File (under `…/OGG/Sound Effects/`) | Size |
|---|---|---|
| **Gun fire** | `Firearms/Fire/assault_rifle_fire_ufx_1.ogg` | 49 KB |
| **Gun fire (supp.)** | `Firearms/Fire/assault_rifle_fire_suppressed_ufx_1.ogg` | 32 KB |
| **Explosion / blast** | `Explosions/explosion_close_short_ufx_2.ogg` | 78 KB |
| **Explosion / blast** | `Explosions/explosion_close_long_ufx_7.ogg` | 157 KB |
| **Reload (in)** | `Firearms/Reloading/assault_rifle_reloading_mag_in_ufx_1.ogg` | 20 KB |
| **Reload (out)** | `Firearms/Reloading/assault_rifle_reloading_mag_out_ufx_1.ogg` | 38 KB |
| **Breech / racking** | `Firearms/Racking/pistol_racking_ufx_2.ogg` | 29 KB |
| **Empty / dry-fire** | `Firearms/Misc/rifle_universal_empty_barrel_ufx_2.ogg` | 29 KB |
| **Impact (concrete)** | `Impact & Break/Concrete/impact_concrete_ufx_1.ogg` | 37 KB |
| **Impact (body)** | `Impact & Break/Body/impact_body_ufx_2.ogg` | 14 KB |
| **Break (wood)** | `Impact & Break/Wood/break_wood_ufx_1.ogg` | 72 KB |
| **Footsteps (6 surfaces)** | `Player/Footsteps/{Snow,Metal,Gravel,Dirt,Concrete,Wood}/footstep_*_ufx_*.ogg` | 22–35 KB ea |
| **Mechanical / craft** | `Misc/item_craft_ufx_1.ogg`, `item_craft_ufx_3.ogg` | 19 / 34 KB |
| **Melee (8 files)** | `Melee/{Draw,Throw,Swing,Hit}/…` (knife draw/swing, throws, `blunt_hit_bloody_*` ×4) | 13–47 KB ea |
| **Music (LOOP)** | `Bonus/OST/1998_ufx_1_loop.ogg` | 3.68 MB |

> `footstep_snow_ufx_3.ogg` and `footstep_metal_ufx_3.ogg` are the most Kessel-relevant footsteps
> (snow crunch + walking on the hull). The 8 melee files are largely irrelevant to a tank sim but are
> on disk, so they are catalogued for completeness.

### 1.3 Things that look like audio but are NOT (red herrings)
Checked and confirmed **zero audio entries** inside each:

- `Heavy Weapons Pack.zip` — Blender **models** (`bazooka/grenadelauncher/knife/mauser/weaponPack` `.blend`/`.fbx`/`.png`). No sounds despite the name.
- `Train and Railway.zip` — a train **model** (`trainpack.blend`/`.fbx`) + 2 textures. No sounds.
- `PSX Tech [Sample].zip`, `PSX Mega Pack [Demo].zip`, `PSX Mega Pack II [Demo].zip`, `PSX Textures [Demo].zip`, `PSX Textures II [Demo].zip`, `Roads.rar` — visual/material packs, 0 audio.
- `pizza_assets\The Humble Bundle Key.zip` — `the_humble_bundle_thumbnail.png`, `Pizza Doggy's.png`, `The Humble Bundle Key.txt` (443 B redemption key). **The pizzadoggy bundle is not yet downloaded; no sound files.**

### 1.4 Empty / image-only locations
- `C:\shop\tank_simulator\RawAssets\Audio\` — exists but **empty** (the intended landing folder for imports).
- `C:\shop\ts_assets\New folder\` — empty. `C:\shop\ts_assets\_extracted\` — only meshes/textures, no audio.
- `C:\shop\tank_simulator\references\` — reference **images** only (jpg/png/gif, incl. `pizzadoggy_refs/` screenshots).
- `C:\shop\tank_simulator\Content\` — **no imported sound assets** (only the `L_KesselTest` level + dev collections).

---

## 2. Seam reality check — what `UKesselAudioComponent` exposes vs what is wired

Interface: `Source/Kessel/Public/KesselAudioComponent.h`. Implementation: `…/Private/KesselAudioComponent.cpp`
(every method is currently an empty `// TODO(audio workflow)` stub). The grep below is the ground truth
on which seams already have a **caller** in gameplay code:

| Seam (signature) | Caller in code? | Where | Notes |
|---|---|---|---|
| `OnFire()` | **YES** | `KesselGunComponent.cpp:89` (in `Fire()`, via `Owner->FindComponentByClass`) | Fires on the **pawn** that shot. |
| `OnReload()` | **YES** | `KesselGunComponent.cpp:102` (in `OnReloadComplete()`) | Single event at reload **completion** (not per mechanical step). |
| `OnTrack(float Speed)` | **YES** | `KesselTankPawn.cpp:184` (every tick, `CurrentSpeed` in uu/s) | The only **per-tick** movement-audio hook that is actually called. |
| `OnImpact(FVector WorldDir)` | **YES** | `KesselProjectile.cpp:147` (on shell hit) | Resolved on the **HitActor**'s component, not the shooter. `WorldDir` is a **direction**, not a point (see §3.2). |
| `OnEngineRPM(float RPM)` | **NO** | — | Defined, never called. The pawn drives `OnTrack` but never `OnEngineRPM`. |
| `OnCollision(float Force)` | **NO** | — | Defined, never called. |
| `SetButtonedState(bool)` | **NO** | — | Defined, never called (the view component would be the caller). |

**Consequence for M1:** the engine note has a *source file* but **no upstream caller** on its dedicated seam.
The cheapest path is to **ride the already-firing `OnTrack(Speed)`** for both the engine loop pitch and the
tread, and leave `OnEngineRPM` for when the movement component is taught to call it (a later code edit,
out of scope for this read-only pass).

---

## 3. Gap analysis — seam → available source

| Seam | Best available source today | Coverage |
|---|---|---|
| `OnFire()` | `Rust&Blood/.../Explosions/explosion_close_short_ufx_2.ogg` (best blast timbre for a tank gun) — fallback `Firearms/Fire/assault_rifle_fire_ufx_1.ogg` | **COVERED** |
| `OnImpact()` | `Rust&Blood/.../Explosions/explosion_close_long_ufx_7.ogg` or `Impact & Break/Concrete/impact_concrete_ufx_1.ogg`; Echoes `SFX/Impacts/impact_distant_6.mp3` for distant | **COVERED** |
| `OnTrack()` / engine bed | Echoes `SFX/Machines/machine_10_loop.mp3` (looping) as the diesel bed; `footstep_metal_ufx_3.ogg` retriggered as tread clank | **COVERED (proxy)** |
| `OnEngineRPM()` | same `machine_10_loop.mp3` | **SOURCE OK — seam has no caller** (drive via `OnTrack` for M1) |
| `OnReload()` | `Rust&Blood/.../Firearms/Reloading/assault_rifle_reloading_mag_in_ufx_1.ogg` (or `Racking/pistol_racking_ufx_2.ogg` for a breech-clack) | **COVERED** |
| `OnCollision()` | `impact_concrete_ufx_1.ogg` / `break_wood_ufx_1.ogg` (scale gain by `Force`) | **SOURCE OK — seam has no caller** |
| `SetButtonedState()` | No sample needed (it's a *mix* change: native LPF/gain duck). Open-hatch bed = Echoes `ambience_d6_loop.mp3`. | **SEAM HAS NO CALLER — native LPF later** |

**Genuinely missing (record later, per `AUDIO_ARCHITECTURE.md §11`):** a real 8.8 cm **main-gun report**
(we are proxying with a close explosion), a dedicated **track/tread loop**, a **wind/blizzard** bed
(the Echoes ambiences are generic loops, usable as a stand-in), and the **incoming-shell whistle** (none on disk).

---

## 4. Concrete wiring plan (native UE 5.8 only — NO new plugins)

### 4.0 Module / Build.cs — no change required (confirmed)
`Kessel.Build.cs:16` already lists `"Engine"` in `PublicDependencyModuleNames`. That single module provides:
- `UGameplayStatics::PlaySoundAtLocation` / `SpawnSoundAttached` — `Engine/Classes/Kismet/GameplayStatics.h`
- `UAudioComponent` (looping engine note) — `Engine/Classes/Components/AudioComponent.h`
- `USoundBase` / `USoundWave` (the asset type the seams will reference) — `Engine/Classes/Sound/SoundBase.h`

So the only code-side additions in the later C++ pass are **`#include` lines**, not module edits. (If the
project ever moves to the MetaSounds C++ API or `AudioModulation` C++, *those* need module additions — but
the M1 `PlaySound`/`UAudioComponent` path does not.)

### 4.1 Import step (one-time, editor — done by the user or a later pass)
The files are raw archives. Before any wiring:
1. **Extract** the two ZIPs (e.g. into `RawAssets/Audio/Echoes` and `RawAssets/Audio/RustAndBlood`).
2. **Convert MP3/OGG → 16-bit PCM WAV.** UE 5.8's stock importer is happiest with PCM WAV; MP3/OGG import
   is not guaranteed in-editor. One-liner per file, e.g.:
   `ffmpeg -i machine_10_loop.mp3 -ar 48000 -ac 2 -c:a pcm_s16le machine_10_loop.wav`
   (The *full* Echoes kit already ships WAV — only the demo build forces this conversion.)
3. **Import** the WAVs as `USoundWave` into `/Game/Audio/...` (§5). For the engine + ambience loops, tick
   **Looping** on the imported `USoundWave` (and author them as seamless loops).

> This is an **editor import** — it cannot be scripted from this read-only pass. The wiring in §4.2–4.4 is
> the *plan* the later code/editor pass executes; it is written as illustrative C++, not applied.

### 4.2 One-shots — `OnFire()`, `OnImpact()`, `OnReload()`, `OnCollision()`
Add `EditAnywhere` `TObjectPtr<USoundBase>` properties to `UKesselAudioComponent` (so swapping placeholder →
real SFX is a data edit), then play them at a world location:

```cpp
// KesselAudioComponent.h  (new UPROPERTYs, Category="Kessel|Audio")
UPROPERTY(EditAnywhere) TObjectPtr<USoundBase> FireSound;     // explosion_close_short_ufx_2
UPROPERTY(EditAnywhere) TObjectPtr<USoundBase> ImpactSound;   // explosion_close_long_ufx_7 / impact_concrete
UPROPERTY(EditAnywhere) TObjectPtr<USoundBase> ReloadSound;   // assault_rifle_reloading_mag_in (or pistol_racking)
UPROPERTY(EditAnywhere) TObjectPtr<USoundBase> CollisionSound;// impact_concrete / break_wood

// KesselAudioComponent.cpp  (#include "Kismet/GameplayStatics.h")
void UKesselAudioComponent::OnFire()
{
    if (FireSound && GetOwner())
        UGameplayStatics::PlaySoundAtLocation(this, FireSound, GetOwner()->GetActorLocation(), MasterVolume);
}
void UKesselAudioComponent::OnImpact(FVector WorldDir)
{
    if (!ImpactSound || !GetOwner()) return;
    // NOTE: WorldDir is a direction, not a point. M1: play at the hit actor's origin, nudged along the dir
    // so panning reads the side the round came from. A later seam tweak can pass the true impact point.
    const FVector Loc = GetOwner()->GetActorLocation() + WorldDir.GetSafeNormal() * 150.f;
    UGameplayStatics::PlaySoundAtLocation(this, ImpactSound, Loc, MasterVolume);
}
void UKesselAudioComponent::OnReload()
{
    if (ReloadSound && GetOwner())
        UGameplayStatics::PlaySoundAtLocation(this, ReloadSound, GetOwner()->GetActorLocation(), MasterVolume);
}
void UKesselAudioComponent::OnCollision(float Force)  // seam currently uncalled — wire a caller later
{
    if (CollisionSound && GetOwner())
        UGameplayStatics::PlaySoundAtLocation(this, CollisionSound, GetOwner()->GetActorLocation(),
                                              FMath::Clamp(Force / CollisionRefImpulse, 0.1f, 1.f) * MasterVolume);
}
```
`PlaySoundAtLocation` is fire-and-forget (no handle to manage) — correct for transient one-shots and it
gets attenuation/panning for free, which already delivers the "bang in the correct ear" cue without any
plugin.

### 4.3 Looping engine note — a persistent `UAudioComponent`, pitched by speed
Create the loop once and keep a handle so we can re-pitch it every tick. Because `OnEngineRPM` has no caller,
**drive it from `OnTrack(Speed)` for M1**:

```cpp
// KesselAudioComponent.h
UPROPERTY(EditAnywhere) TObjectPtr<USoundBase> EngineLoop;        // machine_10_loop (Looping=true)
UPROPERTY(VisibleAnywhere) TObjectPtr<UAudioComponent> EngineAudio;

// Create lazily on first OnTrack (component has no BeginPlay override yet; adding one is fine in the code pass):
void UKesselAudioComponent::EnsureEngine()
{
    if (!EngineAudio && EngineLoop && GetOwner())
        EngineAudio = UGameplayStatics::SpawnSoundAttached(
            EngineLoop, GetOwner()->GetRootComponent(), NAME_None, FVector::ZeroVector,
            EAttachLocation::KeepRelativeOffset, /*bStopWhenAttachedToDestroyed*/ true,
            MasterVolume, /*PitchMultiplier*/ 1.f);  // EngineLoop asset has Looping ticked
}
void UKesselAudioComponent::OnTrack(float Speed)
{
    EnsureEngine();
    if (EngineAudio)
    {
        const float Norm  = FMath::Clamp(FMath::Abs(Speed) / MaxHullSpeed, 0.f, 1.f); // MaxHullSpeed ~ pawn top speed
        EngineAudio->SetPitchMultiplier(FMath::Lerp(0.85f, 1.6f, Norm)); // idle rumble -> revved
        EngineAudio->SetVolumeMultiplier(FMath::Lerp(0.6f, 1.0f, Norm) * MasterVolume);
    }
}
// When OnEngineRPM later gets a caller, move the pitch math there (RPM is the truer driver) and let
// OnTrack handle only the tread layer.
```

### 4.4 Buttoned vs open — `SetButtonedState(bool)` (mix change, no sample)
M1 minimal: duck the open-air ambience bed and (optionally) drop a one-line **interior LPF**. The full
submix/`AudioModulation` crossfade is the `AUDIO_ARCHITECTURE.md §5` job; for the thin pass, gating an
ambience `UAudioComponent`'s volume on `bButtoned` already sells "thunk — the world goes quiet."

---

## 5. Recommended imported-sound folder structure

Mirror the seam categories so a sound's home is obvious and the `EditAnywhere` slots are quick to fill:

```
/Game/Audio/
    Engine/      SW_EngineLoop          (machine_10_loop,  Looping=true)
    Weapon/      SW_MainGunFire         (explosion_close_short_ufx_2)
                 SW_Reload              (assault_rifle_reloading_mag_in)
                 SW_Racking             (pistol_racking_ufx_2)
    Impact/      SW_ShellImpact         (explosion_close_long_ufx_7)
                 SW_ImpactConcrete      (impact_concrete_ufx_1)
                 SW_CollisionThud       (break_wood_ufx_1)
    Ambient/     SW_Ambience_A          (ambience_d6_loop,  Looping=true)
                 SW_Ambience_B          (ambience_d22_loop, Looping=true)
                 SW_Footstep_Snow       (footstep_snow_ufx_3)
    UI/          SW_Click               (gui_click_1)
                 SW_Submit              (gui_submit_9)
    Music/       SW_Track_PitchBlack    (pitch black (middle_loop))   [optional]
```

Keep raw imports' source files in `RawAssets/Audio/` (already exists, currently empty) so re-import after a
swap is one click; the imported `USoundWave` assets live under `/Game/Audio/` above.

---

## 6. First audio-pass checklist (priority order)

The three that make the tank feel **alive** — do these first; each is independently shippable:

1. **[ ] Engine loop.** Import `machine_10_loop` → `/Game/Audio/Engine/SW_EngineLoop` (Looping). Wire the
   `EngineAudio` `UAudioComponent` and drive pitch/volume from `OnTrack(Speed)` (§4.3). *Biggest single
   win — silence → a running tank.*
2. **[ ] Gun fire.** Import `explosion_close_short_ufx_2` → `/Game/Audio/Weapon/SW_MainGunFire`. Assign to
   `FireSound`; `OnFire()` is already called (`KesselGunComponent.cpp:89`) — zero gameplay-code change.
3. **[ ] Shell impact.** Import `explosion_close_long_ufx_7` (or `impact_concrete_ufx_1`) →
   `/Game/Audio/Impact/SW_ShellImpact`. Assign to `ImpactSound`; `OnImpact()` is already called
   (`KesselProjectile.cpp:147`).

Then, cheap follow-ups (sources already on disk, two of three seams already called):
4. **[ ] Reload.** `assault_rifle_reloading_mag_in` → `ReloadSound` (`OnReload()` already called).
5. **[ ] Open-hatch ambience.** `ambience_d6_loop` as the exterior bed, gated by `SetButtonedState` (needs a
   caller added in the view component — a later code edit).
6. **[ ] Collision thud.** `impact_concrete_ufx_1` → `CollisionSound` (needs an `OnCollision` caller — later).

**Note on `OnEngineRPM` / `OnCollision` / `SetButtonedState`:** these three seams have **no caller** in the
current source. Their *sounds* are ready, but lighting them up requires adding the call site in the movement
/ armor / view components — a small code edit deferred to the implementation pass, not this read-only audit.

---

*Sources verified read-only on 2026-06-29 via `unzip -l` (no extraction) and Grep over `Source/Kessel`.
Asset archives: `C:\shop\ts_assets\`. Seam interface: `Source/Kessel/Public/KesselAudioComponent.h`.*
