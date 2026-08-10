# 06 — Atmosphere: General Winter Volumetric Fog, Emissive Scatter, Blizzard

Milestone 1 atmosphere spec for **Kessel**. Goal: cold blue-grey whiteout fog with short
view distance, into which isolated warm emissive sources (muzzle flash, burning wrecks,
streetlamps, the player's own gun/lamp) bleed red/amber, plus driving blizzard snow and
side-lit light shafts. Target HW: RTX 4090, so the volumetric grid can be pushed hard.

All engine APIs below are confirmed against the on-disk UE 5.8 source (CL 55116800).
Anchors are `file:line`. Items I could not confirm on-disk are tagged **ASSUMPTION**.

---

## 1. Component inventory (all confirmed in 5.8 core Engine — no plugin required)

| Need | Class | Confirmed at |
|---|---|---|
| Global height fog + volumetric fog | `UExponentialHeightFogComponent` | `Engine/Source/Runtime/Engine/Classes/Components/ExponentialHeightFogComponent.h:17` |
| Per-light scatter into fog | `VolumetricScatteringIntensity` on `ULightComponentBase` | `.../Components/LightComponentBase.h:120` |
| Localized density pockets (smoke, wreck plumes, low gulleys) | `ULocalFogVolumeComponent` | `.../Components/LocalFogVolumeComponent.h:13` |
| Authored dense volumes (heavy smoke columns / sparse VDB) | `UHeterogeneousVolumeComponent` | `.../Components/HeterogeneousVolumeComponent.h` (renderer: `Runtime/Renderer/Private/HeterogeneousVolumes/HeterogeneousVolumes.h`) |
| Blizzard particles + god-ray-ish sprites | Niagara | plugin present: `Engine/Plugins/FX/Niagara/Niagara.uplugin` |
| Directional light-shaft bloom | `bEnableLightShaftBloom` on `ULightComponent` | `.../Components/LightComponent.h:249`, `BloomScale:253` |

**Key 5.8 fact:** Local Fog Volume and Heterogeneous Volumes are **built into core Engine**
(not optional plugins). Heterogeneous Volumes still must be enabled via its project/render
settings + a Sparse Volume Texture or volumetric material; Local Fog Volume works out of the box
(gated by `r.LocalFogVolume.*`). For Milestone 1 the workhorse is Exponential Height Fog +
Volumetric Fog; Local Fog Volumes are the cheap localized-density tool; Heterogeneous Volumes are
optional polish for hero smoke columns and can be deferred.

---

## 2. Primary actor recipe — `BP_WinterAtmosphere` (or place ExponentialHeightFog actor)

One `AExponentialHeightFog` actor in the level. Its component drives the whole volumetric
solve. Properties below are the actual UPROPERTY names (set in BP defaults or via the C++
setters listed in §6).

### 2a. Enable + global density

- `bEnableVolumetricFog = true` — master switch for the froxel solve (`ExponentialHeightFogComponent.h:136`). Note in-source: Volumetric Fog ignores `StartDistance`, `FogMaxOpacity`, `FogCutoffDistance` (those only apply to the cheap analytic height fog).
- `FogDensity` — start **0.05–0.20** for a thick pocket (`:23`, UIMax hint is `.05` but it accepts higher; whiteout wants more). This is the global multiplier feeding extinction.
- `FogHeightFalloff` — **0.05–0.2** so the fog stays thick at tank/turret height and only thins well above the hull (`:30`). Lower = taller visible transition.
- `SecondFogData` — optional second layer (`:34`); use a denser low layer hugging the snow (high density, high falloff) plus the main layer for ground mist. Set its density 0 to disable.

### 2b. Cold blue-grey whiteout color

- `VolumetricFogAlbedo` (FColor, `:151`) — the particle reflectiveness; **this is the dominant color knob.** Set a cool desaturated blue-grey, e.g. **(R150, G170, B200)**-ish. Water/ice particles are near-white; pushing blue here gives the whiteout its cold cast while still scattering warm lights warmly (albedo tints scattered light, it does not force everything blue).
- `FogInscatteringLuminance` (FogInscatteringColor, `:43`) — ambient inscatter color/brightness. Cool grey-blue, low luminance. NOTE the in-source caveat at `:40`: when `r.SupportExpFogMatchesVolumetricFog = 1` this is ignored and the **Volumetric Fog Emissive** is used instead — so for volumetric look prefer driving ambient via `VolumetricFogEmissive`.
- `VolumetricFogEmissive` (FLinearColor, `:159`) — uniform light emitted by the fog itself ("more light the further you look through it"). Use a **very dim cold blue** to lift the whiteout floor so deep fog reads as luminous milky white rather than black. Keep low or distance reads flat.
- Pair with a **SkyLight** (cool ambient) and a weak overcast **DirectionalLight** for the diffuse snow-sky bounce; volumetric fog does not get static skylight, so use a Stationary/Movable SkyLight.

### 2c. Short view distance / whiteout falloff

- `VolumetricFogDistance` ("View Distance", `:169`, UIMin 1000 UIMax 10000 cm) — distance the froxel grid covers past start. For a claustrophobic pocket keep the grid **~6000–9000** (60–90 m) and let density do the occlusion; pushing higher just spreads froxels thinner.
- `VolumetricFogExtinctionScale` (`:163`, UIMin .1 UIMax 10) — multiplies absorption. Raise to **2–4** to crush visibility so enemy tanks loom out of nothing at short range. This is the main "how far can I see" dial alongside FogDensity.
- `VolumetricFogStartDistance` (`:175`) — keep small (e.g. 50–200 cm) so fog begins right at the gun mantlet/periscope glass.
- `VolumetricFogNearFadeInDistance` (`:181`) — short, so fog doesn't pop right on the lens.
- `FogMaxOpacity` (`:116`) only affects the analytic (non-volumetric) layer; leave ~1.

### 2d. Scattering directionality (the look of the warm smear)

- `VolumetricFogScatteringDistribution` (phase G, `:144`, UIMin -.9 UIMax .9) — **the most important knob for the reference look.**
  - `0.0` = isotropic (even glow halo around every light — soft, dreamy).
  - `0.7–0.9` = strong forward scatter (tight bright halos, dramatic shafts when a light faces you).
  - For warm point-light bloom in blizzard, **0.2–0.5** gives a generous-but-shaped halo. The header note at `:141` confirms: to get visible side light-shafts the distribution must be **closer to 0**. Start **0.3**.

---

## 3. Per-light emissive scatter — making muzzle flash / wrecks / lamps bleed into fog

`VolumetricScatteringIntensity` lives on `ULightComponentBase` so **every** light type
(point, spot, rect, directional) has it (`LightComponentBase.h:120`). Header doc: *"Intensity of
the volumetric scattering from this light. This scales Intensity and LightColor."* Default 1.0,
slider hint range 0.25–4.0 but it accepts higher.

Per-light setup for the warm sources:

| Source | Light type | Color | VolumetricScatteringIntensity | Notes |
|---|---|---|---|---|
| Muzzle flash | Point (pulsed) | warm amber-white ~(1.0,0.55,0.2) | **3–8** (brief) | Pulse intensity+scatter over ~0.06 s via timeline/Niagara light renderer. Huge transient fog bloom. |
| Burning wreck | Point + flicker | deep red-orange ~(1.0,0.3,0.08) | **2–4** | Animate intensity with noise for ember flicker; attach a Local Fog Volume (smoke plume, §4). |
| Streetlamp / pole light | Spot, downward | sodium amber ~(1.0,0.7,0.35) | **2–4** | Spot cone in fog gives the iconic cone-of-light shaft. Low phase-G fog shows the cone from the side. |
| Player gun lamp / interior CRT glow | Point/Spot, short range | CRT green or warm tungsten | **1–2** | Keep subtle; this is the warm interior key bleeding onto the glass. |
| Flare / signal | Point, moving | magnesium white→red | **4–6** | Dramatic moving halo through the blizzard. |

Critical interactions:
- Lights must be **Movable** (or Stationary) to inject into volumetric fog; static lights don't scatter into it (header `:154` note re. precomputed lighting).
- Because albedo (§2b) tints scattered light, a **cool fog albedo + warm light = warm core fading to cool halo edge** — exactly the reference's red/amber-into-blue smear. Do NOT over-blue the albedo or warm lights desaturate.
- `r.VolumetricFog.InjectShadowedLightsSeparately` (default 1, `VolumetricFog.cpp:102`) lets shadow-casting local lights inject correctly — keep on so the tank hull occludes its own muzzle bloom.
- `r.VolumetricFog.Emissive` (default 1, `:175`) must stay on for `VolumetricFogEmissive` to work.

---

## 4. Localized density — `ULocalFogVolumeComponent` (smoke pockets, wreck plumes, gulleys)

Confirmed `LocalFogVolumeComponent.h:13`. Base unit sphere is **500 uu radius**
(`GetBaseVolumeSize():74`); scale the actor transform to size it. It blends as
`Coverage = 1 - Transmittance` combined from a radial + height term, so you get soft spherical
edges plus internal height falloff.

Key props:
- `RadialFogExtinction` (Radial Fog Density, `:20`) — density at sphere center. **0.5–2.0** for a thick smoke ball.
- `HeightFogExtinction` (Height Fog Density, `:24`) + `HeightFogFalloff` (`:28`, default 1000) + `HeightFogOffset` (`:32`) — for plumes that pool low and thin upward.
- `FogPhaseG` (Scattering Distribution, `:36`, default 0.2, clamp 0–0.999) — same phase concept as global fog; raise for forward-scatter smoke catching the wreck fire.
- `FogAlbedo` (`:40`) — tint per volume. A burning-wreck plume can be a **dirty warm-grey/brown** while the world fog stays blue.
- `FogEmissive` (`:44`, luminance units) — give a wreck plume a faint warm self-glow so it reads lit even away from the fire light.
- `FogStartDistance` (`:48`) — clamped to `r.LocalFogVolume.GlobalStartDistance`.
- `FogSortPriority` (`:52`) — override distance sorting when volumes overlap.

Usage: parent a Local Fog Volume to each destroyed-tank/wreck actor and to scripted
smoke-screen events; parent small ones to low terrain to pool extra mist in shell craters.
These are far cheaper than Heterogeneous Volumes and integrate directly into the same fog solve.

**Heterogeneous Volumes** (`HeterogeneousVolumeComponent.h`) — defer to post-M1 polish. Use only
for a couple of hero smoke columns authored as Sparse Volume Textures / volumetric materials;
they are a separate, heavier render path (`Runtime/Renderer/Private/HeterogeneousVolumes/`).

---

## 5. Blizzard snow + light shafts (Niagara)

Niagara is present (`Engine/Plugins/FX/Niagara/Niagara.uplugin`). Build the blizzard as
camera-anchored Niagara systems (the player is always first-person inside the tank, so anchor
emitters to the camera/periscope, not the world).

Recommended emitter stack:
1. **Driving snow (near)** — GPU sprite emitter, large bounding box, spawned in a box around the camera, advected by a constant wind `Velocity` + curl-noise `VectorField`/`CurlNoiseForce` for gusting. High spawn rate, small motion-blurred sprites. Use a **Camera Offset** / kill-box so flakes recycle around the moving viewpoint.
2. **Streaking snow (fast, foreground)** — a few large stretched-sprite flakes very close to lens for parallax and speed sensation; respond to vehicle velocity (read via a parameter set from the pawn).
3. **Ground spindrift** — low GPU emitter blowing along the snow surface, screen-anchored to horizon, sells the wind.
4. **Frozen-lens accumulation** — optional: a post-process/material decal on the periscope glass, not Niagara, but driven by the same wind param.

**God-rays / light shafts:** two complementary mechanisms —
- *Engineered volumetric shafts* come for free from §2d: low `VolumetricFogScatteringDistribution`
  (~0.1–0.3) + spot/point lights with high `VolumetricScatteringIntensity` produce true
  in-fog cones and shafts. This is the primary, physically-consistent shaft source.
- *Screen-space directional shafts*: on the overcast `DirectionalLight` set
  `bEnableLightShaftBloom = true` (`LightComponent.h:249`) and tune `BloomScale` (`:253`) for a
  soft blizzard-sky bloom bleeding around hard occluders (wreck silhouettes). Keep subtle —
  overcast scene, not sunbeams.

Niagara can also emit its own **Light Renderer** components per particle (e.g. embers off a
wreck), which then scatter into the fog via their own `VolumetricScatteringIntensity` —
cheap way to get drifting warm motes glowing in the volume. **ASSUMPTION**: Niagara Light
Renderer exposes volumetric scattering intensity per-light as in prior 5.x; verify in the
Niagara light renderer properties when the editor is available.

---

## 6. C++ setters (for runtime tuning / quality presets later — no compile this phase)

On `UExponentialHeightFogComponent` (`ExponentialHeightFogComponent.h`):
`SetVolumetricFog(bool)` `:305`, `SetVolumetricFogScatteringDistribution(float)` `:308`,
`SetVolumetricFogExtinctionScale(float)` `:311`, `SetVolumetricFogAlbedo(FColor)` `:314`,
`SetVolumetricFogEmissive(FLinearColor)` `:317`, `SetVolumetricFogDistance(float)` `:320`,
`SetVolumetricFogStartDistance(float)` `:323`, `SetFogDensity(float)` `:251`,
`SetFogHeightFalloff(float)` `:287`, `SetFogInscatteringColor(FLinearColor)` `:257`.

On lights: `SetVolumetricScatteringIntensity(float)` (`LightComponent.h:292`),
`SetEnableLightShaftBloom(bool)` `:327`, `SetBloomScale(float)` `:330`.

On `ULocalFogVolumeComponent`: `SetRadialFogExtinction` `:56`, `SetHeightFogExtinction` `:58`,
`SetFogPhaseG` `:63`, `SetFogAlbedo` `:68`, `SetFogEmissive` `:70`.

These give the "graphics style must be TUNABLE" hook: expose a struct of these on the
GameMode / a settings asset and apply presets (e.g. "PS1 whiteout" vs "modern").

---

## 7. Volumetric-fog grid performance knobs (CVars, defaults from `VolumetricFog.cpp`)

All `ECVF_Scalability` so they can live in a scalability preset. Defaults are the on-disk values.

| CVar | Default | Anchor | Effect / tuning for this game |
|---|---|---|---|
| `r.VolumetricFog.GridPixelSize` | 16 | `:118` | XY froxel size in pixels. **Lower to 8** on the 4090 for crisp light shafts / sharp wreck-glow edges (4x cost). 16 is fine baseline. |
| `r.VolumetricFog.GridSizeZ` | 64 | `:126` | Depth slices. **Raise to 128** for smoother depth banding in thick fog (the whiteout exaggerates banding). |
| `r.VolumetricFog.DepthDistributionScale` | 32 | `:110` | Slice depth packing. Higher concentrates resolution near camera — good for short-view claustrophobic fog. |
| `r.VolumetricFog.TemporalReprojection` | 1 | `:134` | Keep on; halves visible noise. |
| `r.VolumetricFog.HistoryWeight` | 0.9 | `:150` | Higher = smoother but smearier under fast turret pans/vehicle motion. May need to **lower to ~0.8** because the camera moves a lot; trade for slightly more shimmer. |
| `r.VolumetricFog.Jitter` | 1 | `:142` | Temporal supersampling; keep on. |
| `r.VolumetricFog.LightScatteringSampleJitterMultiplier` | 0 | `:207` | Raise slightly if banding shows in dense fog. |
| `r.VolumetricFog.HistoryMissSupersampleCount` | 4 | `:158` | Reduces noise on camera cuts / fast pans (valid 1–16). Raise toward 8 given lots of turret motion. |
| `r.VolumetricFog.InjectShadowedLightsSeparately` | 1 | `:102` | Keep on so hull/wreck geometry shadows the warm light injection. |
| `r.VolumetricFog.LightSoftFading` | 0 | `:215` | Set ~1 to soften spot/rect edges (less flicker on streetlamp cones). |
| `r.VolumetricFog.Emissive` | 1 | `:175` | Must stay 1 for `VolumetricFogEmissive` floor-lift. |

**ASSUMPTION**: a master `r.VolumetricFog` enable CVar (default 1) exists as in prior 5.x but
was not located in this file; the per-component `bEnableVolumetricFog` is the authoritative
project toggle and is confirmed. The "Volumetric Fog" scalability group (in `BaseScalability.ini`)
also gates these — verify the ini when configuring quality presets.

Budget note: on a 4090 at 1080p–1440p, GridPixelSize 8 + GridSizeZ 128 is affordable; reserve a
lower preset (16 / 64) for the Linux/min-spec path.

---

## 8. Concrete starting recipe to hit the reference look

1. Place `ExponentialHeightFog`. `bEnableVolumetricFog = true`.
2. `FogDensity 0.1`, `FogHeightFalloff 0.1`, `VolumetricFogExtinctionScale 3.0`,
   `VolumetricFogDistance 8000`, `VolumetricFogStartDistance 100`.
3. `VolumetricFogAlbedo (150,170,200)` cool blue-grey; `VolumetricFogEmissive` very dim cold
   blue (e.g. linear (0.01,0.015,0.025)); `VolumetricFogScatteringDistribution 0.3`.
4. Movable SkyLight (cool, low intensity) + overcast Movable DirectionalLight (weak, slightly
   warm-grey) with `bEnableLightShaftBloom = true`, small `BloomScale`.
5. Warm point/spot lights on muzzle/wrecks/lamps with `VolumetricScatteringIntensity 2–4`
   (muzzle pulsed to 6+). Warm colors against the cool albedo → red/amber core melting into
   blue fog.
6. Local Fog Volumes parented to wrecks (warm-grey albedo, faint emissive) for plume density.
7. Camera-anchored Niagara blizzard (driving + streaking + spindrift) reading vehicle velocity.
8. Grid: `r.VolumetricFog.GridPixelSize 8`, `GridSizeZ 128`, `HistoryWeight 0.8`,
   `HistoryMissSupersampleCount 8`.
9. Tune `VolumetricFogScatteringDistribution` and per-light `VolumetricScatteringIntensity`
   together: distribution down for shafts, scatter up for halo intensity.

All values are starting points; every one is exposed via §6 setters for the tunable
graphics-style requirement.
