# Kessel — DYNAMIC ATMOSPHERE SYSTEM (Weather + Time + Light language)

**Team:** Atmosphere & Vibes
**Owns:** the full dynamic weather + day/night + ruin-lighting controller and its data model.
**Does NOT own:** ballistics, the PS1 `MasterRetro` post stack (only consumes it), actor placement
in the live map (the map-blockout team designs a *method*; Claude-main executes placement on the
single editor connection — see HARD CONSTRAINTS).

This builds directly on the **already-validated** atmosphere recipe (`docs/research/06-volumetric-fog-scatter.md`),
the **locked** M1 atmosphere row (`docs/MILESTONE_1_PLAN.md` §2), the **art intent**
(`docs/ART_BIBLE.md` §3–§4), and the **existing C++ scaffold** (`AKesselAtmosphereController` +
`UAtmospherePreset`). Nothing here contradicts those; it *extends static fog into a dynamic system*.

Pillar anchors: P1 deep-sim / abstracted (weather is a real state machine driving real light, presented
impressionistically), P3 sound-as-sensor (weather feeds the audio muffling param — handoff only), P4
"the cold, the dark, and the few warm lights" (this team **is** P4's engine), P5 always-inside-the-steel
(every effect is authored to read through an aperture; the night-light gameplay is the steel's one window
on the dark), with EVA as the deliberate exception (P5 carve-out).

---

## 0. One-paragraph vision

A single run is a **1.5–2.5 hour westward journey**, long enough that the **sky must move**: you start in
flat overcast afternoon and you will drive into dusk, into a blue-black night lit only by burning wrecks
and your own headlamp, and (if you survive long enough) into a thin grey dawn. The **weather is a living
state machine** layered on top: the General Winter whiteout thickens and thins — clear-ish → ground fog →
driving snow → full blizzard whiteout — in slow smooth blends you can *hear coming* before you see it.
Time and weather are **two orthogonal axes** multiplied together into one lit volume: night + blizzard is
the deadliest, most beautiful, most blind state in the game. At night your **headlamp and cabin glow are
both your salvation and your death warrant** — they punch a warm cone through the dark so you can read the
ground, and they are a beacon a T-34 crew can see from 600 m. Scattered through the ruins, **burning
wrecks and surviving lamps** are art, navigation landmarks, and threat cues all at once. The whole thing
is **data-driven** (curves + presets), runs over a **huge streamed world** on a froxel grid that follows
the camera, and degrades gracefully to a Linux/min-spec tier.

---

## 1. Two-axis model: Time × Weather → one lit volume

The system is the product of **two independent controllers** feeding **one set of render targets**:

```
                 TIME-OF-DAY axis                    WEATHER axis
        (continuous 0..24h, slow)            (discrete states + smooth blend)
                 |                                        |
   UTimeOfDaySettings (curves vs hour)      UWeatherPreset[] + transition rules
                 |                                        |
                 v                                        v
        sun/moon dir + intensity              fog density / extinction / albedo
        sky color + exposure (EV)             blizzard 0..1 / wind / muffle
        ambient/skylight tint                 cloud cover / lightshaft bloom
                 |                                        |
                 +------------------+---------------------+
                                    v
            AKesselWeatherTimeController  (the resolver / mixer)
                                    v
   ExponentialHeightFog + DirectionalLight(sun) + DirectionalLight(moon)
   + SkyLight + SkyAtmosphere/Sky + PostProcessVolume(exposure) + Niagara blizzard
   + MPC_Atmosphere (so materials & the retro stack can read time/weather)
```

**Why two axes, not one mega-preset:** a designer wants "midnight blizzard" and "midnight clear" and
"dusk light snow" without authoring N×M presets. Time supplies the *base canvas* (how dark, what color the
sky, where the key light is); weather supplies the *medium* (how much fog/snow, how short the view, how
muffled the sound). The controller multiplies them. Authoring cost is **O(hours curve) + O(weather
states)**, not O(product).

**Relationship to the existing scaffold:** `UWeatherPreset` is the **dynamic, blendable successor** to the
static `UAtmospherePreset`. `UAtmospherePreset` already holds `FogAlbedo / FogExtinction / FogDensity /
ViewDistance / BlizzardDensity / EmissiveScatterIntensity / FogScatteringColor`. We **keep that asset as
the "weather state snapshot" payload** and add a thin `UWeatherPreset` that *references* an
`UAtmospherePreset` plus the new weather-only fields (wind, snow rate, cloud cover, muffle, transition
timing). `AKesselWeatherTimeController` **subclasses or wraps** `AKesselAtmosphereController` so its proven
`ApplyPreset()` / fog-binding code is reused, not rewritten — it just calls `ApplyPreset()` with a
**runtime-lerped** preset each tick instead of once at BeginPlay.

---

## 2. C++ / asset architecture

### 2a. New & extended types

| Type | Base | Role | Notes |
|---|---|---|---|
| `AKesselWeatherTimeController` | `AKesselAtmosphereController` (→`AActor`) | The single live mixer. Owns time clock + weather FSM; ticks; lerps fog/light/sky/exposure each frame; drives MPC. | One per level. Reuses parent's `FogComponent` binding + `ApplyPreset()`. `PrimaryActorTick.bCanEverTick = true` (parent left it false — override). |
| `UTimeOfDaySettings` | `UPrimaryDataAsset` | Curves vs hour-of-day: sun pitch/yaw, sun+moon intensity & color, sky tint, fog-color bias, target EV100 (exposure), star/aurora opacity. | One per "season/region"; M1 ships one ("Stalingrad winter overcast"). |
| `UWeatherPreset` | `UPrimaryDataAsset` | One weather *state* (Clear/Fog/Snow/Blizzard/…): references an `UAtmospherePreset` snapshot + weather-only fields. | The blend target. |
| `FWeatherTransition` | `USTRUCT` | Allowed edges + min/max dwell + blend seconds + audio sting hook. | Drives the FSM; data, not code. |
| `UWeatherProfile` | `UPrimaryDataAsset` | The run's weather *director*: ordered/weighted list of states, transition table, "scripted beats" (e.g. force blizzard at map-west). | Lets the run be handcrafted, not pure-random. |
| `UAtmospherePreset` | `UPrimaryDataAsset` *(exists)* | Reused as the fog/scatter payload inside `UWeatherPreset`. | Add 2–3 fields (see §2c). |
| `MPC_Atmosphere` | `UMaterialParameterCollection` | Bridges runtime state → all materials (snow accumulation, frost-on-glass, wreck-glow, the retro stack reading "is it night"). | New asset. |

`UCurveFloat` / `UCurveLinearColor` assets back every "vs hour" channel in `UTimeOfDaySettings` (so artists
edit curves in-editor, no recompiles). `AKesselWeatherTimeController` exposes a **debug scrub** (set hour,
force weather) for tuning.

### 2b. Why subclass `AKesselAtmosphereController` rather than replace it

- Its fog-actor binding (`SetTargetFog` / `ResolveFogComponent` / `FogComponent` weak ptr) and
  `ApplyPreset()` setter-wall are exactly what the dynamic mixer needs to *push a frame's worth of state*.
  The TODO bodies in the scaffold `.cpp` get filled in once (in the parent) and the child reuses them.
- Keeps the "apply a static preset" path alive for cheap test maps / cutscenes / the M1 step-2 milestone
  ("cold whiteout fog renders") that predates the dynamic system.
- The child **adds** references to the sun/moon `ADirectionalLight`, `ASkyLight`,
  `APostProcessVolume` (or a `USkyAtmosphereComponent`), and the blizzard `UNiagaraComponent`, all as
  `EditInstanceOnly TWeakObjectPtr` set in-editor (same pattern as `FogActor`).

### 2c. Fields to add to `UAtmospherePreset` (small, additive)

```
// weather-medium fields the dynamic mixer blends (additive to existing 7 fields)
float  SnowfallRate        = 0.0;   // 0..1 → Niagara spawn-rate user param
float  WindStrength        = 0.2;   // 0..1 → blizzard advection + flake streak + audio howl
float  AudioMuffle         = 0.0;   // 0..1 → handoff to AUDIO team (snow muffles, P3)
float  CloudCover          = 0.7;   // 0..1 → directional light-shaft bloom + sky exposure droop
float  HeightFalloff       = 0.1;   // already discussed in research/06 §2a; expose it
```

`AudioMuffle` is a **handoff value only** — this team writes it into `MPC_Atmosphere`; the audio
architecture consumes it (sound-as-sensor pillar). We do not own the audio mix.

### 2d. `UWeatherPreset` shape

```
UAtmospherePreset*  Atmosphere;        // the fog/scatter payload (reused asset)
EWeatherState       State;             // Clear / LightSnow / Fog / Snow / Blizzard
float               MinDwellSeconds;   // never thrash; weather is slow
float               MaxDwellSeconds;
USoundBase*         ApproachCue;       // optional: distant wind rising before blizzard (P3 hook)
FLinearColor        SkyTintBias;       // small per-weather sky push layered over the time curve
```

`EWeatherState` enum: `Clear, Overcast, GroundFog, LightSnow, Snow, Blizzard` (M1 uses a subset; §6).

---

## 3. Time-of-day: sun/moon, sky, exposure

The clock is a `double CurrentHour` advanced by `DeltaSeconds * TimeScale`. A **full run ≈ one long
afternoon→night→dawn**, so `TimeScale` is small: at the design target a 2-hour run spans ~16 in-game hours
(13:00 → 05:00), i.e. **TimeScale ≈ 8 in-game-hours / real-hour ≈ 0.00222 hours/sec**. Tunable per run;
scripted "it's getting dark" beats can also *snap* the clock forward at map waypoints so darkness lands
where the level designer wants it (handcrafted-world directive).

Each tick the controller samples `UTimeOfDaySettings` curves at `CurrentHour` and applies:

| Channel | Driven object | API (confirmed-pattern) | Winter intent |
|---|---|---|---|
| **Sun direction** | sun `ADirectionalLight` | `SetActorRotation` / `SetWorldRotation` on the light | Low, raking arc — sun never climbs high in a Stalingrad winter; long blue shadows even at noon. Stays **below the fog's reach** so it reads as diffuse sky, not a disc. |
| **Sun intensity/color** | sun light | `SetIntensity` / `SetLightColor` (`ULightComponent`) | Weak warm-grey by day (`#7E8C97`-lit snow), → near-zero at night. |
| **Moon** | moon `ADirectionalLight` (2nd) | same | Cold blue key at night (`#1B2430` base + a faint cold rim). Cross-fades with sun across dusk/dawn so there's always exactly one "key," never two suns. |
| **Sky color / SkyAtmosphere** | `USkyAtmosphereComponent` or skydome material via MPC | sky params / MPC scalar+vector | Dusk = cold purple-blue (`#2A2740`, ART_BIBLE 3a); deep night = `#0A0E14` with a thin star/aurora wash (opacity from curve). |
| **Skylight ambient** | `ASkyLight` (Movable) | `SetIntensity` / recapture | Drops hard at night so the **only** fill is your own lights + wrecks (P4). Movable so it feeds volumetric fog (research/06 §2b). |
| **Exposure (EV100)** | `APostProcessVolume` auto-exposure min/max | `Settings.AutoExposureMinBrightness/MaxBrightness` (clamp toward a target EV from curve) | **Manual-ish exposure curve**, NOT free auto-exposure — auto-exposure would "fix" the darkness and kill the dread (P4). We clamp the eye-adaptation window per hour so night STAYS dark and your headlamp cone reads as genuine relief. |

**Exposure is gameplay, not a camera nicety.** If auto-exposure brightens the night, the headlamp stops
mattering and the beacon tradeoff (§4) evaporates. The curve keeps min/max EV close together and low at
night so the world is honestly dark and emissive sources pop.

---

## 4. Night: your lights are salvation AND a beacon (the core gameplay tie-in)

This is the headline interaction the directive calls for. It lives at the seam of atmosphere + stealth +
the crew-of-one workload.

**The two states of being lit:**

- **Headlamp ON / cabin lit:** a warm sodium cone (`#FF9B3D`) cuts a readable wedge of ground out of the
  black, scattering into the fog as a visible shaft (research/06 §3 — spot light, high
  `VolumetricScatteringIntensity`, low phase-G). You can see craters, wrecks, the road. **But** that cone
  and the cabin's CRT/tungsten spill (P4d) are an emissive event in an otherwise black volume — and the
  enemy AI gets a **detection bonus** proportional to your emitted-and-scattered luminance and the
  inverse of fog density. At night, lights-on doubles or triples your spot range to hostiles.
- **Lights OFF (running dark):** you are nearly invisible to distant enemies, but you are now driving a
  57-tonne tank by moonlight and muzzle-afterglow through a blizzard. Navigation collapses; you crawl;
  you risk driving into a ditch, a wreck, or a gun line you never saw.

**Implementation (data, no new subsystem owned here):**

- This team exposes, into `MPC_Atmosphere`, three read-only scalars the **stealth/AI** team consumes:
  `PlayerEmittedLuminance` (sum of player headlamp+cabin intensity, gated by on/off), `AmbientDarkness`
  (1 − normalized skylight, from the time curve), and `FogConcealment` (normalized fog extinction). The AI
  team's spot check is roughly `detect ∝ PlayerEmittedLuminance · AmbientDarkness · (1 − FogConcealment)`.
  We own the first three numbers' *atmospheric* inputs; AI owns the formula.
- **Light betrayal is symmetric:** the same `VolumetricScatteringIntensity` shaft that helps the player
  see is the visual the enemy "sees." So the art and the gameplay are the *same* light — no separate
  stealth volume. The fog both reveals (scatters your cone) and conceals (extinction). At high fog, lights
  help you *and* are swallowed before they reach the enemy → fog is the night-driver's friend. Clear night
  = lights reach far both ways → most dangerous to run lit.
- **Cabin light** (the `IA_CabinLight` input already in the M1 step-0 input kit) and **headlight**
  (`IA_Headlight`, also in the kit) are the two toggles. They're the player's primary night-risk levers.
  Muzzle flash is an *involuntary* beacon — every shot is a 0.06 s, scatter-6+ flash that screams your
  position; this couples combat to exposure for free.
- **EVA tie-in (P5 exception):** on foot with the Luger, you have at most a hand-held lamp / lit cigarette.
  Same rule, smaller: a lit cigarette in the dark is a tiny beacon (and the smoking heal mechanic forces
  you to stand still and glow while you regen). The atmosphere system feeds the same `PlayerEmittedLuminance`
  channel for the dismounted lamp.

**Honest scope:** the *full* asymmetric detection model is a later-milestone stealth feature. **M1 slice**
is: working headlamp + cabin spot lights that scatter correctly, the exposure curve that keeps night dark,
and the three MPC channels published (even if M1's stub enemy only reads a crude version). The *feel* of
"do I dare turn the light on" is achievable in M1 even with a simple AI multiplier.

---

## 5. Scattered ruin lights: art + navigation + threat cues

The handcrafted ruins are dotted with **persistent local light sources** that triple as mood, landmarks,
and information. The map-blockout team places the *anchors*; this team defines the **light archetypes** and
their behavior so placement is "drop archetype X here."

| Archetype | Light | Behavior | Reads as |
|---|---|---|---|
| **Burning wreck** | Point, flicker | `#D9531C`→`#B5300E` intensity noise (ember flicker); `VolumetricScatteringIntensity 2–4`; paired `ULocalFogVolumeComponent` smoke plume (research/06 §4, warm-grey albedo + faint emissive). | Death marker (something died here), navigation beacon visible through fog, and a *threat tell* — a fresh wreck means recent combat; the AI may still be near. Burns out over minutes (intensity curve → 0) so the world feels like it's *cooling*. |
| **Surviving lamp / pole** | Spot, downward | sodium `#FF9B3D`, steady, occasional buzz-flicker; cone-shaft in fog. | A still-standing landmark = a place man-made and navigable; rare warmth. Iconic `1afb2910`/`68427a0d` look. |
| **Red signal lamp / flare** | Point, may move | `#E83A3A`→halo `#7A1A22`; high scatter. | **Threat cue** — signal flares = enemy activity / artillery spotting. The hero red-into-snow scatter. |
| **Distant warm points** | low-cost point cluster | amber/red, receding into haze (`57bdc31c`). | "There is a position over there" — directional pull for the westward route or a warning to avoid. |
| **Aurora / starfield** | sky emissive only | opacity from time curve, clear-weather only | Rare beauty beat on a clear cold night; never on during blizzard. |

**Threat-cue grammar (consistent so the player learns it):** warm amber = neutral/landmark; flickering
orange = recent death/combat; **red = danger/enemy**. The player reads the dark by color. This is P3's
"sound is primary sensor" complemented by a disciplined *light* sensor for the eyes.

**Spawned-on-death wrecks reuse this exactly:** M1 step-10 already spawns "the burning-wreck emissive
light" when an enemy dies (MILESTONE_1_PLAN §3.10). That spawn should instantiate the **Burning-wreck
archetype** from this team's library, so combat deaths and pre-placed ruins use one code path and one look.

---

## 6. Weather state machine (clear → fog → snow → blizzard, smooth)

A small FSM on the controller, **data-driven by `UWeatherProfile`**:

- **States:** `Clear / Overcast / GroundFog / LightSnow / Snow / Blizzard` (each = a `UWeatherPreset`).
- **Transitions:** only allowed edges fire (you don't snap clear→blizzard; you pass through snow).
  Each edge has a **blend duration** (30–180 s) over which the controller **lerps every numeric field** of
  the current → target `UAtmospherePreset` (density, extinction, albedo, view distance, snowfall, wind,
  muffle, cloud cover) and cross-fades the Niagara blizzard spawn rate. Colors lerp in linear space.
- **Dwell:** each state holds `MinDwell..MaxDwell` (minutes) before it may transition — weather is *slow*;
  no strobing. A blizzard should feel like it *commits*.
- **Director beats:** `UWeatherProfile` can pin weather to **map progress** (handcrafted world): e.g.
  "force a building blizzard as the player nears the western edge" so the climax is a whiteout charge to
  friendly lines. Random-within-bounds elsewhere keeps runs varied (FTL-style).
- **Telegraphing (P3):** before a blizzard, fire `ApproachCue` (rising wind howl) and start nudging
  `WindStrength` up *seconds before* visibility drops — you hear and feel it coming, then it lands. This is
  the sound-as-sensor pillar doing weather.

**Blend correctness notes:** lerp `FogExtinction`/`FogDensity` on a perceptual (roughly exponential) curve,
not linear, so visibility doesn't "snap" near the thick end. View distance and extinction move *together*
(research/06: extinction is the real "how far can I see" dial). Never blend `bEnableVolumetricFog` — it's
always true (P4 non-negotiable).

---

## 7. Performance over a huge streamed world

The world is massive and streamed (World Partition assumed). The atmosphere must be **camera-relative and
O(1) in world size**:

- **Volumetric fog is already camera-relative.** The froxel grid covers `VolumetricFogDistance`
  (~6000–9000 uu) around the view, regardless of how big the world is — cost is fixed, not world-scale
  (research/06 §7). The huge map costs the fog **nothing extra**. This is the single most important perf
  fact and it's already validated.
- **Blizzard Niagara is camera-anchored** (research/06 §5) — spawn box follows the camera, flakes recycle
  in a kill-box. Particle count is constant, independent of world size.
- **Ruin lights stream with their actors.** Burning-wreck / lamp lights live on placed actors, so World
  Partition loads/unloads them by cell. **Cap simultaneous Movable shadow-casting volumetric lights** —
  these are the expensive ones (each injects into the froxel grid). Budget: keep ≲ 8–12 scatter-contributing
  movable lights live at once; distant ones drop `bAffectVolumetricFog` / shadowing and become cheap
  unshadowed points, or fold into the "distant warm points" cluster archetype. Provide an
  `AtmosphereLOD` per archetype (near = full flicker+shadow+plume; far = static point, no shadow, no plume).
- **The controller ticks lights it owns, not the world.** Sun/moon/sky/exposure are a handful of objects
  updated once per frame from curves — trivial. Weather lerp is a few dozen float lerps — trivial.
- **Two quality tiers via CVars** (already locked, MILESTONE_1_PLAN §2 / research/06 §7):
  - **4090 / high:** `r.VolumetricFog.GridPixelSize 8`, `GridSizeZ 128`, `HistoryWeight 0.8`,
    `HistoryMissSupersampleCount 8`.
  - **Linux / min-spec:** `GridPixelSize 16`, `GridSizeZ 64`, fewer live scatter lights, blizzard
    particle cap halved, smoke `ULocalFogVolume` plumes culled sooner.
  Wire these into a Scalability group ("Atmosphere") so one console line / settings toggle moves the tier.
- **History weight + fast camera:** the turret/vehicle move a lot → lower `HistoryWeight` to ~0.8 and raise
  `HistoryMissSupersampleCount` to cut smear on fast pans (research/06 §7). Weather *transitions* don't
  stress this (slow); the risk is fast turret slew, already mitigated.
- **Streaming hand-off:** when a fog/weather state changes, the controller updates the **single** level fog
  component — there is no per-cell fog to thrash. Local fog volumes (plumes) are the only streamed fog and
  they're cheap and few.

**Honest risk:** many flickering shadow-casting movable lights in dense ruins can spike the froxel inject
cost. Mitigation is the light cap + `AtmosphereLOD` above; verify with `stat GPU` / `r.VolumetricFog`
profiling once the editor is live (we can't profile on paper).

---

## 8. Concrete tunables (starting values)

### 8a. Time-of-day curves (`UTimeOfDaySettings`, sampled vs hour 0–24)

| Channel | 13:00 (start) | 17:00 dusk | 22:00 night | 05:00 dawn |
|---|---|---|---|---|
| Sun pitch (deg below/above horizon) | +12 | +1 | −30 (off) | −6→+2 |
| Sun intensity (lux-ish) | 8 | 2 | 0 | 1 |
| Sun color | `#9AA7B0` warm-grey | `#C98A5A` | — | `#8FA0B5` |
| Moon intensity | 0 | 0.3 | 1.2 | 0.4 |
| Moon color | — | `#37506E` | `#1B2430` cold | `#26405E` |
| Skylight intensity | 1.0 | 0.5 | 0.12 | 0.4 |
| Sky tint | `#AEBCC6` | `#2A2740` purple | `#0A0E14` | `#54626E` |
| Target EV100 (clamp center) | 13 | 11 | 8.5 | 11 |
| Star/aurora opacity (clear only) | 0 | 0.1 | 0.8 | 0.2 |

### 8b. Weather presets (the `UAtmospherePreset` payload per state)

| Field | Clear | Overcast | GroundFog | Snow | Blizzard |
|---|---|---|---|---|---|
| `FogDensity` | 0.03 | 0.06 | 0.12 | 0.12 | 0.20 |
| `FogExtinction` | 1.0 | 1.8 | 2.5 | 3.0 | 4.5 |
| `ViewDistance` (uu) | 9000 | 8000 | 7000 | 7000 | 6000 |
| `FogAlbedo` | (150,170,200) | (150,170,200) | (155,172,198) | (160,175,200) | (175,185,205) |
| `SnowfallRate` | 0 | 0 | 0 | 0.5 | 1.0 |
| `WindStrength` | 0.1 | 0.2 | 0.15 | 0.5 | 1.0 |
| `AudioMuffle` (→audio) | 0.0 | 0.1 | 0.3 | 0.5 | 0.85 |
| `CloudCover` | 0.2 | 0.8 | 0.7 | 0.85 | 1.0 |
| `EmissiveScatterIntensity` (base) | 2.0 | 2.5 | 3.0 | 3.0 | 3.5 |
| Phase-G (scatter distribution) | 0.25 | 0.3 | 0.3 | 0.3 | 0.35 |

`VolumetricFogStartDistance` stays ~100 uu in all states (fog at the lens). Blend duration 30–120 s;
GroundFog/Snow dwell 4–10 min; Blizzard dwell 3–8 min.

### 8c. Player light tunables (night gameplay)

| Param | Headlamp | Cabin glow | Muzzle (pulse) |
|---|---|---|---|
| Type | Spot, forward | Point/spot, interior | Point, transient |
| Color | `#FF9B3D` | `#FF9B3D`/`#33FF66` | `#FFE8B4` |
| `VolumetricScatteringIntensity` | 3–4 | 1–2 | 6+ |
| Detection weight (→AI) | high | low-med | spike (per shot) |
| Toggle | `IA_Headlight` | `IA_CabinLight` | involuntary on fire |

### 8d. Run clock

`TimeScale` default ≈ **0.00222 in-game-hours/sec** (2 h real ≈ 13:00→05:00). Tunable per `UWeatherProfile`;
allow scripted clock-snaps at map waypoints.

---

## 9. M1 slice vs later

### M1 (ship in Milestone 1 — extends MILESTONE_1_PLAN step 2 & 11)

- `AKesselWeatherTimeController` subclassing the existing `AKesselAtmosphereController`; **finish the
  scaffold TODOs** (`ResolveFogComponent`, `ApplyPreset` setter-wall) — that's already on the M1 critical
  path (step 2 "atmosphere first").
- A **single working day/night**: sun+moon directional cross-fade + skylight + exposure curve driven by
  `UTimeOfDaySettings`. Even one hand-authored arc (afternoon→night) sells the world.
- **Weather lerp between 2–3 states** (Overcast ↔ Snow ↔ Blizzard) with smooth blends — proves the FSM and
  the blend math. Full 6-state director can wait.
- **Headlamp + cabin lights** scattering correctly (the inputs already exist in the step-0 kit), and the
  **exposure clamp that keeps night dark**. Publish the three MPC detection channels (even if M1's stub
  enemy reads a crude version).
- **Burning-wreck archetype** wired to the step-10 enemy-death spawn (reuse, free win) + a couple of
  pre-placed lamp/wreck anchors in `L_KesselTest`.
- `MPC_Atmosphere` with `IsNight`, `FogConcealment`, `WindStrength`, `SnowAccumulation` so the retro stack
  and snow materials can read state.
- Two CVar quality tiers wired to a Scalability group.

### Later (post-M1, honest deferrals)

- Full 6-state `UWeatherProfile` director with map-progress-pinned beats and audio approach cues.
- The **complete asymmetric night-stealth detection model** (this team only supplies the atmospheric
  inputs; AI/stealth owns the formula and is a separate later milestone).
- **Frost/snow accumulation on periscope glass** and **snow build-up on the hull** driven by
  `MPC_Atmosphere` (`SnowAccumulation`, `WindStrength`) — material work, polish pass.
- `UHeterogeneousVolume` hero smoke columns (research/06 §4 explicitly defers these).
- Aurora/starfield skydome material, distant-warm-point clusters, signal-flare events.
- EVA hand-lamp / lit-cigarette beacon coupling (waits on the EVA + cigarette systems landing).
- Seasonal/regional `UTimeOfDaySettings` variants.

---

## 10. EXACT in-editor setup steps (the METHOD Claude-main executes)

Per HARD CONSTRAINTS this team does **not** touch the live editor. Below is the ordered method Claude-main
runs on the single MCP/editor connection. Assumes M1 step-1/2 done (module + `L_KesselTest` + one
`ExponentialHeightFog`).

1. **Create data assets** (Content Browser, `/Game/Atmosphere/`):
   - `UTimeOfDaySettings` → `DA_TimeOfDay_Winter`; create the backing `UCurveFloat`/`UCurveLinearColor`
     assets and key them to §8a.
   - `UAtmospherePreset` instances per weather state: `DA_Weather_Overcast`, `_Snow`, `_Blizzard`
     (M1 subset), fields per §8b.
   - `UWeatherPreset` wrappers referencing each `UAtmospherePreset`; `UWeatherProfile`
     `DA_WeatherProfile_M1` listing the 3 states + the allowed transitions (Overcast↔Snow↔Blizzard) with
     30–120 s blends and dwell ranges.
   - `MPC_Atmosphere` with scalars `IsNight, FogConcealment, WindStrength, SnowAccumulation,
     PlayerEmittedLuminance, AmbientDarkness` and vector `SkyTint`.
2. **Place the lighting rig** in `L_KesselTest`:
   - Sun `DirectionalLight` (**Movable**), Moon `DirectionalLight` (**Movable**), both feeding volumetric
     fog. `ASkyLight` (**Movable**, recapture-on-the-fly). One `USkyAtmosphere` *or* a skydome mesh with an
     MPC-driven material. One `APostProcessVolume` (Unbound) for the exposure clamp.
   - On the sun light enable `bEnableLightShaftBloom`, small `BloomScale` (research/06 §5).
3. **Place `AKesselWeatherTimeController`** in the level. In its details, set the `EditInstanceOnly`
   weak-ptr refs: `FogActor` (the existing fog), `SunLight`, `MoonLight`, `SkyLight`, `PostProcessVolume`,
   `BlizzardNiagara`, and the asset refs `TimeOfDaySettings`, `WeatherProfile`, `MPC_Atmosphere`.
4. **Volumetric fog component** on the existing `ExponentialHeightFog`: confirm `bEnableVolumetricFog=true`,
   set the research/06 §8 baseline (density 0.1, extinction 3, distance 8000, albedo (150,170,200),
   phase-G 0.3, start 100). The controller will overwrite these at runtime from the active preset — these
   are just the editor-time "looks right when not playing" defaults.
5. **Blizzard Niagara**: create the camera-anchored system (driving + streaking + spindrift, research/06
   §5) with user params `SpawnRate`, `WindStrength`, `VehicleVelocity`; expose them so the controller
   pushes `SnowfallRate`/`WindStrength` each frame.
6. **Ruin-light archetypes** as small Blueprint/actor presets (Burning-wreck = flicker point + LocalFog
   plume; Lamp = sodium spot; Red-signal = red point). Set their `VolumetricScatteringIntensity` per §8c.
   Hand the Burning-wreck archetype to the step-10 enemy-death spawn so combat reuses it.
7. **Player lights** on the pawn: headlamp Spot + cabin Point/Spot child components, bound to
   `IA_Headlight`/`IA_CabinLight`; set scatter intensities (§8c). Have the pawn write its emitted luminance
   into `MPC_Atmosphere.PlayerEmittedLuminance`.
8. **Exposure**: in the PostProcessVolume, enable exposure override; the controller writes
   `MinBrightness`/`MaxBrightness` from the time curve's target EV each frame (clamp window narrow at night).
9. **CVar tiers**: add an "Atmosphere" Scalability group (or a console-var apply on the controller) with the
   high vs min-spec froxel/blizzard values (§7).
10. **Verify (manual, R-M2 — graph/material read-back is weak this release):** PIE, scrub the controller's
    debug hour 13→05 and watch sun→moon cross-fade + exposure darken; force Overcast→Snow→Blizzard and
    watch the smooth fog/snow blend; toggle the headlamp at night and confirm the warm scatter cone +
    that `PlayerEmittedLuminance` moves in `MPC_Atmosphere`. Profile with `stat GPU` / `r.VolumetricFog`
    against the 8/12 live-light budget.

---

## 11. Cross-references & handoffs

- Engine recipe / confirmed APIs / CVars: `docs/research/06-volumetric-fog-scatter.md` (this doc is its
  *dynamic* superset).
- Art intent (palette, scatter language, per-view look): `docs/ART_BIBLE.md` §3–§4, §7.
- Locked atmosphere tech row + risks (R-M2 manual verify, two quality tiers): `docs/MILESTONE_1_PLAN.md` §2.
- Existing scaffold this extends: `Source/Kessel/{Public,Private}/KesselAtmosphereController.*`,
  `Source/Kessel/Public/AtmospherePreset.h`.
- **Handoff — AUDIO team:** we publish `AudioMuffle` + `WindStrength` + `ApproachCue` triggers into
  `MPC_Atmosphere`; they own the muffling/howl mix (P3 sound-as-sensor).
- **Handoff — AI/STEALTH team:** we publish `PlayerEmittedLuminance`, `AmbientDarkness`, `FogConcealment`;
  they own the detection formula (§4).
- **Handoff — MAP-BLOCKOUT team:** we define ruin-light archetypes; they specify anchor positions for
  Claude-main to place (§5).
- **Handoff — RETRO/POST team:** `MPC_Atmosphere.IsNight`/`FogConcealment` available so the PS1 stack and
  snow/frost materials can react; volumetric fog + emissive scatter stay ON at every `MasterRetro` (P4
  non-negotiable, ART_BIBLE §2).
